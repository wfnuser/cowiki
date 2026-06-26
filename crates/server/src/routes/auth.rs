use axum::extract::{Query, State};
use axum::response::Redirect;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub user: UserInfo,
    pub api_key: String,
}

/// A user's `name` doubles as their unique handle: it's how GitHub sign-in keys
/// them (`gh_user.login`) and how `@mentions` resolve them. Locally-registered
/// users must therefore be handle-shaped too — no spaces, GitHub-login charset —
/// so a `@name` token always parses back to exactly one user.
fn validate_handle(name: &str) -> Result<()> {
    let ok = (1..=39).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "name must be 1–39 characters, letters/digits/'-'/'_' only".into(),
        ))
    }
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

/// Register a new user, returns user info + API key
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(input): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>> {
    validate_handle(&input.name)?;
    if cowiki_db::users::find_by_name(&state.db, &input.name)
        .await?
        .is_some()
    {
        return Err(AppError::BadRequest("name already taken".into()));
    }

    let (user, raw_key) =
        cowiki_db::users::create(&state.db, &input.name, input.email.as_deref(), None).await?;
    init_user_space(&state, &user).await?;

    Ok(Json(AuthResponse {
        api_key: raw_key,
        user: UserInfo {
            id: user.id.to_string(),
            name: user.name,
            email: user.email,
            avatar_url: None,
        },
    }))
}

/// Get current user info (from API key)
pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<UserInfo>> {
    let user = extract_user(&state.db, &headers).await?;
    Ok(Json(UserInfo {
        id: user.id.to_string(),
        name: user.name,
        email: user.email,
        avatar_url: None,
    }))
}

/// Regenerate API key
pub async fn regenerate_key(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<AuthResponse>> {
    let user = extract_user(&state.db, &headers).await?;
    let (updated, raw_key) = cowiki_db::users::regenerate_api_key(&state.db, user.id).await?;

    Ok(Json(AuthResponse {
        api_key: raw_key,
        user: UserInfo {
            id: updated.id.to_string(),
            name: updated.name,
            email: updated.email,
            avatar_url: None,
        },
    }))
}

// ── GitHub OAuth ──

pub async fn github_login(State(state): State<Arc<AppState>>) -> Redirect {
    // CSRF protection (#59): a random, single-use `state` nonce that the callback
    // must echo back. Stored in-memory with a TTL (single-instance server).
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    {
        let mut states = state.oauth_states.lock().unwrap();
        let now = std::time::Instant::now();
        states.retain(|_, t| now.duration_since(*t).as_secs() < OAUTH_STATE_TTL_SECS);
        states.insert(nonce.clone(), now);
    }
    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=read:user%20user:email&state={}",
        state.config.auth.github_client_id,
        urlencoding::encode(&state.config.auth.github_redirect_uri),
        nonce,
    );
    Redirect::temporary(&url)
}

const OAUTH_STATE_TTL_SECS: u64 = 600;

#[derive(Deserialize)]
pub struct GithubCallbackParams {
    pub code: String,
    pub state: Option<String>,
}

