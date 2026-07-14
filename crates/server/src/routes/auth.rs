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
    if cowiki_db::users::find_by_name(&state.db, &input.name)
        .await?
        .is_some()
    {
        return Err(AppError::BadRequest("name already taken".into()));
    }

    let user =
        cowiki_db::users::create(&state.db, &input.name, input.email.as_deref(), None).await?;
    init_user_space(&state, &user).await?;

    Ok(Json(AuthResponse {
        api_key: user.api_key.clone(),
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
    let updated = cowiki_db::users::regenerate_api_key(&state.db, user.id).await?;

    Ok(Json(AuthResponse {
        api_key: updated.api_key.clone(),
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
    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=read:user%20user:email",
        state.config.auth.github_client_id,
        urlencoding::encode(&state.config.auth.github_redirect_uri),
    );
    Redirect::temporary(&url)
}

#[derive(Deserialize)]
pub struct GithubCallbackParams {
    pub code: String,
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

    let user = if let Some(existing) =
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
        existing
    } else {
        let user =
            cowiki_db::users::create(&state.db, &gh_user.login, email.as_deref(), None).await?;
        if let Err(e) = init_user_space(&state, &user).await {
            tracing::error!("failed to init user space: {:?}", e);
        }
        tracing::info!("created user {} via GitHub OAuth", user.name);
        user
    };

    // TODO: SECURITY — Passing the API key as a query parameter in the redirect URL is a risk:
    // it may be logged in server access logs, browser history, and HTTP Referer headers.
    // This should be replaced with a short-lived auth code exchange or secure HttpOnly cookie flow.
    // Not changing the flow now as it would break the current login integration.
    let redirect_url = format!(
        "{}/?api_key={}&user_name={}&user_id={}",
        state.config.frontend_url,
        user.api_key,
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
    use sha2::{Digest, Sha256};

    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("missing Authorization header".into()))?;

    let api_key = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("invalid Authorization format".into()))?;

    // Fast path: primary key (plaintext in users.api_key)
    if let Some(user) = cowiki_db::users::find_by_api_key(db, api_key).await? {
        return Ok(user);
    }

    // Fallback: secondary keys (SHA-256 hashed in api_keys.key_hash)
    let key_hash: String = {
        let mut hasher = Sha256::new();
        hasher.update(api_key.as_bytes());
        format!("{:x}", hasher.finalize())
    };

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

    let welcome = "---\ntype: Note\ntitle: \"Getting Started\"\ndescription: \"Welcome to your personal knowledge space.\"\n---\n\n# Getting Started\n\nWelcome to **CoWiki** — your personal knowledge space.\n\n## What you can do here\n\n- **Add sources** — paste text or URLs, CoWiki will compile them into wiki pages\n- **Organize** — create folders to keep your knowledge structured\n- **Search** — find anything with semantic search\n- **Collaborate** — join or create a Team Space to share knowledge with others\n";

    personal_repo
        .write_file(
            &branch,
            "getting-started.md",
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

        let team_welcome = "---\ntype: Note\ntitle: \"Team Space Home\"\ndescription: \"Welcome to the team's shared knowledge base.\"\n---\n\n# Team Space Home\n\nWelcome to the team! This is your shared knowledge base.\n\nUse the **+** button in the sidebar to add pages and folders.\n\n## Getting started\n\n1. **Add sources** — paste articles, docs, or notes\n2. **Compile** — AI turns your sources into structured wiki pages\n3. **Submit** — submit your drafts for team review\n4. **Review** — approve or request changes on teammates' submissions\n";

        team_repo
            .write_file(
                "main",
                "team-space-home.md",
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
