//! Native readiness probes and sign-in, without a login terminal.
//!
//! Codex: `account/read`, then `account/login/start` returns an auth URL that
//! CoWiki validates before opening the system browser. Claude Code:
//! `claude auth status --json`, and `claude auth login` runs its own browser
//! flow. ACP agents: the advertised `authenticate` method (the agent runs its
//! own browser flow).

use super::acp;
use super::claude;
use super::environment::agent_environment;
use super::registry::native_protocol;
use super::rpc::{Framing, Incoming, LaunchSpec, RpcError, RpcProcess};
use super::{native_executable, not_installed_message};
use crate::terminal::{agent_display_name, first_nonempty_line, AgentKind};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// Oldest Codex CLI whose app-server v2 protocol CoWiki uses. The adapter was
/// checked against 0.166.0 (`codex app-server generate-ts`).
pub const MIN_CODEX_VERSION: (u64, u64, u64) = (0, 163, 0);
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const CACHE_AGE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeStatus {
    Available,
    SignedOut,
    NotInstalled,
    Incompatible,
    Broken,
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeReadiness {
    pub agent: AgentKind,
    pub status: NativeStatus,
    pub protocol: Option<&'static str>,
    pub version: Option<String>,
    pub account: Option<String>,
    pub message: Option<String>,
    pub detail: Option<String>,
    pub can_sign_in: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginEvent {
    pub agent: AgentKind,
    pub status: &'static str,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStarted {
    pub agent: AgentKind,
    /// The validated URL CoWiki opened, when the provider returned one.
    pub url: Option<String>,
    pub already_open: bool,
}

pub type LoginNotifier = Arc<dyn Fn(LoginEvent) + Send + Sync>;
pub type BrowserOpener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
type Notifications = mpsc::Receiver<(String, Value)>;

fn readiness(agent: AgentKind, status: NativeStatus) -> NativeReadiness {
    NativeReadiness {
        agent,
        status,
        protocol: native_protocol(agent),
        version: None,
        account: None,
        message: None,
        detail: None,
        can_sign_in: false,
    }
}

pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|part| part.matches('.').count() >= 2)
        .find_map(|part| {
            let mut numbers = part.split('.').map(|number| number.parse::<u64>().ok());
            Some((numbers.next()??, numbers.next()??, numbers.next()??))
        })
}

/// Runs a short CLI command (no shell, no input) and returns its stdout,
/// or None when it could not start or did not finish within `timeout`.
fn run_quiet(
    executable: &Path,
    args: &[&str],
    environment: &[(String, String)],
    timeout: Duration,
) -> Option<String> {
    let mut child = Command::new(executable)
        .args(args)
        .current_dir(probe_directory())
        .env_clear()
        .envs(environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn cli_version(executable: &Path, environment: &[(String, String)]) -> Option<String> {
    let text = run_quiet(
        executable,
        &["--version"],
        environment,
        Duration::from_secs(6),
    )?;
    first_nonempty_line(&crate::terminal::strip_terminal_control_sequences(&text))
}

/// `claude auth status --json` (Claude Code 2.1 and newer).
pub fn claude_auth_status(executable: &Path, environment: &[(String, String)]) -> Option<Value> {
    let text = run_quiet(
        executable,
        &["auth", "status", "--json"],
        environment,
        Duration::from_secs(10),
    )?;
    serde_json::from_str::<Value>(text.trim())
        .ok()
        .filter(|status| status.get("loggedIn").is_some())
}

pub fn claude_account_label(status: &Value) -> Option<String> {
    let email = status.get("email").and_then(Value::as_str);
    let provider = status.get("apiProvider").and_then(Value::as_str);
    let method = status.get("authMethod").and_then(Value::as_str);
    let plan = status
        .get("subscriptionType")
        .and_then(Value::as_str)
        .filter(|plan| !plan.is_empty())
        .map(|plan| {
            let mut letters = plan.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + letters.as_str())
                .unwrap_or_default()
        });
    let name = match (provider, method) {
        (Some(provider), _) if provider != "firstParty" => provider.to_string(),
        (_, Some("claude.ai")) => match plan {
            Some(plan) => format!("Claude {plan}"),
            None => "Claude".to_string(),
        },
        (_, Some(method)) if !method.is_empty() => method.to_string(),
        _ => return email.map(ToOwned::to_owned),
    };
    Some(match email {
        Some(email) => format!("{name} ({email})"),
        None => name,
    })
}

fn probe_directory() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

fn spawn_quiet(
    label: &str,
    program: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
    framing: Framing,
) -> Result<Arc<RpcProcess>, String> {
    let (process, _) = spawn_listening(label, program, args, extra_env, framing)?;
    Ok(process)
}

fn spawn_listening(
    label: &str,
    program: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
    framing: Framing,
) -> Result<(Arc<RpcProcess>, Notifications), String> {
    let mut env = agent_environment();
    env.retain(|(key, _)| !extra_env.iter().any(|(fixed, _)| fixed == key));
    env.extend(
        extra_env
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string())),
    );
    let spec = LaunchSpec {
        program: program.to_path_buf(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
        cwd: probe_directory(),
        env,
    };
    let (sender, receiver) = mpsc::channel();
    let sender = Mutex::new(sender);
    let process = RpcProcess::spawn(
        label,
        &spec,
        framing,
        Arc::new(move |incoming| {
            if let Incoming::Notification { method, params } = incoming {
                if let Ok(sender) = sender.lock() {
                    let _ = sender.send((method, params));
                }
            }
        }),
        Box::new(|_| {}),
    )?;
    Ok((process, receiver))
}

