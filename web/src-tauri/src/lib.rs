mod cloud_sync;
mod extract;
mod knowledge_index;
mod local_engine;
mod mcp;
mod okf;
mod terminal;

use local_engine::{
    AgentChange, BrokenLink, Checkpoint, FileDiff, IngestFileOutcome, LocalEngine, PageFull,
    PageMeta, SearchResponse, SourceContent, SourceItem, Space, SpaceHistory, SubmitResult,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{Manager, State};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopOAuthCredential {
    api_key: String,
    user_name: String,
    user_id: String,
}

#[tauri::command]
async fn start_desktop_oauth(auth_base_url: String) -> Result<DesktopOAuthCredential, String> {
    tauri::async_runtime::spawn_blocking(move || run_loopback_oauth(auth_base_url))
        .await
        .map_err(|error| format!("desktop OAuth task failed: {error}"))?
}

#[tauri::command]
fn choose_local_space_directory() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn choose_source_files() -> Vec<String> {
    let extensions = extract::all_supported_extensions();
    rfd::FileDialog::new()
        .add_filter("Supported sources", &extensions)
        .pick_files()
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
fn local_list_spaces(engine: State<'_, LocalEngine>) -> Result<Vec<Space>, String> {
    engine.list_spaces()
}

#[tauri::command]
fn local_add_space(
    engine: State<'_, LocalEngine>,
    name: String,
    slug: String,
    local_path: String,
    create_directory: bool,
) -> Result<Space, String> {
    let _mutation = engine.lock_mutations()?;
    let selected = PathBuf::from(local_path);
    let folder = if create_directory {
        if name.trim().is_empty() || name.contains(['/', '\\']) || matches!(name.trim(), "." | "..")
        {
            return Err("Space name cannot be used as a folder name".to_string());
        }
        let folder = selected.join(name.trim());
        std::fs::create_dir(&folder)
            .map_err(|error| format!("cannot create Space folder: {error}"))?;
        folder
    } else {
        selected
    };
    engine.add_space(&name, &slug, &folder)
}

#[tauri::command]
fn local_list_pages(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<Vec<PageMeta>, String> {
    engine.list_pages(&space_slug)
}

#[tauri::command]
fn local_get_page(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    page_slug: String,
) -> Result<PageFull, String> {
    engine.get_page(&space_slug, &page_slug)
}

#[tauri::command]
fn local_write_page(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    page_slug: String,
    content: String,
    expected_content: Option<String>,
    create_only: Option<bool>,
) -> Result<(), String> {
    let _mutation = engine.lock_mutations()?;
    engine.write_page_checked(
        &space_slug,
        &page_slug,
        &content,
        expected_content.as_deref(),
        create_only.unwrap_or(false),
    )
}

#[tauri::command]
fn local_create_folder(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    name: String,
    parent: Option<String>,
) -> Result<(), String> {
    let _mutation = engine.lock_mutations()?;
    engine.create_folder(&space_slug, &name, parent.as_deref())
}

#[tauri::command]
fn local_list_sources(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<Vec<SourceItem>, String> {
    engine.list_sources(&space_slug)
}

#[tauri::command]
fn local_get_source(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    filename: String,
) -> Result<SourceContent, String> {
    engine.get_source(&space_slug, &filename)
}

#[tauri::command]
fn local_ingest(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    source_type: String,
    content: String,
    filename: Option<String>,
) -> Result<SourceItem, String> {
    let _mutation = engine.lock_mutations()?;
    engine.ingest(&space_slug, &source_type, &content, filename.as_deref())
}

#[tauri::command]
async fn local_ingest_files(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    source_paths: Vec<String>,
) -> Result<Vec<IngestFileOutcome>, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation = engine.lock_mutations()?;
        engine.ingest_files(&space_slug, &source_paths)
    })
    .await
    .map_err(|error| format!("local source import task failed: {error}"))?
}

#[tauri::command]
fn local_rename_path(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    from: String,
    to: String,
) -> Result<(), String> {
    let _mutation = engine.lock_mutations()?;
    engine.rename_path(&space_slug, &from, &to)
}

#[tauri::command]
fn local_delete_path(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    path: String,
) -> Result<(), String> {
    let _mutation = engine.lock_mutations()?;
    engine.delete_path(&space_slug, &path)
}

#[tauri::command]
fn local_search(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    query: String,
    limit: usize,
) -> Result<SearchResponse, String> {
    engine.search(&space_slug, &query, limit)
}

#[tauri::command]
fn local_list_broken_links(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<Vec<BrokenLink>, String> {
    engine.list_broken_links(&space_slug)
}

#[tauri::command]
fn local_submit(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    paths: Vec<String>,
) -> Result<SubmitResult, String> {
    let _mutation = engine.lock_mutations()?;
    engine.submit(&space_slug, &paths)
}

#[tauri::command]
fn local_working_diff(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<Vec<FileDiff>, String> {
    let _mutation = engine.lock_mutations()?;
    engine.working_diff(&space_slug)
}

#[tauri::command]
fn local_keep_working_diff(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    expected: Vec<FileDiff>,
) -> Result<SubmitResult, String> {
    let _mutation = engine.lock_mutations()?;
    engine.keep_working_diff(&space_slug, &expected)
}

#[tauri::command]
fn local_history(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<SpaceHistory, String> {
    let _mutation = engine.lock_mutations()?;
    engine.history(&space_slug)
}

#[tauri::command]
fn local_create_checkpoint(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    name: Option<String>,
) -> Result<Checkpoint, String> {
    let _mutation = engine.lock_mutations()?;
    engine.create_checkpoint(&space_slug, name.as_deref())
}

#[tauri::command]
fn local_create_agent_change(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    agent_name: String,
) -> Result<AgentChange, String> {
    let _mutation = engine.lock_mutations()?;
    engine.create_agent_change(&space_slug, &agent_name)
}

#[tauri::command]
fn local_list_agent_changes(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<Vec<AgentChange>, String> {
    engine.list_agent_changes(&space_slug)
}

#[tauri::command]
fn local_merge_agent_change(
    engine: State<'_, LocalEngine>,
    terminals: State<'_, terminal::TerminalState>,
    space_slug: String,
    change_id: String,
) -> Result<AgentChange, String> {
    // Lock order is terminal close gate, then Draft mutation. Terminal starts
    // only touch their registry and never wait on the mutation lock.
    let _closing = terminals.begin_change_close(&space_slug, &change_id)?;
    let _mutation = engine.lock_mutations()?;
    engine.merge_agent_change(&space_slug, &change_id)
}

#[tauri::command]
fn local_discard_agent_change(
    engine: State<'_, LocalEngine>,
    terminals: State<'_, terminal::TerminalState>,
    space_slug: String,
    change_id: String,
) -> Result<AgentChange, String> {
    // Keep the same order as merge to avoid a registry/mutation lock cycle.
    let _closing = terminals.begin_change_close(&space_slug, &change_id)?;
    let _mutation = engine.lock_mutations()?;
    engine.discard_agent_change(&space_slug, &change_id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn cloud_link_space(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    cloud_base_url: String,
    api_key: String,
    cloud_space_id: Option<String>,
    git_url: Option<String>,
    cloud_name: String,
    cloud_slug: String,
    commit_message: Option<String>,
    user_name: String,
    user_id: String,
) -> Result<cloud_sync::CloudSyncResult, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let user_id = uuid::Uuid::parse_str(&user_id)
            .map_err(|_| "signed-in Cloud user id is invalid".to_string())?;
        let _mutation = engine.lock_mutations()?;
        cloud_sync::link_space(
            &engine,
            &space_slug,
            &cloud_base_url,
            &api_key,
            cloud_space_id.as_deref(),
            git_url.as_deref(),
            &cloud_name,
            &cloud_slug,
            commit_message.as_deref(),
            &user_name,
            user_id,
        )
    })
    .await
    .map_err(|error| format!("Cloud link task failed: {error}"))?
}

#[tauri::command]
async fn cloud_get_status(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<cloud_sync::CloudSyncResult, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cloud_sync::get_status(&engine, &space_slug))
        .await
        .map_err(|error| format!("Cloud status task failed: {error}"))?
}

#[tauri::command]
async fn cloud_sync_if_clean(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    api_key: String,
) -> Result<cloud_sync::CloudSyncResult, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation = engine.lock_mutations()?;
        cloud_sync::sync_if_clean(&engine, &space_slug, &api_key)
    })
    .await
    .map_err(|error| format!("Cloud sync task failed: {error}"))?
}

