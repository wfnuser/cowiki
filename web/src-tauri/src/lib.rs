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

/// Default port for the embedded backend; falls back to an OS-assigned one.
const LOCAL_API_PORT: u16 = 3000;

/// Local-first startup: reuse an already-running backend on :3000 when one is
/// healthy (dev convenience — `cargo run` your own server and the app uses
/// it); otherwise run the whole backend in-process on a loopback listener
/// with the SQLite metadata store. Returns the API origin the webview should
/// call.
async fn start_backend() -> Result<String, String> {
    if external_backend_is_healthy(LOCAL_API_PORT) {
        return Ok(format!("http://127.0.0.1:{LOCAL_API_PORT}"));
    }

    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", LOCAL_API_PORT)).await {
        Ok(l) => l,
        // Port taken by something that isn't a healthy cowiki server — let the
        // OS pick; the origin is injected into the page so nothing hardcodes it.
        Err(_) => tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| format!("failed to bind local api listener: {e}"))?,
    };
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

/// Minimal `GET /api/health` probe over std TCP (no HTTP client dependency).
fn external_backend_is_healthy(port: u16) -> bool {
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(300),
    ) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    if stream
        .write_all(b"GET /api/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.starts_with("HTTP/1.1 200") && buf.ends_with("ok")
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
            tauri::WebviewWindowBuilder::new(
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
    use super::{build_desktop_login_url, parse_callback_request_line};

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