fn codex_initialize(rpc: &RpcProcess) -> Result<(), RpcError> {
    rpc.request(
        "initialize",
        json!({
            "clientInfo": {"name": "cowiki", "title": "CoWiki", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"experimentalApi": false, "requestAttestation": false},
        }),
        PROBE_TIMEOUT,
    )?;
    rpc.notify("initialized", json!({}))
}

fn account_label(account: &Value) -> Option<String> {
    match account.get("type").and_then(Value::as_str)? {
        "chatgpt" => Some(match account.get("email").and_then(Value::as_str) {
            Some(email) => format!("ChatGPT ({email})"),
            None => "ChatGPT".into(),
        }),
        "apiKey" => Some("API key".into()),
        other => Some(other.to_string()),
    }
}

/// `~/.grok/auth.json` (or `GROK_AUTH_PATH`) holds Grok's sign-in. Only its
/// existence is checked; CoWiki never reads it.
fn grok_signed_in(environment: &[(String, String)]) -> bool {
    let lookup = |key: &str| {
        environment
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    };
    if let Some(path) = lookup("GROK_AUTH_PATH") {
        return Path::new(&path).exists();
    }
    lookup("HOME")
        .map(|home| Path::new(&home).join(".grok").join("auth.json").exists())
        .unwrap_or(false)
}