#[tauri::command]
async fn cloud_submit(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    api_key: String,
    commit_message: Option<String>,
    pull_request_title: Option<String>,
    pull_request_body: Option<String>,
    user_name: String,
) -> Result<cloud_sync::CloudSyncResult, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation = engine.lock_mutations()?;
        cloud_sync::submit(
            &engine,
            &space_slug,
            &api_key,
            commit_message.as_deref(),
            pull_request_title.as_deref(),
            pull_request_body.as_deref(),
            &user_name,
        )
    })
    .await
    .map_err(|error| format!("Cloud submit task failed: {error}"))?
}

#[tauri::command]
async fn cloud_rebase_continue(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<cloud_sync::CloudSyncResult, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation = engine.lock_mutations()?;
        cloud_sync::rebase_continue(&engine, &space_slug)
    })
    .await
    .map_err(|error| format!("Cloud rebase task failed: {error}"))?
}

#[tauri::command]
async fn cloud_rebase_abort(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<cloud_sync::CloudSyncResult, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation = engine.lock_mutations()?;
        cloud_sync::rebase_abort(&engine, &space_slug)
    })
    .await
    .map_err(|error| format!("Cloud rebase task failed: {error}"))?
}

#[tauri::command]
async fn open_external_url(url: String) -> Result<(), String> {
    let url = validate_external_url(&url)?.to_string();
    tauri::async_runtime::spawn_blocking(move || open_system_browser(&url))
        .await
        .map_err(|error| format!("browser opener task failed: {error}"))?
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            start_desktop_oauth,
            open_external_url,
            choose_local_space_directory,
            choose_source_files,
            local_list_spaces,
            local_add_space,
            local_list_pages,
            local_get_page,
            local_write_page,
            local_create_folder,
            local_list_sources,
            local_get_source,
            local_ingest,
            local_ingest_files,
            local_rename_path,
            local_delete_path,
            local_search,
            local_list_broken_links,
            local_submit,
            local_working_diff,
            local_keep_working_diff,
            local_history,
            local_create_checkpoint,
            local_create_agent_change,
            local_list_agent_changes,
            local_merge_agent_change,
            local_discard_agent_change,
            terminal::agent_probe,
            cloud_link_space,
            cloud_get_status,
            cloud_sync_if_clean,
            cloud_submit,
            cloud_rebase_continue,
            cloud_rebase_abort,
            terminal::terminal_create,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_kill,
        ])
        .setup(|app| {
            // Keep the small, rebuildable index beside the previous local
            // metadata so repositories opened by older CoWiki builds can be
            // recovered automatically on upgrade.
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.path().app_data_dir().unwrap_or_default());
            let cowiki_home = home.join("cowiki");
            let engine = LocalEngine::open(&cowiki_home.join(".cowiki"))
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            engine
                .import_legacy_spaces(&cowiki_home)
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(engine);
            app.manage(terminal::TerminalState::default());

            let window_builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                    .title("CoWiki")
                    .inner_size(1280.0, 860.0)
                    .min_inner_size(980.0, 680.0);
            #[cfg(target_os = "macos")]
            let window_builder = window_builder
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true);
            let window = window_builder.build()?;
            window.show()?;
            window.set_focus()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running CoWiki desktop client");
}

