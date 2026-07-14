use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[derive(serde::Serialize)]
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
        .map_err(|e| format!("desktop oauth task failed: {e}"))?
}

/// Bind a private desktop listener. The OS-assigned port prevents the desktop
/// client from accidentally attaching to a cloud/dev backend already running
/// on a well-known port. Loopback keeps the unauthenticated local-mode surface
/// unreachable from other machines.
async fn bind_private_local_listener() -> Result<tokio::net::TcpListener, std::io::Error> {
    tokio::net::TcpListener::bind(("127.0.0.1", 0)).await
}

/// Start the desktop-owned local runtime with its SQLite metadata store.
/// It is deliberately isolated from `cowiki-backend`: cloud services may run
/// on the same machine without becoming the desktop app's local data source.
/// Returns the private API origin injected into the webview.
async fn start_backend() -> Result<String, String> {
    let listener = bind_private_local_listener()
        .await
        .map_err(|e| format!("failed to bind private local api listener: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to read local api port: {e}"))?
        .port();

    let config = cowiki_server::Config::local(None);
    let state = cowiki_server::build_state(config)
        .await
        .map_err(|e| format!("failed to start local backend: {e:#}"))?;
    tauri::async_runtime::spawn(async move {
        if let Err(e) = cowiki_server::serve_on(listener, state).await {
            eprintln!("embedded cowiki backend exited: {e:#}");
        }
    });

    Ok(format!("http://127.0.0.1:{port}"))
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![start_desktop_oauth])
        .setup(|app| {
            let origin = tauri::async_runtime::block_on(start_backend())
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            // The page reads this before anything else (runtime.ts) — it wins
            // over the hardcoded localhost default, so an OS-assigned port
            // still works.
            let window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::default(),
            )
            .title("CoWiki")
            .inner_size(1280.0, 860.0)
            .min_inner_size(980.0, 680.0)
            .initialization_script(format!(
                "window.__COWIKI_API_ORIGIN__ = '{origin}';"
            ))
            .build()?;
            // A programmatically-created macOS window is not activated by the
            // empty `app.windows` config. Make first launch visible instead of
            // leaving a healthy webview behind the desktop.
            window.show()?;
            window.set_focus()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running cowiki desktop client");
}

fn run_loopback_oauth(auth_base_url: String) -> Result<DesktopOAuthCredential, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to start local oauth callback: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("failed to configure oauth callback listener: {e}"))?;

    let callback_url = format!(
        "http://127.0.0.1:{}/auth/callback",
        listener
            .local_addr()
            .map_err(|e| format!("failed to read local oauth port: {e}"))?
            .port()
    );
    let login_url = build_desktop_login_url(&auth_base_url, &callback_url)?;
    open_system_browser(&login_url)?;

    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buf = [0_u8; 8192];
                let n = stream
                    .read(&mut buf)
                    .map_err(|e| format!("failed to read oauth callback: {e}"))?;
                let request = String::from_utf8_lossy(&buf[..n]);
                let first_line = request
                    .lines()
                    .next()
                    .ok_or_else(|| "empty oauth callback request".to_string())?;
                let credential = parse_callback_request_line(first_line)?;
                let body = "CoWiki sign-in complete. You can return to the desktop app.";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                return Ok(credential);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("desktop oauth timed out".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("failed to accept oauth callback: {e}")),
        }
    }
}

fn build_desktop_login_url(auth_base_url: &str, callback_url: &str) -> Result<String, String> {
    let mut url = url::Url::parse(auth_base_url).map_err(|e| format!("invalid auth URL: {e}"))?;
    url.query_pairs_mut()
        .append_pair("client", "desktop")
        .append_pair("callback", callback_url);
    Ok(url.to_string())
}

fn parse_callback_request_line(line: &str) -> Result<DesktopOAuthCredential, String> {
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET" {
        return Err("oauth callback must use GET".into());
    }
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|e| format!("invalid oauth callback: {e}"))?;
    if url.path() != "/auth/callback" {
        return Err("unexpected oauth callback path".into());
    }
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    Ok(DesktopOAuthCredential {
        api_key: params
            .get("api_key")
            .ok_or_else(|| "missing api_key in oauth callback".to_string())?
            .to_string(),
        user_name: params
            .get("user_name")
            .ok_or_else(|| "missing user_name in oauth callback".to_string())?
            .to_string(),
        user_id: params
            .get("user_id")
            .ok_or_else(|| "missing user_id in oauth callback".to_string())?
            .to_string(),
    })
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
    .map_err(|e| format!("failed to open browser: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("browser opener exited with status {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bind_private_local_listener, build_desktop_login_url, parse_callback_request_line,
    };

    #[tokio::test]
    async fn private_local_listener_is_loopback_and_os_assigned() {
        let listener = bind_private_local_listener().await.unwrap();
        let address = listener.local_addr().unwrap();

        assert!(address.ip().is_loopback());
        assert_ne!(address.port(), 0);
    }

    #[test]
    fn builds_desktop_login_url_with_loopback_callback() {
        let url = build_desktop_login_url(
            "http://localhost:3000/api/auth/github",
            "http://127.0.0.1:39281/auth/callback",
        )
        .unwrap();

        assert_eq!(
            url,
            "http://localhost:3000/api/auth/github?client=desktop&callback=http%3A%2F%2F127.0.0.1%3A39281%2Fauth%2Fcallback",
        );
    }

    #[test]
    fn parses_loopback_callback_request_line() {
        let credential = parse_callback_request_line(
            "GET /auth/callback?api_key=cw_123&user_name=octo-cat&user_id=user-1 HTTP/1.1",
        )
        .unwrap();

        assert_eq!(credential.api_key, "cw_123");
        assert_eq!(credential.user_name, "octo-cat");
        assert_eq!(credential.user_id, "user-1");
    }
}
