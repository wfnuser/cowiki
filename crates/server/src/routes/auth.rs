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
    if cowiki_db::users::find_by_name(&state.db, &input.name).await?.is_some() {
        return Err(AppError::BadRequest("name already taken".into()));
    }

    let user = cowiki_db::users::create(&state.db, &input.name, input.email.as_deref(), None).await?;

    // Create user branch + personal space
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

// ── GitHub OAuth ──────────────────────────────────────────────

/// Redirect user to GitHub authorization page
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
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GithubEmail {
    email: String,
    primary: bool,
}

/// GitHub OAuth callback — exchange code for token, get user info, create/find user
pub async fn github_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GithubCallbackParams>,
) -> Result<Redirect> {
    let client = reqwest::Client::new();

    // 1. Exchange code for access token
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

    // 2. Get GitHub user info
    let gh_user = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token_resp.access_token))
        .header("User-Agent", "cowiki")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github user fetch failed: {e}")))?
        .json::<GithubUser>()
        .await
        .map_err(|e| AppError::Internal(format!("github user parse failed: {e}")))?;

    // 3. Get email if not public
    let email = if gh_user.email.is_some() {
        gh_user.email.clone()
    } else {
        let emails = client
            .get("https://api.github.com/user/emails")
            .header("Authorization", format!("Bearer {}", token_resp.access_token))
            .header("User-Agent", "cowiki")
            .send()
            .await
            .ok()
            .and_then(|r| futures::executor::block_on(r.json::<Vec<GithubEmail>>()).ok());
        emails.and_then(|list| list.into_iter().find(|e| e.primary).map(|e| e.email))
    };

    // 4. Find or create user
    let user = if let Some(existing) = cowiki_db::users::find_by_name(&state.db, &gh_user.login).await? {
        existing
    } else {
        let user = cowiki_db::users::create(&state.db, &gh_user.login, email.as_deref(), None).await?;
        if let Err(e) = init_user_space(&state, &user).await {
            tracing::error!("failed to init user space: {:?}", e);
        }
        tracing::info!("created user {} via GitHub OAuth", user.name);
        user
    };

    // 5. Redirect to frontend with API key
    let redirect_url = format!(
        "{}/?api_key={}&user_name={}&user_id={}",
        state.config.frontend_url,
        user.api_key,
        urlencoding::encode(&user.name),
        user.id,
    );
    Ok(Redirect::temporary(&redirect_url))
}

// ── Helpers ──────────────────────────────────────────────

/// Extract user from Authorization header (Bearer <api_key>)
pub async fn extract_user(
    db: &sqlx::PgPool,
    headers: &axum::http::HeaderMap,
) -> Result<cowiki_db::users::User> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("missing Authorization header".into()))?;

    let api_key = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::BadRequest("invalid Authorization format".into()))?;

    cowiki_db::users::find_by_api_key(db, api_key)
        .await?
        .ok_or_else(|| AppError::BadRequest("invalid API key".into()))
}

/// Helper: get the user's personal branch name
pub fn user_branch(user: &cowiki_db::users::User) -> String {
    format!("user/{}", user.id)
}

/// Initialize a new user's space: git branch + personal workspace + welcome page
async fn init_user_space(state: &crate::AppState, user: &cowiki_db::users::User) -> Result<()> {
    let branch = user_branch(user);

    // 1. Create Git branch
    state.wiki_repo
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 2. Create personal workspace in DB
    let slug = format!("personal-{}", &user.id.to_string()[..8]);
    cowiki_db::workspaces::create(&state.db, &format!("{}'s Space", user.name), &slug, "private", user.id)
        .await
        .ok(); // Ignore if already exists

    // 3. Write a welcome page
    let welcome = r#"---
title: "Welcome to CoWiki"
summary: "Getting started with your personal knowledge space."
kind: concept
---

# Welcome to CoWiki

This is your personal knowledge space. Here are a few things you can do:

## Ingest Sources

Add URLs, text, or files as sources. CoWiki will compile them into structured wiki pages.

## Compile

Click **Compile** to transform your sources into interlinked wiki pages using AI.

## Submit to Teamspace

When you're ready, submit your pages to a shared teamspace for team review.

## Search

Use semantic search to find knowledge across your spaces.

Happy building!
"#;

    state.wiki_repo
        .write_file(&branch, "wiki/welcome.md", welcome.as_bytes(), "init: welcome page", &user.name)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(())
}
