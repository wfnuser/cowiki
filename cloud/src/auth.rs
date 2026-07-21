use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use subtle::ConstantTimeEq;
use time::OffsetDateTime;
use url::Url;

use crate::AppState;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::model::User;

pub const OAUTH_STATE_TTL: Duration = Duration::from_secs(600);
const DESKTOP_EXCHANGE_TTL_SECONDS: i64 = 60;

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub user: User,
    pub raw_api_key: String,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(&parts.headers)?.to_string();
        let user = db::authenticate_api_key(&state.pool, &token, &state.config.token_pepper)
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(Self {
            user,
            raw_api_key: token,
        })
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/github", get(github_start))
        .route("/api/auth/github/callback", get(github_callback))
        .route("/api/auth/desktop/exchange", post(desktop_exchange))
        .route("/api/me", get(me))
        .route("/api/auth/logout", post(logout))
}

#[derive(Debug, Deserialize)]
struct GithubStartQuery {
    client: Option<String>,
    callback: Option<String>,
}

async fn github_start(
    State(state): State<AppState>,
    Query(query): Query<GithubStartQuery>,
) -> AppResult<Redirect> {
    let callback = match query.client.as_deref() {
        Some("desktop") => Some(
            validate_desktop_callback(
                query
                    .callback
                    .as_deref()
                    .ok_or_else(|| AppError::BadRequest("desktop callback is required".into()))?,
            )
            .map_err(AppError::BadRequest)?,
        ),
        None | Some("web") => None,
        Some(_) => return Err(AppError::BadRequest("unsupported OAuth client".into())),
    };

    let raw_state = random_secret("cw_state_");
    let state_hash = api_key_hash(&raw_state, &state.config.token_pepper);
    db::create_oauth_state(
        &state.pool,
        &state_hash,
        callback.as_ref().map(Url::as_str),
        OffsetDateTime::now_utc() + time::Duration::seconds(OAUTH_STATE_TTL.as_secs() as i64),
    )
    .await?;

    let redirect_uri = state
        .config
        .public_origin
        .join("/api/auth/github/callback")
        .map_err(|error| AppError::Internal(error.to_string()))?;
    let mut authorize = Url::parse("https://github.com/login/oauth/authorize")
        .map_err(|error| AppError::Internal(error.to_string()))?;
    authorize
        .query_pairs_mut()
        .append_pair("client_id", &state.config.github_client_id)
        .append_pair("redirect_uri", redirect_uri.as_str())
        .append_pair("scope", "read:user")
        .append_pair("state", &raw_state);
    Ok(Redirect::temporary(authorize.as_str()))
}

