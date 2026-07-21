use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthenticatedUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::git_repo::{GitRepoError, GitRepoStore};
use crate::model::MemberRole;

const MAX_GIT_REQUEST_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitService {
    UploadPack,
    ReceivePack,
}

#[derive(Debug, Clone)]
pub struct GitHttpRequest {
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub bootstrap: bool,
}

#[derive(Debug, Clone)]
pub struct GitHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/git/{repository}/{*path}", any(git_http_handler))
}

async fn git_http_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((repository, path)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Response> {
    let space_id = parse_repository_segment(&repository)?;
    let role = db::member_role(&state.pool, space_id, user.user.id)
        .await?
        .ok_or(AppError::NotFound)?;
    let service = classify_request(&method, &path, query.as_deref())
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    authorize_service(role, service).map_err(|_| AppError::Forbidden)?;
    if body.len() > MAX_GIT_REQUEST_BYTES {
        return Err(AppError::BadRequest("Git request is too large".into()));
    }
    state.repos.ensure_space(space_id).map_err(git_internal)?;
    let bootstrap = service == GitService::ReceivePack
        && role == MemberRole::Owner
        && !state.repos.main_exists(space_id).map_err(git_internal)?;
    let request = GitHttpRequest {
        method,
        path,
        query,
        headers,
        body: body.to_vec(),
        bootstrap,
    };

    let response = if service == GitService::ReceivePack {
        let lock = state.repos.space_lock(space_id).map_err(git_internal)?;
        let _guard = lock.lock().await;
        run_git_http_backend(&state.repos, space_id, user.user.id, role, request)
            .await
            .map_err(git_internal)?
    } else {
        run_git_http_backend(&state.repos, space_id, user.user.id, role, request)
            .await
            .map_err(git_internal)?
    };
    let mut output = (response.status, response.body).into_response();
    *output.headers_mut() = response.headers;
    Ok(output)
}

fn parse_repository_segment(value: &str) -> AppResult<Uuid> {
    let id = value
        .strip_suffix(".git")
        .ok_or_else(|| AppError::BadRequest("invalid Git repository path".into()))?;
    Uuid::parse_str(id).map_err(|_| AppError::BadRequest("invalid Git repository path".into()))
}

pub fn classify_request(
    method: &Method,
    path: &str,
    query: Option<&str>,
) -> Result<GitService, GitRepoError> {
    if path.contains("..") || path.starts_with('/') || path.contains('\\') {
        return Err(GitRepoError::InvalidReceive(
            "invalid Git service path".into(),
        ));
    }
    match (method, path) {
        (&Method::GET, "info/refs") => {
            let service = query.and_then(|query| {
                url::form_urlencoded::parse(query.as_bytes())
                    .find(|(key, _)| key == "service")
                    .map(|(_, value)| value.into_owned())
            });
            match service.as_deref() {
                Some("git-upload-pack") => Ok(GitService::UploadPack),
                Some("git-receive-pack") => Ok(GitService::ReceivePack),
                _ => Err(GitRepoError::InvalidReceive(
                    "unsupported Git advertisement".into(),
                )),
            }
        }
        (&Method::POST, "git-upload-pack") => Ok(GitService::UploadPack),
        (&Method::POST, "git-receive-pack") => Ok(GitService::ReceivePack),
        _ => Err(GitRepoError::InvalidReceive(
            "unsupported Git service request".into(),
        )),
    }
}

pub fn authorize_service(role: MemberRole, service: GitService) -> Result<(), GitRepoError> {
    if role.can_read() && (service == GitService::UploadPack || role.can_push()) {
        Ok(())
    } else {
        Err(GitRepoError::InvalidReceive(
            "role cannot use this Git service".into(),
        ))
    }
}

pub async fn run_git_http_backend(
    store: &GitRepoStore,
    space_id: Uuid,
    user_id: Uuid,
    role: MemberRole,
    request: GitHttpRequest,
) -> Result<GitHttpResponse, GitRepoError> {
    let service = classify_request(&request.method, &request.path, request.query.as_deref())?;
    authorize_service(role, service)?;
    let repo_path = store.repo_path(space_id);
    if !repo_path.is_dir() {
        return Err(GitRepoError::MissingRef(space_id.to_string()));
    }

    let mut command = tokio::process::Command::new("git");
    command
        .arg("http-backend")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .env("GIT_PROJECT_ROOT", store.root())
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("PATH_INFO", format!("/{space_id}.git/{}", request.path))
        .env("QUERY_STRING", request.query.as_deref().unwrap_or(""))
        .env("REQUEST_METHOD", request.method.as_str())
        .env("SERVER_PROTOCOL", "HTTP/1.1")
        .env("REMOTE_USER", user_id.to_string())
        .env("COWIKI_USER_ID", user_id.to_string())
        .env(
            "COWIKI_ROLE",
            serde_json::to_value(role)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "viewer".into()),
        )
        .env(
            "COWIKI_RECEIVE_MODE",
            if request.bootstrap {
                "bootstrap"
            } else {
                "normal"
            },
        )
        .env("CONTENT_LENGTH", request.body.len().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(content_type) = request
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    {
        command.env("CONTENT_TYPE", content_type);
    }
    if let Some(protocol) = request
        .headers
        .get("git-protocol")
        .and_then(|value| value.to_str().ok())
    {
        command.env("HTTP_GIT_PROTOCOL", protocol);
    }
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&request.body).await?;
    }
    let output = child.wait_with_output().await?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(GitRepoError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    parse_cgi_response(output.stdout)
}

pub fn parse_cgi_response(output: Vec<u8>) -> Result<GitHttpResponse, GitRepoError> {
    let boundary = output
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .map(|index| (index, 4))
        .or_else(|| {
            output
                .windows(2)
                .position(|bytes| bytes == b"\n\n")
                .map(|index| (index, 2))
        })
        .ok_or_else(|| GitRepoError::Git("Git CGI response has no header boundary".into()))?;
    let header_text = std::str::from_utf8(&output[..boundary.0])
        .map_err(|_| GitRepoError::Git("Git CGI response headers are not UTF-8".into()))?;
    let mut status = StatusCode::OK;
    let mut headers = HeaderMap::new();
    for line in header_text.lines() {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| GitRepoError::Git("invalid Git CGI header".into()))?;
        if name.eq_ignore_ascii_case("status") {
            let code = value
                .trim()
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u16>().ok())
                .and_then(|code| StatusCode::from_u16(code).ok())
                .ok_or_else(|| GitRepoError::Git("invalid Git CGI status".into()))?;
            status = code;
            continue;
        }
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|error| GitRepoError::Git(error.to_string()))?;
        let value = HeaderValue::from_str(value.trim())
            .map_err(|error| GitRepoError::Git(error.to_string()))?;
        headers.append(name, value);
    }
    Ok(GitHttpResponse {
        status,
        headers,
        body: output[(boundary.0 + boundary.1)..].to_vec(),
    })
}

fn git_internal(error: GitRepoError) -> AppError {
    match error {
        GitRepoError::MissingRef(_) => AppError::NotFound,
        GitRepoError::InvalidReceive(message) => AppError::BadRequest(message),
        other => AppError::Internal(other.to_string()),
    }
}