pub fn run_mcp_if_requested() -> Result<bool, String> {
    mcp::run_from_process_args()
}

fn run_loopback_oauth(auth_base_url: String) -> Result<DesktopOAuthCredential, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("failed to start local OAuth callback: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let callback = format!(
        "http://127.0.0.1:{}/auth/callback",
        listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port()
    );
    let mut login = url::Url::parse(&auth_base_url)
        .map_err(|error| format!("invalid Cloud sign-in URL: {error}"))?;
    login
        .query_pairs_mut()
        .append_pair("client", "desktop")
        .append_pair("callback", &callback);
    open_system_browser(login.as_str())?;

    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buffer = [0_u8; 8192];
                let read = stream
                    .read(&mut buffer)
                    .map_err(|error| error.to_string())?;
                let request = String::from_utf8_lossy(&buffer[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .ok_or_else(|| "invalid OAuth callback".to_string())?;
                let callback_url = url::Url::parse(&format!("http://127.0.0.1{target}"))
                    .map_err(|error| error.to_string())?;
                let parameters = callback_url
                    .query_pairs()
                    .into_owned()
                    .collect::<std::collections::HashMap<_, _>>();
                let credential = if let (Some(api_key), Some(user_name), Some(user_id)) = (
                    parameters.get("api_key"),
                    parameters.get("user_name"),
                    parameters.get("user_id"),
                ) {
                    DesktopOAuthCredential {
                        api_key: api_key.clone(),
                        user_name: user_name.clone(),
                        user_id: user_id.clone(),
                    }
                } else {
                    let code = parameters
                        .get("code")
                        .ok_or("missing desktop exchange code")?;
                    exchange_desktop_oauth_code(&auth_base_url, code)?
                };
                let body = "CoWiki Cloud sign-in complete. You can return to the desktop app.";
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                let _ = stream.write_all(response.as_bytes());
                return Ok(credential);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("Cloud sign-in timed out".to_string());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn exchange_desktop_oauth_code(
    auth_base_url: &str,
    code: &str,
) -> Result<DesktopOAuthCredential, String> {
    let endpoint = url::Url::parse(auth_base_url)
        .map_err(|error| format!("invalid Cloud sign-in URL: {error}"))?
        .join("desktop/exchange")
        .map_err(|error| format!("invalid Cloud exchange URL: {error}"))?;
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?
        .post(endpoint)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .map_err(|error| format!("Cloud sign-in exchange failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Cloud sign-in exchange was rejected ({})",
            response.status()
        ));
    }
    response
        .json::<DesktopOAuthCredential>()
        .map_err(|error| format!("Cloud sign-in response was invalid: {error}"))
}

fn open_system_browser(url: &str) -> Result<(), String> {
    let status = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(url).status()
    }
    .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("browser opener exited with {status}"))
    }
}

fn validate_external_url(url: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(url).map_err(|error| format!("invalid external URL: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
        return Err("external URL must be HTTP(S) and include a host".to_string());
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_browser_urls_accept_only_http_and_https() {
        assert!(validate_external_url("https://cloud.cowiki.app/cloud").is_ok());
        assert!(validate_external_url("http://localhost:8787/cloud").is_ok());
        assert!(validate_external_url("file:///tmp/private").is_err());
        assert!(validate_external_url("javascript:alert(1)").is_err());
    }
}