#[derive(Deserialize)]
struct GithubTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GithubUser {
    login: String,
    email: Option<String>,
    // Retained for JSON deserialization compatibility with GitHub API response.
    #[serde(default, rename = "avatar_url")]
    _avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GithubEmail {
    email: String,
    primary: bool,
}

pub async fn github_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GithubCallbackParams>,
) -> Result<Redirect> {
    // Verify the CSRF nonce minted at login: present, known, fresh, single-use.
    let nonce = params
        .state
        .as_deref()
        .ok_or_else(|| AppError::Unauthorized("missing oauth state".into()))?;
    {
        let mut states = state.oauth_states.lock().unwrap();
        let fresh = states
            .remove(nonce)
            .map(|t| t.elapsed().as_secs() < OAUTH_STATE_TTL_SECS)
            .unwrap_or(false);
        if !fresh {
            return Err(AppError::Unauthorized(
                "invalid or expired oauth state".into(),
            ));
        }
    }
    let client = reqwest::Client::new();

    let token_resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": state.config.auth.github_client_id,
            "client_secret": state.config.auth.github_client_secret,
            "code": params.code,
        }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github token exchange failed: {e}")))?
        .json::<GithubTokenResponse>()
        .await
        .map_err(|e| AppError::Internal(format!("github token parse failed: {e}")))?;

    let gh_user = client
        .get("https://api.github.com/user")
        .header(
            "Authorization",
            format!("Bearer {}", token_resp.access_token),
        )
        .header("User-Agent", "cowiki")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github user fetch failed: {e}")))?
        .json::<GithubUser>()
        .await
        .map_err(|e| AppError::Internal(format!("github user parse failed: {e}")))?;

    let email = if gh_user.email.is_some() {
        gh_user.email.clone()
    } else {
        let emails = match client
            .get("https://api.github.com/user/emails")
            .header(
                "Authorization",
                format!("Bearer {}", token_resp.access_token),
            )
            .header("User-Agent", "cowiki")
            .send()
            .await
        {
            Ok(r) => r.json::<Vec<GithubEmail>>().await.ok(),
            Err(_) => None,
        };
        emails.and_then(|list| list.into_iter().find(|e| e.primary).map(|e| e.email))
    };

    let (user, raw_key) = if let Some(existing) =
        cowiki_db::users::find_by_name(&state.db, &gh_user.login).await?
    {
        // Update email if the existing user has none but GitHub provides one
        if existing.email.is_none() {
            if let Some(ref gh_email) = email {
                if let Err(e) =
                    cowiki_db::users::update_email(&state.db, existing.id, gh_email).await
                {
                    tracing::warn!("failed to update email for user {}: {:?}", existing.name, e);
                }
            }
        }
        // The primary key is stored hashed and can't be recovered — mint a fresh
        // secondary key for this sign-in instead of rotating the primary (which
        // would log out the user's other sessions/agents). Revoke the previous
        // "GitHub sign-in" key first so repeated logins don't accumulate keys
        // unboundedly — each sign-in supersedes the last (one active login key).
        let _ = cowiki_db::api_keys::revoke_by_name(&state.db, existing.id, "GitHub sign-in").await;
        let minted = cowiki_db::api_keys::create(&state.db, existing.id, "GitHub sign-in").await?;
        (existing, minted.raw_key)
    } else {
        let (user, raw_key) =
            cowiki_db::users::create(&state.db, &gh_user.login, email.as_deref(), None).await?;
        if let Err(e) = init_user_space(&state, &user).await {
            tracing::error!("failed to init user space: {:?}", e);
        }
        tracing::info!("created user {} via GitHub OAuth", user.name);
        (user, raw_key)
    };

    // The credential travels in the URL *fragment* (#59): fragments never reach
    // servers, access logs, or Referer headers — unlike the old ?api_key= query.
    // (A full HttpOnly-cookie session flow is the account-system redesign, #12.)
    let redirect_url = format!(
        "{}/#api_key={}&user_name={}&user_id={}",
        state.config.frontend_url,
        raw_key,
        urlencoding::encode(&user.name),
        user.id,
    );
    Ok(Redirect::temporary(&redirect_url))
}

// ── Helpers ──

pub async fn extract_user(
    db: &sqlx::PgPool,
    headers: &axum::http::HeaderMap,
) -> Result<cowiki_db::users::User> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("missing Authorization header".into()))?;

    let api_key = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("invalid Authorization format".into()))?;

    // Primary key: stored as SHA-256 (find_by_api_key hashes the raw token)
    if let Some(user) = cowiki_db::users::find_by_api_key(db, api_key).await? {
        return Ok(user);
    }

    // Fallback: secondary keys (SHA-256 hashed in api_keys.key_hash). Use the same
    // canonical hash function as the primary key so the two paths can't drift.
    let key_hash = cowiki_db::users::hash_api_key(api_key);

    if let Some((_key, user_id)) = cowiki_db::api_keys::find_by_key_hash(db, &key_hash).await? {
        // Touch last_used_at (fire-and-forget)
        let db_clone = db.clone();
        let hash_clone = key_hash.clone();
        tokio::spawn(async move {
            let _ = cowiki_db::api_keys::touch_last_used(&db_clone, &hash_clone).await;
        });

        return cowiki_db::users::find_by_id(db, user_id)
            .await?
            .ok_or_else(|| AppError::Unauthorized("invalid API key".into()));
    }

    Err(AppError::Unauthorized("invalid API key".into()))
}