pub fn probe(agent: AgentKind) -> NativeReadiness {
    let name = agent_display_name(agent);
    let Some(protocol) = native_protocol(agent) else {
        return NativeReadiness {
            message: Some(format!(
                "{name} has no structured protocol CoWiki can drive yet"
            )),
            ..readiness(agent, NativeStatus::Unsupported)
        };
    };
    let Some(executable) = native_executable(agent) else {
        return NativeReadiness {
            message: Some(not_installed_message(agent)),
            ..readiness(agent, NativeStatus::NotInstalled)
        };
    };
    let environment = agent_environment();
    let version = cli_version(&executable, &environment);
    let base = NativeReadiness {
        version: version.clone(),
        ..readiness(agent, NativeStatus::Available)
    };
    match protocol {
        "codex-app-server" => {
            if let Some(found) = version.as_deref().and_then(parse_version) {
                if found < MIN_CODEX_VERSION {
                    return NativeReadiness {
                        status: NativeStatus::Incompatible,
                        message: Some(format!(
                            "CoWiki needs Codex {}.{}.{} or newer",
                            MIN_CODEX_VERSION.0, MIN_CODEX_VERSION.1, MIN_CODEX_VERSION.2
                        )),
                        ..base
                    };
                }
            }
            let rpc = match spawn_quiet(
                "Codex",
                &executable,
                &["app-server", "--listen", "stdio://"],
                &[],
                Framing::Codex,
            ) {
                Ok(rpc) => rpc,
                Err(error) => {
                    return NativeReadiness {
                        status: NativeStatus::Broken,
                        message: Some(error),
                        ..base
                    };
                }
            };
            let result = codex_initialize(&rpc)
                .and_then(|_| rpc.request("account/read", json!({}), PROBE_TIMEOUT));
            let _ = rpc.shutdown(Duration::from_millis(800));
            match result {
                Ok(account) => {
                    let signed_out = account.get("account").is_none_or(Value::is_null)
                        && account.get("requiresOpenaiAuth").and_then(Value::as_bool) == Some(true);
                    NativeReadiness {
                        status: if signed_out {
                            NativeStatus::SignedOut
                        } else {
                            NativeStatus::Available
                        },
                        account: account.get("account").and_then(account_label),
                        message: signed_out
                            .then(|| "Codex is installed but not signed in".to_string()),
                        can_sign_in: true,
                        ..base
                    }
                }
                Err(error) => NativeReadiness {
                    status: NativeStatus::Incompatible,
                    message: Some("This Codex CLI did not answer the app-server handshake".into()),
                    detail: Some(error.to_string()),
                    ..base
                },
            }
        }
        "acp" => {
            let Some((args, env)) = acp::launch_args(agent) else {
                return NativeReadiness {
                    status: NativeStatus::Unsupported,
                    ..base
                };
            };
            let rpc = match spawn_quiet(name, &executable, &args, &env, Framing::JsonRpc2) {
                Ok(rpc) => rpc,
                Err(error) => {
                    return NativeReadiness {
                        status: NativeStatus::Broken,
                        message: Some(error),
                        ..base
                    };
                }
            };
            let result = rpc.request("initialize", acp::initialize_params(), PROBE_TIMEOUT);
            let _ = rpc.shutdown(Duration::from_millis(800));
            match result.map_err(|error| error.to_string()).and_then(|value| {
                acp::check_initialize(&value)?;
                Ok(value)
            }) {
                Ok(initialized) => {
                    let can_sign_in = initialized
                        .get("authMethods")
                        .and_then(Value::as_array)
                        .is_some_and(|methods| !methods.is_empty());
                    let signed_out = agent == AgentKind::Grok && !grok_signed_in(&environment);
                    NativeReadiness {
                        status: if signed_out {
                            NativeStatus::SignedOut
                        } else {
                            NativeStatus::Available
                        },
                        message: signed_out
                            .then(|| format!("{name} is installed but not signed in")),
                        can_sign_in,
                        ..base
                    }
                }
                Err(error) => NativeReadiness {
                    status: NativeStatus::Incompatible,
                    message: Some(format!(
                        "{name} did not answer the Agent Client Protocol handshake"
                    )),
                    detail: Some(error),
                    ..base
                },
            }
        }
        "claude-stream-json" => {
            let Some(status) = claude_auth_status(&executable, &environment) else {
                return NativeReadiness {
                    status: NativeStatus::Incompatible,
                    message: Some(
                        "This Claude Code is too old for the sidebar. Update it with `claude update`, then check again."
                            .into(),
                    ),
                    ..base
                };
            };
            let account = claude_account_label(&status);
            if status.get("loggedIn").and_then(Value::as_bool) != Some(true) {
                return NativeReadiness {
                    status: NativeStatus::SignedOut,
                    message: Some("Claude Code is installed but not signed in".into()),
                    can_sign_in: true,
                    ..base
                };
            }
            match claude_handshake(&executable) {
                Ok(()) => NativeReadiness {
                    account,
                    can_sign_in: true,
                    ..base
                },
                Err(error) => NativeReadiness {
                    status: NativeStatus::Incompatible,
                    message: Some(
                        "This Claude Code did not answer the stream-json handshake. Update it with `claude update`, then check again."
                            .into(),
                    ),
                    detail: Some(error),
                    account,
                    ..base
                },
            }
        }
        _ => NativeReadiness {
            status: NativeStatus::Unsupported,
            ..base
        },
    }
}

/// Starts Claude Code the way a session does (without a conversation and
/// without saving one), sends `initialize`, and stops it. No model call.
fn claude_handshake(executable: &Path) -> Result<(), String> {
    let args = claude::probe_args();
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let rpc = spawn_quiet(
        "Claude Code",
        executable,
        &args,
        &[],
        Framing::ClaudeControl,
    )?;
    let result = rpc.request("initialize", claude::initialize_params(), PROBE_TIMEOUT);
    let _ = rpc.shutdown(Duration::from_millis(1500));
    let response = result.map_err(|error| error.to_string())?;
    claude::check_initialize(&response).map_err(|error| error.message().to_string())
}