#[derive(Debug, Deserialize)]
struct GithubCallbackQuery {
    code: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GithubTokenResponse {
    access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubUserResponse {
    id: u64,
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

async fn github_callback(
    State(state): State<AppState>,
    Query(query): Query<GithubCallbackQuery>,
) -> AppResult<Redirect> {
    let state_hash = api_key_hash(&query.state, &state.config.token_pepper);
    let callback = db::consume_oauth_state(&state.pool, &state_hash)
        .await?
        .ok_or_else(|| AppError::BadRequest("OAuth state is invalid or expired".into()))?;
    let redirect_uri = state
        .config
        .public_origin
        .join("/api/auth/github/callback")
        .map_err(|error| AppError::Internal(error.to_string()))?;
    let http = reqwest::Client::new();
    let token = http
        .post("https://github.com/login/oauth/access_token")
        .header(header::ACCEPT.as_str(), "application/json")
        .json(&serde_json::json!({
            "client_id": state.config.github_client_id,
            "client_secret": state.config.github_client_secret,
            "code": query.code,
            "redirect_uri": redirect_uri.as_str(),
        }))
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("GitHub token exchange failed: {error}")))?
        .error_for_status()
        .map_err(|error| AppError::Internal(format!("GitHub rejected OAuth code: {error}")))?
        .json::<GithubTokenResponse>()
        .await
        .map_err(|error| AppError::Internal(format!("invalid GitHub token response: {error}")))?
        .access_token
        .ok_or_else(|| AppError::Internal("GitHub returned no access token".into()))?;

    let profile = http
        .get("https://api.github.com/user")
        .bearer_auth(token)
        .header(header::USER_AGENT.as_str(), "cowiki-cloud")
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("GitHub profile request failed: {error}")))?
        .error_for_status()
        .map_err(|error| AppError::Internal(format!("GitHub rejected profile request: {error}")))?
        .json::<GithubUserResponse>()
        .await
        .map_err(|error| AppError::Internal(format!("invalid GitHub profile: {error}")))?;
    let github_id = i64::try_from(profile.id)
        .map_err(|_| AppError::Internal("GitHub user id is out of range".into()))?;
    let display_name = profile.name.as_deref().unwrap_or(&profile.login);
    let user = db::upsert_github_user(
        &state.pool,
        github_id,
        &profile.login,
        display_name,
        profile.avatar_url.as_deref(),
    )
    .await?;
    let exchange_code =
        db::create_desktop_exchange_code(&state.pool, user.id, &state.config.token_pepper).await?;

    let mut target = if let Some(ref callback) = callback {
        Url::parse(callback).map_err(|error| AppError::Internal(error.to_string()))?
    } else {
        state
            .config
            .public_origin
            .join("/auth/callback")
            .map_err(|error| AppError::Internal(error.to_string()))?
    };
    if callback.is_some() {
        target.query_pairs_mut().append_pair("code", &exchange_code);
    } else {
        target.set_fragment(Some(&format!("auth_code={exchange_code}")));
    }
    Ok(Redirect::temporary(target.as_str()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopExchangeRequest {
    code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopExchangeResponse {
    api_key: String,
    user_name: String,
    user_id: uuid::Uuid,
}

async fn desktop_exchange(
    State(state): State<AppState>,
    Json(request): Json<DesktopExchangeRequest>,
) -> AppResult<Response> {
    let issued =
        db::exchange_desktop_code(&state.pool, request.code.trim(), &state.config.token_pepper)
            .await?
            .ok_or_else(|| AppError::BadRequest("desktop code is invalid or expired".into()))?;
    Ok(no_store_json(DesktopExchangeResponse {
        api_key: issued.api_key,
        user_name: issued.user.display_name,
        user_id: issued.user.id,
    }))
}

async fn me(user: AuthenticatedUser) -> Response {
    no_store_json(serde_json::json!({ "user": user.user }))
}

async fn logout(State(state): State<AppState>, user: AuthenticatedUser) -> AppResult<StatusCode> {
    if db::revoke_api_key(&state.pool, &user.raw_api_key, &state.config.token_pepper).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::Unauthorized)
    }
}

fn no_store_json<T: Serialize>(value: T) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn bearer_token(headers: &HeaderMap) -> AppResult<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
        .ok_or(AppError::Unauthorized)
}

pub fn validate_desktop_callback(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|error| format!("invalid desktop callback: {error}"))?;
    let valid = url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port().is_some()
        && url.path() == "/auth/callback"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    valid
        .then_some(url)
        .ok_or_else(|| "desktop callback must be http://127.0.0.1:<port>/auth/callback".into())
}

pub fn random_secret(prefix: &str) -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    format!(
        "{prefix}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    )
}

pub fn api_key_hash(value: &str, pepper: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(pepper.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hasher.finalize().into()
}

pub fn verify_api_key(value: &str, pepper: &str, expected: &[u8]) -> bool {
    let actual = api_key_hash(value, pepper);
    actual.as_slice().ct_eq(expected).into()
}

pub const fn desktop_exchange_ttl_seconds() -> i64 {
    DESKTOP_EXCHANGE_TTL_SECONDS
}