pub fn user_branch(user: &cowiki_db::users::User) -> String {
    format!("user/{}", user.id)
}

/// Initialize a new user's space: per-workspace repos + welcome pages
async fn init_user_space(state: &crate::AppState, user: &cowiki_db::users::User) -> Result<()> {
    let branch = user_branch(user);

    // 1. Create personal workspace in DB
    let personal_slug = format!("personal-{}", &user.id.to_string()[..8]);
    cowiki_db::workspaces::create(
        &state.db,
        &format!("{}'s Space", user.name),
        &personal_slug,
        "private",
        user.id,
    )
    .await
    .ok();

    // 2. Init personal repo + user branch + welcome page
    let personal_repo = state
        .repo_manager
        .get(&personal_slug)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    personal_repo
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let welcome = "---\ntitle: \"Getting Started\"\nsummary: \"Welcome to your personal knowledge space.\"\nkind: concept\n---\n\n# Getting Started\n\nWelcome to **CoWiki** — your personal knowledge space.\n\n## What you can do here\n\n- **Add sources** — paste text or URLs, CoWiki will compile them into wiki pages\n- **Organize** — create folders to keep your knowledge structured\n- **Search** — find anything with semantic search\n- **Collaborate** — join or create a Team Space to share knowledge with others\n";

    personal_repo
        .write_file(
            &branch,
            "wiki/getting-started.md",
            welcome.as_bytes(),
            "init: getting started",
            &user.name,
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 3. Create default Team Space ("General") in DB
    let team_slug = format!("general-{}", &user.id.to_string()[..8]);
    if cowiki_db::workspaces::create(&state.db, "General", &team_slug, "public", user.id)
        .await
        .is_ok()
    {
        // 4. Init team repo + welcome page on main
        let team_repo = state
            .repo_manager
            .get(&team_slug)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let team_welcome = "---\ntitle: \"Team Space Home\"\nsummary: \"Welcome to the team's shared knowledge base.\"\nkind: overview\n---\n\n# Team Space Home\n\nWelcome to the team! This is your shared knowledge base.\n\nUse the **+** button in the sidebar to add pages and folders.\n\n## Getting started\n\n1. **Add sources** — paste articles, docs, or notes\n2. **Compile** — AI turns your sources into structured wiki pages\n3. **Submit** — submit your drafts for team review\n4. **Review** — approve or request changes on teammates' submissions\n";

        team_repo
            .write_file(
                "main",
                "wiki/team-space-home.md",
                team_welcome.as_bytes(),
                "init: team space home",
                "cowiki",
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Create user branch in team repo too
        team_repo
            .ensure_user_branch(&user.id.to_string())
            .map_err(|e| AppError::Internal(e.to_string()))?;

        tracing::info!("created team space '{}' for user {}", team_slug, user.name);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_handle;

    #[test]
    fn accepts_github_login_shaped_names() {
        for ok in ["wfnuser", "Dead-pool_mine", "a", "x9", &"a".repeat(39)] {
            assert!(validate_handle(ok).is_ok(), "should accept {ok:?}");
        }
    }

    #[test]
    fn rejects_spaces_empty_and_overlong() {
        for bad in ["John Doe", "", "  ", "name!", "емоджи", &"a".repeat(40)] {
            assert!(validate_handle(bad).is_err(), "should reject {bad:?}");
        }
    }
}