/// Only HTTPS URLs on the provider's own sign-in hosts are opened.
pub fn validate_auth_url(url: &str, allowed_hosts: &[&str]) -> Result<url::Url, String> {
    let parsed =
        url::Url::parse(url).map_err(|_| "the sign-in link is not a valid URL".to_string())?;
    if parsed.scheme() != "https" {
        return Err("the sign-in link is not HTTPS".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("the sign-in link contains credentials".into());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if !allowed_hosts.iter().any(|allowed| host == *allowed) {
        return Err(format!(
            "the sign-in link points to an unexpected host ({host})"
        ));
    }
    Ok(parsed)
}

pub const CODEX_AUTH_HOSTS: &[&str] = &["auth.openai.com", "chatgpt.com", "auth0.openai.com"];

struct LoginProcess {
    rpc: Arc<RpcProcess>,
    login_id: Option<String>,
}

#[derive(Clone)]
pub struct AgentAuth {
    logins: Arc<Mutex<HashMap<AgentKind, LoginProcess>>>,
    cache: Arc<Mutex<HashMap<AgentKind, (Instant, NativeReadiness)>>>,
    open_browser: BrowserOpener,
}

impl AgentAuth {
    pub fn new(open_browser: BrowserOpener) -> Self {
        Self {
            logins: Arc::default(),
            cache: Arc::default(),
            open_browser,
        }
    }

    pub fn probe(&self, agent: AgentKind, force: bool) -> NativeReadiness {
        if !force {
            if let Some((checked, value)) = self
                .cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(&agent).cloned())
            {
                if checked.elapsed() < CACHE_AGE {
                    return value;
                }
            }
        }
        let value = probe(agent);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(agent, (Instant::now(), value.clone()));
        }
        value
    }

    fn forget(&self, agent: AgentKind) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(&agent);
        }
    }

    pub fn start_login(
        &self,
        agent: AgentKind,
        notify: LoginNotifier,
    ) -> Result<LoginStarted, String> {
        let name = agent_display_name(agent);
        {
            let mut logins = self
                .logins
                .lock()
                .map_err(|_| "sign-in state is unavailable".to_string())?;
            if let Some(existing) = logins.get(&agent) {
                if existing.rpc.is_alive() {
                    return Ok(LoginStarted {
                        agent,
                        url: None,
                        already_open: true,
                    });
                }
                logins.remove(&agent);
            }
        }
        let executable = native_executable(agent).ok_or_else(|| not_installed_message(agent))?;
        match native_protocol(agent) {
            Some("codex-app-server") => {
                let (rpc, notifications) = spawn_listening(
                    "Codex sign-in",
                    &executable,
                    &["app-server", "--listen", "stdio://"],
                    &[],
                    Framing::Codex,
                )?;
                let started = codex_initialize(&rpc).and_then(|_| {
                    rpc.request(
                        "account/login/start",
                        json!({"type": "chatgpt"}),
                        PROBE_TIMEOUT,
                    )
                });
                let started = match started {
                    Ok(started) => started,
                    Err(error) => {
                        let _ = rpc.shutdown(Duration::from_millis(500));
                        return Err(format!("Codex could not start sign-in: {error}"));
                    }
                };
                let login_id = started
                    .get("loginId")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let url = started
                    .get("authUrl")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let url = match validate_auth_url(url, CODEX_AUTH_HOSTS) {
                    Ok(url) => url.to_string(),
                    Err(error) => {
                        let _ = rpc.shutdown(Duration::from_millis(500));
                        return Err(format!("CoWiki did not open Codex's sign-in link: {error}"));
                    }
                };
                if let Err(error) = (self.open_browser)(&url) {
                    let _ = rpc.shutdown(Duration::from_millis(500));
                    return Err(format!("Could not open the browser: {error}"));
                }
                self.logins
                    .lock()
                    .map_err(|_| "sign-in state is unavailable".to_string())?
                    .insert(
                        agent,
                        LoginProcess {
                            rpc: rpc.clone(),
                            login_id: login_id.clone(),
                        },
                    );
                let auth = self.clone();
                std::thread::spawn(move || {
                    let deadline = Instant::now() + LOGIN_TIMEOUT;
                    let mut outcome = ("failed", Some("Sign-in timed out".to_string()));
                    while Instant::now() < deadline {
                        match notifications.recv_timeout(Duration::from_millis(500)) {
                            Ok((method, params)) if method == "account/login/completed" => {
                                let same = params
                                    .get("loginId")
                                    .and_then(Value::as_str)
                                    .map(ToOwned::to_owned)
                                    == login_id
                                    || params.get("loginId").is_none_or(Value::is_null);
                                if !same {
                                    continue;
                                }
                                outcome = if params.get("success").and_then(Value::as_bool)
                                    == Some(true)
                                {
                                    ("completed", None)
                                } else {
                                    (
                                        "failed",
                                        params
                                            .get("error")
                                            .and_then(Value::as_str)
                                            .map(ToOwned::to_owned),
                                    )
                                };
                                break;
                            }
                            Ok(_) => {}
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if !rpc.is_alive() {
                                    outcome = ("cancelled", None);
                                    break;
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                outcome = ("cancelled", None);
                                break;
                            }
                        }
                    }
                    auth.finish_login(agent, &rpc, outcome.0, outcome.1, &notify);
                });
                Ok(LoginStarted {
                    agent,
                    url: Some(url),
                    already_open: false,
                })
            }
            Some("acp") => {
                let (args, env) =
                    acp::launch_args(agent).ok_or_else(|| format!("{name} cannot sign in here"))?;
                let rpc = spawn_quiet(
                    &format!("{name} sign-in"),
                    &executable,
                    &args,
                    &env,
                    Framing::JsonRpc2,
                )?;
                let initialized =
                    match rpc.request("initialize", acp::initialize_params(), PROBE_TIMEOUT) {
                        Ok(value) => value,
                        Err(error) => {
                            let _ = rpc.shutdown(Duration::from_millis(500));
                            return Err(format!("{name} did not answer: {error}"));
                        }
                    };
                let method_id = initialized
                    .get("authMethods")
                    .and_then(Value::as_array)
                    .and_then(|methods| methods.first())
                    .and_then(|method| method.get("id").and_then(Value::as_str))
                    .map(ToOwned::to_owned);
                let Some(method_id) = method_id else {
                    let _ = rpc.shutdown(Duration::from_millis(500));
                    return Err(format!("{name} does not offer a sign-in method over ACP"));
                };
                self.logins
                    .lock()
                    .map_err(|_| "sign-in state is unavailable".to_string())?
                    .insert(
                        agent,
                        LoginProcess {
                            rpc: rpc.clone(),
                            login_id: None,
                        },
                    );
                let auth = self.clone();
                std::thread::spawn(move || {
                    // The agent opens the browser for its own sign-in.
                    let result = rpc.request(
                        "authenticate",
                        json!({"methodId": method_id}),
                        LOGIN_TIMEOUT,
                    );
                    let (status, message) = match result {
                        Ok(_) => ("completed", None),
                        Err(RpcError::Closed(_)) => ("cancelled", None),
                        Err(error) => ("failed", Some(error.to_string())),
                    };
                    auth.finish_login(agent, &rpc, status, message, &notify);
                });
                Ok(LoginStarted {
                    agent,
                    url: None,
                    already_open: false,
                })
            }
            Some("claude-stream-json") => {
                // `claude auth login` opens the browser and waits for it.
                let rpc = spawn_quiet(
                    "Claude Code sign-in",
                    &executable,
                    &["auth", "login"],
                    &[],
                    Framing::Codex,
                )?;
                self.logins
                    .lock()
                    .map_err(|_| "sign-in state is unavailable".to_string())?
                    .insert(
                        agent,
                        LoginProcess {
                            rpc: rpc.clone(),
                            login_id: None,
                        },
                    );
                let auth = self.clone();
                std::thread::spawn(move || {
                    let finished = rpc.wait_exit(LOGIN_TIMEOUT);
                    let signed_in = finished
                        && claude_auth_status(&executable, &agent_environment())
                            .and_then(|status| status.get("loggedIn").and_then(Value::as_bool))
                            == Some(true);
                    let (status, message) = if signed_in {
                        ("completed", None)
                    } else if !finished {
                        ("failed", Some("Sign-in timed out".to_string()))
                    } else if auth.login_cancelled(agent, &rpc) {
                        ("cancelled", None)
                    } else {
                        (
                            "failed",
                            Some(
                                rpc.exit_reason()
                                    .unwrap_or_else(|| "Claude Code did not sign in".into()),
                            ),
                        )
                    };
                    auth.finish_login(agent, &rpc, status, message, &notify);
                });
                Ok(LoginStarted {
                    agent,
                    url: None,
                    already_open: false,
                })
            }
            _ => Err(format!(
                "{name} signs in through its own app. Sign in there, then check again."
            )),
        }
    }

    fn finish_login(
        &self,
        agent: AgentKind,
        rpc: &Arc<RpcProcess>,
        status: &'static str,
        message: Option<String>,
        notify: &LoginNotifier,
    ) {
        let _ = rpc.shutdown(Duration::from_millis(800));
        if let Ok(mut logins) = self.logins.lock() {
            if logins
                .get(&agent)
                .is_some_and(|login| Arc::ptr_eq(&login.rpc, rpc))
            {
                logins.remove(&agent);
            }
        }
        self.forget(agent);
        notify(LoginEvent {
            agent,
            status,
            message,
        });
    }

    /// A sign-in that `cancel_login` already removed.
    fn login_cancelled(&self, agent: AgentKind, rpc: &Arc<RpcProcess>) -> bool {
        self.logins.lock().is_ok_and(|logins| {
            !logins
                .get(&agent)
                .is_some_and(|login| Arc::ptr_eq(&login.rpc, rpc))
        })
    }

    pub fn cancel_login(&self, agent: AgentKind) -> Result<(), String> {
        let login = self
            .logins
            .lock()
            .map_err(|_| "sign-in state is unavailable".to_string())?
            .remove(&agent);
        let Some(login) = login else { return Ok(()) };
        if let Some(login_id) = login.login_id.as_ref() {
            let _ = login.rpc.request(
                "account/login/cancel",
                json!({"loginId": login_id}),
                Duration::from_secs(5),
            );
        }
        login.rpc.shutdown(Duration::from_millis(800))
    }

    pub fn shutdown(&self) {
        let logins = self
            .logins
            .lock()
            .map(|mut logins| logins.drain().map(|(_, login)| login).collect::<Vec<_>>())
            .unwrap_or_default();
        for login in logins {
            let _ = login.rpc.shutdown(Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_links_must_be_https_on_the_provider_host() {
        assert!(validate_auth_url(
            "https://auth.openai.com/oauth/authorize?x=1",
            CODEX_AUTH_HOSTS
        )
        .is_ok());
        assert!(validate_auth_url("http://auth.openai.com/oauth", CODEX_AUTH_HOSTS).is_err());
        assert!(validate_auth_url("https://user:pw@auth.openai.com/", CODEX_AUTH_HOSTS).is_err());
        assert!(
            validate_auth_url("https://auth.openai.com.evil.example/", CODEX_AUTH_HOSTS).is_err()
        );
        assert!(validate_auth_url("javascript:alert(1)", CODEX_AUTH_HOSTS).is_err());
        assert!(validate_auth_url("file:///etc/passwd", CODEX_AUTH_HOSTS).is_err());
    }

    #[test]
    fn claude_accounts_read_like_the_cli() {
        let status = json!({"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty", "email": "a@b.c", "subscriptionType": "pro"});
        assert_eq!(
            claude_account_label(&status).as_deref(),
            Some("Claude Pro (a@b.c)")
        );
        let bedrock =
            json!({"loggedIn": true, "authMethod": "third_party", "apiProvider": "bedrock"});
        assert_eq!(claude_account_label(&bedrock).as_deref(), Some("bedrock"));
        assert_eq!(claude_account_label(&json!({"loggedIn": false})), None);
    }

    #[test]
    fn parses_cli_versions() {
        assert_eq!(parse_version("codex-cli 0.166.0"), Some((0, 166, 0)));
        assert_eq!(
            parse_version("grok 1.0.67 (2fe064390d07)"),
            Some((1, 0, 67))
        );
        assert_eq!(parse_version("2.1.3 (Claude Code)"), Some((2, 1, 3)));
        assert!(parse_version("codex-cli 0.162.0").unwrap() < MIN_CODEX_VERSION);
        assert_eq!(parse_version("unknown"), None);
    }
}
