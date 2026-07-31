//! Desktop-owned pseudo terminals for local agent CLIs.
//!
//! The renderer receives an opaque session id and can only write, resize, or
//! kill that session. Commands are intentionally restricted to supported
//! agents; arbitrary command execution is left to the interactive shell.

use crate::local_engine::LocalEngine;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const MIN_COLS: u16 = 20;
const MAX_COLS: u16 = 500;
const MIN_ROWS: u16 = 5;
const MAX_ROWS: u16 = 200;
const MAX_TASK_PROMPT_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Codex,
    Claude,
    Grok,
    Antigravity,
    OpenCode,
    Hermes,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TerminalMode {
    Live,
    Background,
}

impl TerminalMode {
    fn prompt(self) -> &'static str {
        match self {
            Self::Live => "You are editing the current Draft working tree directly. Re-read files immediately before every edit and never overwrite concurrent human or Agent changes.",
            Self::Background => "You are in a CoWiki-managed background worktree captured from the Draft at dispatch time. Only edit files in this worktree. Do not checkout, merge, commit, or push; CoWiki collects the result and merges it into the latest Draft after review.",
        }
    }
}

impl AgentKind {
    fn command(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Grok => "grok",
            Self::Antigravity => "agy",
            Self::OpenCode => "opencode",
            Self::Hermes => "hermes",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TerminalIntent {
    Run,
    Login,
}

impl Default for TerminalIntent {
    fn default() -> Self {
        Self::Run
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentReadinessStatus {
    NotInstalled,
    Broken,
    SignedOut,
    Ready,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReadiness {
    agent: AgentKind,
    status: AgentReadinessStatus,
    executable: Option<String>,
    version: Option<String>,
    auth_method: Option<String>,
    message: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateRequest {
    session_id: String,
    cwd: String,
    mode: TerminalMode,
    space_slug: String,
    change_id: Option<String>,
    agent: AgentKind,
    #[serde(default)]
    intent: TerminalIntent,
    initial_command: Option<String>,
    task_prompt: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreated {
    session_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalDataEvent {
    session_id: String,
    data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalExitEvent {
    session_id: String,
    exit_code: Option<u32>,
}

struct TerminalSession {
    identity: TerminalIdentity,
    // Keeping the master alive owns the PTY and enables resize.
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TerminalIdentity {
    space_slug: String,
    change_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AgentChangeIdentity {
    space_slug: String,
    change_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalPhase {
    Starting,
    Running,
    Stopping,
}

struct TerminalSlot {
    identity: TerminalIdentity,
    phase: TerminalPhase,
    session: Option<TerminalSession>,
}

impl TerminalSlot {
    fn starting(identity: TerminalIdentity) -> Self {
        Self {
            identity,
            phase: TerminalPhase::Starting,
            session: None,
        }
    }

    fn identity(&self) -> &TerminalIdentity {
        &self.identity
    }

    fn running(&self) -> Option<&TerminalSession> {
        if self.phase == TerminalPhase::Running {
            self.session.as_ref()
        } else {
            None
        }
    }

    fn running_mut(&mut self) -> Option<&mut TerminalSession> {
        if self.phase == TerminalPhase::Running {
            self.session.as_mut()
        } else {
            None
        }
    }
}

#[derive(Default)]
struct TerminalRegistry {
    sessions: HashMap<String, TerminalSlot>,
    closing_changes: HashSet<AgentChangeIdentity>,
}

impl TerminalRegistry {
    fn mark_stopping(&mut self, session_id: &str) -> Result<(), String> {
        let slot = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("unknown terminal session: {session_id}"))?;
        match slot.phase {
            TerminalPhase::Starting => Err("terminal session is still starting".to_string()),
            TerminalPhase::Running => {
                slot.phase = TerminalPhase::Stopping;
                Ok(())
            }
            TerminalPhase::Stopping => Ok(()),
        }
    }
}

#[derive(Clone, Default)]
pub struct TerminalState {
    registry: Arc<Mutex<TerminalRegistry>>,
}

pub(crate) struct TerminalChangeGuard {
    registry: Arc<Mutex<TerminalRegistry>>,
    identity: AgentChangeIdentity,
}

struct TerminalStartReservation {
    state: TerminalState,
    session_id: String,
    active: bool,
}

impl TerminalStartReservation {
    fn new(
        state: TerminalState,
        session_id: &str,
        identity: TerminalIdentity,
    ) -> Result<Self, String> {
        state.reserve_session(session_id, identity)?;
        Ok(Self {
            state,
            session_id: session_id.to_string(),
            active: true,
        })
    }

    fn finish(&mut self) {
        self.active = false;
    }
}

impl Drop for TerminalStartReservation {
    fn drop(&mut self) {
        if self.active {
            self.state.release_session(&self.session_id);
        }
    }
}

impl fmt::Debug for TerminalChangeGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalChangeGuard")
            .field("identity", &self.identity)
            .finish()
    }
}

impl Drop for TerminalChangeGuard {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.closing_changes.remove(&self.identity);
        }
    }
}

impl TerminalState {
    fn reserve_session(&self, session_id: &str, identity: TerminalIdentity) -> Result<(), String> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| "terminal state is unavailable".to_string())?;
        if registry.sessions.contains_key(session_id) {
            return Err("terminal session already exists".to_string());
        }
        if let Some(change_id) = identity.change_id.as_ref() {
            let change = AgentChangeIdentity {
                space_slug: identity.space_slug.clone(),
                change_id: change_id.clone(),
            };
            if registry.closing_changes.contains(&change) {
                return Err("Agent Change is being closed; its terminal cannot restart".to_string());
            }
        } else if registry
            .closing_changes
            .iter()
            .any(|change| change.space_slug == identity.space_slug)
        {
            return Err(
                "Agent Change in this Space is being closed; a Live terminal cannot start"
                    .to_string(),
            );
        }
        registry
            .sessions
            .insert(session_id.to_string(), TerminalSlot::starting(identity));
        Ok(())
    }

    fn install_session(&self, session_id: &str, session: TerminalSession) -> Result<(), String> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| "terminal state is unavailable".to_string())?;
        let Some(slot) = registry.sessions.get_mut(session_id) else {
            return Err("terminal session reservation was lost".to_string());
        };
        if slot.phase != TerminalPhase::Starting {
            return Err("terminal session reservation is no longer starting".to_string());
        }
        if slot.identity != session.identity {
            return Err("terminal session identity changed while starting".to_string());
        }
        slot.phase = TerminalPhase::Running;
        slot.session = Some(session);
        Ok(())
    }

    fn release_session(&self, session_id: &str) -> Option<TerminalSession> {
        self.registry
            .lock()
            .ok()?
            .sessions
            .remove(session_id)
            .and_then(|slot| slot.session)
    }

    fn request_stop(&self, session_id: &str) -> Result<(), String> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| "terminal state is unavailable".to_string())?;
        let Some(slot) = registry.sessions.get_mut(session_id) else {
            return Ok(());
        };
        if slot.phase == TerminalPhase::Stopping {
            return Ok(());
        }
        let session = slot
            .running_mut()
            .ok_or_else(|| "terminal session is still starting".to_string())?;
        stop_terminal_processes(session)?;
        registry.mark_stopping(session_id)
    }

    pub(crate) fn begin_change_close(
        &self,
        space_slug: &str,
        change_id: &str,
    ) -> Result<TerminalChangeGuard, String> {
        let identity = AgentChangeIdentity {
            space_slug: space_slug.to_string(),
            change_id: change_id.to_string(),
        };
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| "terminal state is unavailable".to_string())?;
        let blocking_terminal = registry.sessions.values().find(|slot| {
            let terminal = slot.identity();
            terminal.space_slug == identity.space_slug
                && (terminal.change_id.is_none()
                    || terminal.change_id.as_deref() == Some(identity.change_id.as_str()))
        });
        if let Some(slot) = blocking_terminal {
            if slot.identity().change_id.is_none() {
                return Err(
                    "Stop the active Live terminal before merging or discarding an Agent Change in this Space."
                        .to_string(),
                );
            }
            return Err(
                "Stop the active background terminal before merging or discarding this Agent Change."
                    .to_string(),
            );
        }
        if !registry.closing_changes.insert(identity.clone()) {
            return Err("Agent Change is already being closed".to_string());
        }
        drop(registry);
        Ok(TerminalChangeGuard {
            registry: self.registry.clone(),
            identity,
        })
    }
}

#[tauri::command]
pub fn agent_probe(agent: AgentKind) -> AgentReadiness {
    probe_agent(agent)
}

#[tauri::command]
pub fn terminal_create(
    app: AppHandle,
    state: State<'_, TerminalState>,
    local_engine: State<'_, LocalEngine>,
    request: TerminalCreateRequest,
) -> Result<TerminalCreated, String> {
    let session_id = validate_session_id(&request.session_id)?;
    let cwd = resolve_terminal_cwd(
        &local_engine,
        request.mode,
        &request.space_slug,
        request.change_id.as_deref(),
        &request.cwd,
    )?;
    let space = local_engine.find_space(&request.space_slug)?;
    validate_initial_command(request.agent, request.initial_command.as_deref())?;
    if request.intent == TerminalIntent::Login
        && (request.agent != AgentKind::Codex || request.mode != TerminalMode::Live)
    {
        return Err("Codex login must run in a Live setup terminal".to_string());
    }
    let readiness = probe_agent(request.agent);
    let agent_executable = readiness
        .executable
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| readiness_error(&readiness))?;
    match (request.intent, readiness.status) {
        (TerminalIntent::Run, AgentReadinessStatus::Ready) => {}
        (TerminalIntent::Login, AgentReadinessStatus::Ready | AgentReadinessStatus::SignedOut) => {}
        _ => return Err(readiness_error(&readiness)),
    }
    let size = normalized_pty_size(request.cols, request.rows);
    let identity = TerminalIdentity {
        space_slug: request.space_slug.clone(),
        change_id: request.change_id.clone(),
    };
    let mut reservation =
        TerminalStartReservation::new(state.inner().clone(), &session_id, identity.clone())?;

    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate CoWiki MCP executable: {error}"))?;
    if matches!(request.agent, AgentKind::Antigravity) {
        ensure_antigravity_mcp_config(&cwd, &executable, &space.slug)?;
    }
    let task_prompt = validate_task_prompt(request.task_prompt.as_deref())?;
    let agent_launch = build_agent_command(
        request.agent,
        request.mode,
        request.intent,
        &space.slug,
        &agent_executable,
        &executable,
        task_prompt.as_deref(),
    );

    let pair = native_pty_system()
        .openpty(size)
        .map_err(|error| format!("failed to create terminal: {error}"))?;
    let shell = resolve_shell();
    let mut command = CommandBuilder::new(&shell);
    add_shell_command_args(&mut command, &shell, &agent_launch.shell_command);
    command.cwd(&cwd);
    for (key, value) in &agent_launch.environment {
        command.env(key, value);
    }

    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("failed to start terminal shell: {error}"))?;
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to open terminal output: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("failed to open terminal input: {error}"))?;
    let killer = child.clone_killer();

    state.install_session(
        &session_id,
        TerminalSession {
            identity,
            master: pair.master,
            writer,
            killer,
        },
    )?;
    reservation.finish();

    spawn_output_forwarder(app.clone(), session_id.clone(), reader);
    spawn_exit_watcher(app, session_id.clone(), state.inner().clone(), child);

    Ok(TerminalCreated { session_id })
}

fn validate_session_id(value: &str) -> Result<String, String> {
    let uuid = value
        .strip_prefix("terminal:")
        .ok_or_else(|| "invalid terminal session id".to_string())?;
    uuid::Uuid::parse_str(uuid).map_err(|_| "invalid terminal session id".to_string())?;
    Ok(value.to_string())
}

fn validate_task_prompt(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > MAX_TASK_PROMPT_BYTES {
        return Err(format!(
            "Agent task is too long (maximum {MAX_TASK_PROMPT_BYTES} bytes)"
        ));
    }
    if value.contains('\0') {
        return Err("Agent task contains an invalid null byte".to_string());
    }
    Ok(Some(value.to_string()))
}

const SPACE_PROTOCOL: &str = "You are maintaining a CoWiki knowledge Space. Markdown files are the source of truth. Follow the Open Knowledge Format, the Space's own rules, and its arbitrary OKF hierarchy; never invent fixed wiki, entities, or concepts directories. Local maintenance never requires a CoWiki API key or server. Before answering about the Space, search and read the relevant pages, then cite their relative paths. When the cowiki MCP tools are available, use them for retrieval; otherwise use normal file and text-search tools. Before claiming that knowledge is absent, search and list evidence first. Raw sources are immutable. Integrate durable knowledge into the maintained wiki, reconcile contradictions, keep links/index/log consistent, and make every change reversible. Re-read a file immediately before editing it; never silently overwrite concurrent human changes. Do not commit, checkout, merge, push, or edit CoWiki SQLite metadata.";

const AGENT_PROMPT_ENV: &str = "COWIKI_AGENT_PROMPT";
const CLAUDE_MCP_CONFIG_ENV: &str = "COWIKI_CLAUDE_MCP_CONFIG";
const CODEX_MCP_COMMAND_ENV: &str = "COWIKI_CODEX_MCP_COMMAND";
const CODEX_MCP_ARGS_ENV: &str = "COWIKI_CODEX_MCP_ARGS";
const HERMES_EPHEMERAL_SYSTEM_PROMPT_ENV: &str = "HERMES_EPHEMERAL_SYSTEM_PROMPT";

struct AgentLaunchCommand {
    shell_command: String,
    environment: Vec<(&'static str, String)>,
}

#[cfg(test)]
impl AgentLaunchCommand {
    fn environment_value(&self, key: &str) -> Option<&str> {
        self.environment
            .iter()
            .find_map(|(candidate, value)| (*candidate == key).then_some(value.as_str()))
    }
}

fn build_agent_command(
    agent: AgentKind,
    mode: TerminalMode,
    intent: TerminalIntent,
    space_slug: &str,
    agent_executable: &Path,
    executable: &Path,
    task_prompt: Option<&str>,
) -> AgentLaunchCommand {
    let agent_command = shell_quote(&agent_executable.to_string_lossy());
    if intent == TerminalIntent::Login {
        return AgentLaunchCommand {
            shell_command: format!("exec {agent_command} login"),
            environment: Vec::new(),
        };
    }
    let executable_text = executable.to_string_lossy();
    let mcp_args = vec!["--mcp", "--space", space_slug];
    let prompt = match task_prompt {
        Some(task) => format!(
            "{SPACE_PROTOCOL} {} The app assigned this current task; begin with it now:\n{task}",
            mode.prompt()
        ),
        None => format!("{SPACE_PROTOCOL} {}", mode.prompt()),
    };
    let prompt_environment = || vec![(AGENT_PROMPT_ENV, prompt.clone())];
    match agent {
        AgentKind::Claude => {
            let config = serde_json::json!({
                "mcpServers": {
                    "cowiki": {
                        "type": "stdio",
                        "command": executable_text,
                        "args": mcp_args,
                    }
                }
            });
            AgentLaunchCommand {
                shell_command: format!(
                    "exec {agent_command} --mcp-config \"${CLAUDE_MCP_CONFIG_ENV}\" \
                     --append-system-prompt \"${AGENT_PROMPT_ENV}\""
                ),
                environment: vec![
                    (AGENT_PROMPT_ENV, prompt),
                    (CLAUDE_MCP_CONFIG_ENV, config.to_string()),
                ],
            }
        }
        AgentKind::Codex => {
            let command_toml = toml_string(&executable_text);
            let args_toml = format!(
                "[{}]",
                mcp_args
                    .iter()
                    .map(|argument| toml_string(argument))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            AgentLaunchCommand {
                shell_command: format!(
                    "exec {agent_command} --no-alt-screen -c \"${CODEX_MCP_COMMAND_ENV}\" \
                     -c \"${CODEX_MCP_ARGS_ENV}\" \"${AGENT_PROMPT_ENV}\""
                ),
                environment: vec![
                    (AGENT_PROMPT_ENV, prompt),
                    (
                        CODEX_MCP_COMMAND_ENV,
                        format!("mcp_servers.cowiki.command={command_toml}"),
                    ),
                    (
                        CODEX_MCP_ARGS_ENV,
                        format!("mcp_servers.cowiki.args={args_toml}"),
                    ),
                ],
            }
        }
        AgentKind::Grok => AgentLaunchCommand {
            shell_command: format!("exec {agent_command} --rules \"${AGENT_PROMPT_ENV}\""),
            environment: prompt_environment(),
        },
        AgentKind::Antigravity => AgentLaunchCommand {
            shell_command: format!(
                "exec {agent_command} --dangerously-skip-permissions --prompt-interactive \"${AGENT_PROMPT_ENV}\""
            ),
            environment: prompt_environment(),
        },
        AgentKind::OpenCode => AgentLaunchCommand {
            shell_command: format!("exec {agent_command} --prompt \"${AGENT_PROMPT_ENV}\""),
            environment: prompt_environment(),
        },
        // Hermes supports an ephemeral, session-only system prompt through
        // this environment variable. It avoids mutating the user's global
        // skills or writing Agent instructions into their Space.
        AgentKind::Hermes => AgentLaunchCommand {
            shell_command: format!("exec {agent_command} chat"),
            environment: vec![(HERMES_EPHEMERAL_SYSTEM_PROMPT_ENV, prompt)],
        },
    }
}

fn readiness_error(readiness: &AgentReadiness) -> String {
    readiness
        .message
        .clone()
        .unwrap_or_else(|| format!("{} is not ready", readiness.agent.command()))
}

fn probe_agent(agent: AgentKind) -> AgentReadiness {
    probe_agent_in(agent, agent_search_directories())
}

fn probe_agent_in(
    agent: AgentKind,
    directories: impl IntoIterator<Item = PathBuf>,
) -> AgentReadiness {
    let Some(executable) = resolve_agent_executable_in(agent, directories) else {
        return AgentReadiness {
            agent,
            status: AgentReadinessStatus::NotInstalled,
            executable: None,
            version: None,
            auth_method: None,
            message: Some(format!(
                "{} is not installed or is not on PATH",
                agent.command()
            )),
            detail: None,
        };
    };
    let executable_text = executable.to_string_lossy().into_owned();
    let version_output =
        match run_probe_command(&executable, &["--version"], Duration::from_secs(4)) {
            Ok(output) => output,
            Err(error) => {
                return AgentReadiness {
                    agent,
                    status: AgentReadinessStatus::Broken,
                    executable: Some(executable_text),
                    version: None,
                    auth_method: None,
                    message: Some(format!("{} could not be checked", agent.command())),
                    detail: Some(error),
                };
            }
        };
    if !version_output.status.success() {
        return AgentReadiness {
            agent,
            status: AgentReadinessStatus::Broken,
            executable: Some(executable_text),
            version: None,
            auth_method: None,
            message: Some(format!("{} is installed but cannot start", agent.command())),
            detail: Some(probe_output_text(&version_output)),
        };
    }
    let version = first_nonempty_line(&probe_output_text(&version_output));

    if agent != AgentKind::Codex {
        return AgentReadiness {
            agent,
            status: AgentReadinessStatus::Ready,
            executable: Some(executable_text),
            version,
            auth_method: None,
            message: None,
            detail: None,
        };
    }

    let auth_output =
        match run_probe_command(&executable, &["login", "status"], Duration::from_secs(5)) {
            Ok(output) => output,
            Err(error) => {
                return AgentReadiness {
                    agent,
                    status: AgentReadinessStatus::SignedOut,
                    executable: Some(executable_text),
                    version,
                    auth_method: None,
                    message: Some("Codex authentication could not be verified".to_string()),
                    detail: Some(error),
                };
            }
        };
    let auth_text = probe_output_text(&auth_output);
    if !auth_output.status.success() {
        return AgentReadiness {
            agent,
            status: AgentReadinessStatus::SignedOut,
            executable: Some(executable_text),
            version,
            auth_method: None,
            message: Some("Codex is installed but not signed in".to_string()),
            detail: (!auth_text.is_empty()).then_some(auth_text),
        };
    }

    AgentReadiness {
        agent,
        status: AgentReadinessStatus::Ready,
        executable: Some(executable_text),
        version,
        auth_method: first_nonempty_line(&auth_text),
        message: None,
        detail: None,
    }
}

fn resolve_agent_executable_in(
    agent: AgentKind,
    directories: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    directories
        .into_iter()
        .filter(|directory| directory.is_absolute())
        .filter_map(|directory| directory.join(agent.command()).canonicalize().ok())
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(candidate: &Path) -> bool {
    let Ok(metadata) = candidate.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn agent_search_directories() -> Vec<PathBuf> {
    let mut directories = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    directories.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]);
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        directories.extend([
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".npm-global/bin"),
        ]);
    }
    let mut unique = Vec::new();
    for directory in directories {
        if !unique.contains(&directory) {
            unique.push(directory);
        }
    }
    unique
        .into_iter()
        .filter(|directory| directory.is_absolute())
        .collect()
}

fn run_probe_command(
    executable: &Path,
    arguments: &[&str],
    timeout: Duration,
) -> Result<Output, String> {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(path) = std::env::join_paths(agent_search_directories()) {
        command.env("PATH", path);
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(_) => return child.wait_with_output().map_err(|error| error.to_string()),
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} readiness check timed out",
                    executable.display()
                ));
            }
        }
    }
}

fn probe_output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = [stdout.trim(), stderr.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    combined.chars().take(2_000).collect()
}

fn first_nonempty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn ensure_antigravity_mcp_config(
    cwd: &Path,
    executable: &Path,
    space_slug: &str,
) -> Result<(), String> {
    let agents_dir = cwd.join(".agents");
    let config_path = agents_dir.join("mcp_config.json");
    fs::create_dir_all(&agents_dir).map_err(|error| {
        format!(
            "failed to create Antigravity configuration directory {}: {error}",
            agents_dir.display()
        )
    })?;

    let mut config = if config_path.exists() {
        let bytes = fs::read(&config_path).map_err(|error| {
            format!(
                "failed to read Antigravity MCP configuration {}: {error}",
                config_path.display()
            )
        })?;
        serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|error| {
            format!(
                "Antigravity MCP configuration {} is invalid JSON: {error}",
                config_path.display()
            )
        })?
    } else {
        serde_json::json!({})
    };

    let root = config.as_object_mut().ok_or_else(|| {
        format!(
            "Antigravity MCP configuration {} must contain a JSON object",
            config_path.display()
        )
    })?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            format!(
                "Antigravity MCP configuration {} must contain an mcpServers object",
                config_path.display()
            )
        })?;
    servers.insert(
        "cowiki".to_string(),
        serde_json::json!({
            "command": executable.to_string_lossy(),
            "args": ["--mcp", "--space", space_slug],
        }),
    );

    let serialized = serde_json::to_vec_pretty(&config)
        .map_err(|error| format!("failed to serialize Antigravity MCP configuration: {error}"))?;
    fs::write(&config_path, serialized).map_err(|error| {
        format!(
            "failed to write Antigravity MCP configuration {}: {error}",
            config_path.display()
        )
    })
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('\"', "\\\""))
}

#[tauri::command]
pub fn terminal_write(
    state: State<'_, TerminalState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let mut registry = state
        .registry
        .lock()
        .map_err(|_| "terminal state is unavailable".to_string())?;
    let session = registry
        .sessions
        .get_mut(&session_id)
        .and_then(TerminalSlot::running_mut)
        .ok_or_else(|| format!("unknown terminal session: {session_id}"))?;
    session
        .writer
        .write_all(data.as_bytes())
        .and_then(|_| session.writer.flush())
        .map_err(|error| format!("failed to write terminal input: {error}"))
}

#[tauri::command]
pub fn terminal_resize(
    state: State<'_, TerminalState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let registry = state
        .registry
        .lock()
        .map_err(|_| "terminal state is unavailable".to_string())?;
    let session = registry
        .sessions
        .get(&session_id)
        .and_then(TerminalSlot::running)
        .ok_or_else(|| format!("unknown terminal session: {session_id}"))?;
    session
        .master
        .resize(normalized_pty_size(Some(cols), Some(rows)))
        .map_err(|error| format!("failed to resize terminal: {error}"))
}

#[tauri::command]
pub fn terminal_kill(state: State<'_, TerminalState>, session_id: String) -> Result<(), String> {
    state.request_stop(&session_id)
}

fn stop_terminal_processes(session: &mut TerminalSession) -> Result<(), String> {
    #[cfg(unix)]
    if let Some(process_group) = session.master.process_group_leader() {
        // portable-pty makes the shell a session leader, but its cloned killer
        // signals only the shell PID. Signal the current foreground process
        // group as well so an interactive Agent CLI is asked to stop before
        // the shell. The registry remains Stopping until child.wait confirms
        // that the PTY child has actually exited.
        let result = unsafe { libc::kill(-process_group, libc::SIGHUP) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(format!("failed to stop terminal process group: {error}"));
            }
        }
    }
    match session.killer.kill() {
        Ok(()) => Ok(()),
        #[cfg(unix)]
        Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
        Err(error) => Err(format!("failed to stop terminal: {error}")),
    }
}

fn spawn_output_forwarder(app: AppHandle, session_id: String, mut reader: Box<dyn Read + Send>) {
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let data = String::from_utf8_lossy(&buffer[..read]).into_owned();
                    let _ = app.emit(
                        "terminal:data",
                        TerminalDataEvent {
                            session_id: session_id.clone(),
                            data,
                        },
                    );
                }
            }
        }
    });
}

fn spawn_exit_watcher(
    app: AppHandle,
    session_id: String,
    state: TerminalState,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
) {
    std::thread::spawn(move || {
        let exit_code = child.wait().ok().map(|status| status.exit_code());
        state.release_session(&session_id);
        let _ = app.emit(
            "terminal:exit",
            TerminalExitEvent {
                session_id,
                exit_code,
            },
        );
    });
}

fn canonical_space_path(raw_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw_path);
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("invalid Space directory: {error}"))?;
    if !canonical.is_dir() {
        return Err("Space path must be a directory".to_string());
    }
    Ok(canonical)
}

fn resolve_terminal_cwd(
    local_engine: &LocalEngine,
    mode: TerminalMode,
    space_slug: &str,
    change_id: Option<&str>,
    raw_cwd: &str,
) -> Result<PathBuf, String> {
    let requested = canonical_space_path(raw_cwd)?;
    let space = local_engine.find_space(space_slug)?;
    let expected = match mode {
        TerminalMode::Live => {
            if change_id.is_some() {
                return Err("a Live terminal cannot claim an Agent Change".to_string());
            }
            space
                .local_path
                .canonicalize()
                .map_err(|error| error.to_string())?
        }
        TerminalMode::Background => {
            let change_id = change_id
                .ok_or_else(|| "a background terminal requires an Agent Change".to_string())?;
            local_engine.agent_change_worktree(space_slug, change_id)?
        }
    };
    if requested != expected {
        return Err("terminal directory does not match its Space and Agent Change".to_string());
    }
    Ok(requested)
}

fn validate_initial_command(agent: AgentKind, initial_command: Option<&str>) -> Result<(), String> {
    if initial_command.is_some_and(|command| command != agent.command()) {
        return Err("terminal initial command must match the selected agent".to_string());
    }
    Ok(())
}

fn normalized_pty_size(cols: Option<u16>, rows: Option<u16>) -> PtySize {
    PtySize {
        cols: cols.unwrap_or(DEFAULT_COLS).clamp(MIN_COLS, MAX_COLS),
        rows: rows.unwrap_or(DEFAULT_ROWS).clamp(MIN_ROWS, MAX_ROWS),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn resolve_shell() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("COMSPEC")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("powershell.exe"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("SHELL")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from("/bin/sh"))
    }
}

fn add_shell_command_args(command: &mut CommandBuilder, shell: &Path, agent_command: &str) {
    #[cfg(not(windows))]
    {
        if shell
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                matches!(
                    name,
                    "bash" | "csh" | "dash" | "fish" | "ksh" | "sh" | "tcsh" | "zsh"
                )
            })
        {
            command.arg("-l");
            command.arg("-i");
        }
        command.arg("-c");
        command.arg(agent_command);
    }

    #[cfg(windows)]
    {
        let _ = shell;
        command.arg("-NoLogo");
        command.arg("-Command");
        command.arg(agent_command);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_agent_command, ensure_antigravity_mcp_config, normalized_pty_size, probe_agent_in,
        resolve_agent_executable_in, resolve_terminal_cwd, validate_initial_command,
        validate_session_id, validate_task_prompt, AgentKind, AgentReadinessStatus,
        TerminalIdentity, TerminalIntent, TerminalMode, TerminalState, MAX_TASK_PROMPT_BYTES,
    };
    #[cfg(unix)]
    use super::{TerminalPhase, TerminalSession};
    use crate::local_engine::LocalEngine;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    #[cfg(unix)]
    use std::path::PathBuf;

    #[test]
    fn clamps_terminal_dimensions() {
        let size = normalized_pty_size(Some(1), Some(999));
        assert_eq!(size.cols, 20);
        assert_eq!(size.rows, 200);
    }

    #[test]
    fn accepts_only_the_selected_agent_as_initial_command() {
        assert!(validate_initial_command(AgentKind::Codex, Some("codex")).is_ok());
        assert!(validate_initial_command(AgentKind::Claude, Some("claude")).is_ok());
        assert!(validate_initial_command(AgentKind::Grok, Some("grok")).is_ok());
        assert!(validate_initial_command(AgentKind::Antigravity, Some("agy")).is_ok());
        assert!(validate_initial_command(AgentKind::OpenCode, Some("opencode")).is_ok());
        assert!(validate_initial_command(AgentKind::Hermes, Some("hermes")).is_ok());
        assert!(validate_initial_command(AgentKind::Codex, Some("rm -rf ~")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolves_only_absolute_executable_agent_paths() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("codex");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();

        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o644);
        std::fs::set_permissions(&executable, permissions).unwrap();
        assert!(
            resolve_agent_executable_in(AgentKind::Codex, [directory.path().to_path_buf()])
                .is_none()
        );

        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions).unwrap();
        assert_eq!(
            resolve_agent_executable_in(AgentKind::Codex, [directory.path().to_path_buf()]),
            Some(executable.canonicalize().unwrap())
        );
        assert!(
            resolve_agent_executable_in(AgentKind::Codex, [PathBuf::from("relative/bin")])
                .is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn distinguishes_missing_signed_out_and_ready_codex_installations() {
        let directory = tempfile::tempdir().unwrap();
        let search_path = || [directory.path().to_path_buf()];

        let missing = probe_agent_in(AgentKind::Codex, search_path());
        assert_eq!(missing.status, AgentReadinessStatus::NotInstalled);

        let executable = directory.path().join("codex");
        std::fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli test'; exit 0; fi\nif [ \"$1\" = \"login\" ]; then echo 'Not logged in' >&2; exit 1; fi\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions).unwrap();

        let signed_out = probe_agent_in(AgentKind::Codex, search_path());
        assert_eq!(signed_out.status, AgentReadinessStatus::SignedOut);
        assert_eq!(signed_out.version.as_deref(), Some("codex-cli test"));

        std::fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli test'; exit 0; fi\nif [ \"$1\" = \"login\" ]; then echo 'Logged in using browser'; exit 0; fi\n",
        )
        .unwrap();
        let ready = probe_agent_in(AgentKind::Codex, search_path());
        assert_eq!(ready.status, AgentReadinessStatus::Ready);
        assert_eq!(
            ready.auth_method.as_deref(),
            Some("Logged in using browser")
        );
    }

    #[test]
    fn terminal_session_ids_are_renderer_known_uuids() {
        let id = format!("terminal:{}", uuid::Uuid::new_v4());
        assert_eq!(validate_session_id(&id).unwrap(), id);
        assert!(validate_session_id("terminal:not-a-uuid").is_err());
        assert!(validate_session_id("other:00000000-0000-0000-0000-000000000000").is_err());
    }

    #[test]
    fn launches_agents_with_read_only_cowiki_mcp_and_maintenance_protocol() {
        let executable = Path::new("/Applications/CoWiki.app/Contents/MacOS/cowiki-desktop");
        let codex = build_agent_command(
            AgentKind::Codex,
            TerminalMode::Live,
            TerminalIntent::Run,
            "research-space",
            Path::new("/opt/homebrew/bin/codex"),
            executable,
            Some("Organize sources/_encoded/example.md"),
        );
        let claude = build_agent_command(
            AgentKind::Claude,
            TerminalMode::Background,
            TerminalIntent::Run,
            "research-space",
            Path::new("/opt/homebrew/bin/claude"),
            executable,
            None,
        );

        assert!(codex
            .shell_command
            .starts_with("exec '/opt/homebrew/bin/codex' --no-alt-screen -c "));
        assert!(codex
            .environment_value("COWIKI_CODEX_MCP_COMMAND")
            .unwrap()
            .contains("mcp_servers.cowiki.command"));
        assert!(codex
            .environment_value("COWIKI_CODEX_MCP_ARGS")
            .unwrap()
            .contains("research-space"));
        assert!(codex
            .environment_value("COWIKI_AGENT_PROMPT")
            .unwrap()
            .contains("Before claiming that knowledge is absent"));
        assert!(codex
            .environment_value("COWIKI_AGENT_PROMPT")
            .unwrap()
            .contains("current Draft working tree"));
        assert!(codex
            .environment_value("COWIKI_AGENT_PROMPT")
            .unwrap()
            .contains("Organize sources/_encoded/example.md"));
        assert!(claude.shell_command.contains("--mcp-config"));
        assert!(claude.shell_command.contains("--append-system-prompt"));
        assert!(claude
            .environment_value("COWIKI_CLAUDE_MCP_CONFIG")
            .unwrap()
            .contains("cowiki"));
        assert!(claude
            .environment_value("COWIKI_AGENT_PROMPT")
            .unwrap()
            .contains("managed background worktree"));
        assert!(claude
            .environment_value("COWIKI_AGENT_PROMPT")
            .unwrap()
            .contains("Do not checkout, merge, commit, or push"));

        let grok = build_agent_command(
            AgentKind::Grok,
            TerminalMode::Live,
            TerminalIntent::Run,
            "research-space",
            Path::new("/opt/homebrew/bin/grok"),
            executable,
            None,
        );
        let antigravity = build_agent_command(
            AgentKind::Antigravity,
            TerminalMode::Live,
            TerminalIntent::Run,
            "research-space",
            Path::new("/opt/homebrew/bin/agy"),
            executable,
            None,
        );
        let opencode = build_agent_command(
            AgentKind::OpenCode,
            TerminalMode::Live,
            TerminalIntent::Run,
            "research-space",
            Path::new("/opt/homebrew/bin/opencode"),
            executable,
            None,
        );
        let hermes = build_agent_command(
            AgentKind::Hermes,
            TerminalMode::Live,
            TerminalIntent::Run,
            "research-space",
            Path::new("/opt/homebrew/bin/hermes"),
            executable,
            None,
        );

        assert!(grok
            .shell_command
            .starts_with("exec '/opt/homebrew/bin/grok' --rules "));
        assert!(grok
            .environment_value("COWIKI_AGENT_PROMPT")
            .unwrap()
            .contains("Before claiming that knowledge is absent"));
        assert!(antigravity.shell_command.starts_with(
            "exec '/opt/homebrew/bin/agy' --dangerously-skip-permissions --prompt-interactive "
        ));
        assert!(antigravity
            .environment_value("COWIKI_AGENT_PROMPT")
            .unwrap()
            .contains("Before claiming that knowledge is absent"));
        assert!(opencode
            .shell_command
            .starts_with("exec '/opt/homebrew/bin/opencode' --prompt "));
        assert!(opencode
            .environment_value("COWIKI_AGENT_PROMPT")
            .unwrap()
            .contains("Before claiming that knowledge is absent"));
        assert_eq!(hermes.shell_command, "exec '/opt/homebrew/bin/hermes' chat");
        assert!(hermes
            .environment_value("HERMES_EPHEMERAL_SYSTEM_PROMPT")
            .unwrap()
            .contains("otherwise use normal file and text-search tools"));
    }

    #[test]
    fn antigravity_workspace_config_preserves_other_mcp_servers() {
        let temp = tempfile::tempdir().unwrap();
        let agents_dir = temp.path().join(".agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("mcp_config.json"),
            r#"{"mcpServers":{"existing":{"command":"existing-mcp","args":["serve"]}}}"#,
        )
        .unwrap();
        let executable = Path::new("/Applications/CoWiki.app/Contents/MacOS/cowiki-desktop");

        ensure_antigravity_mcp_config(temp.path(), executable, "research-space").unwrap();

        let config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(agents_dir.join("mcp_config.json")).unwrap())
                .unwrap();
        assert_eq!(config["mcpServers"]["existing"]["command"], "existing-mcp");
        assert_eq!(
            config["mcpServers"]["cowiki"]["command"],
            executable.to_string_lossy().as_ref()
        );
        assert_eq!(
            config["mcpServers"]["cowiki"]["args"],
            serde_json::json!(["--mcp", "--space", "research-space"])
        );
    }

    #[test]
    fn agent_launch_command_fits_macos_canonical_terminal_input() {
        let executable = Path::new(
            "/Users/huangqinghao/Workspace/ClaudeSpace/Workspace/cowiki/.worktrees/local-agent-change-reviews/web/src-tauri/target/debug/cowiki-desktop",
        );
        let claude = build_agent_command(
            AgentKind::Claude,
            TerminalMode::Background,
            TerminalIntent::Run,
            "general-5371996c",
            Path::new("/opt/homebrew/bin/claude"),
            executable,
            None,
        );

        assert!(
            claude.shell_command.len() < 1024,
            "Agent command is {} bytes and will be truncated by macOS MAX_CANON",
            claude.shell_command.len(),
        );
    }

    #[test]
    fn validates_renderer_supplied_agent_tasks() {
        assert_eq!(
            validate_task_prompt(Some("  Organize sources/example.md  ")).unwrap(),
            Some("Organize sources/example.md".to_string())
        );
        assert!(validate_task_prompt(Some("bad\0task")).is_err());
        assert!(validate_task_prompt(Some(&"x".repeat(MAX_TASK_PROMPT_BYTES + 1))).is_err());
    }

    #[test]
    fn terminal_mode_and_identity_control_the_only_allowed_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let notes = temp.path().join("notes");
        let other = temp.path().join("other");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let notes_space = engine.add_space("Notes", "notes", &notes).unwrap();
        let other_space = engine.add_space("Other", "other", &other).unwrap();
        let notes_change = engine
            .create_agent_change(&notes_space.slug, "Codex")
            .unwrap();
        let other_change = engine
            .create_agent_change(&other_space.slug, "Claude Code")
            .unwrap();

        assert_eq!(
            resolve_terminal_cwd(
                &engine,
                TerminalMode::Live,
                &notes_space.slug,
                None,
                notes.to_str().unwrap(),
            )
            .unwrap(),
            notes.canonicalize().unwrap()
        );
        assert!(resolve_terminal_cwd(
            &engine,
            TerminalMode::Live,
            &notes_space.slug,
            None,
            other.to_str().unwrap(),
        )
        .is_err());
        assert_eq!(
            resolve_terminal_cwd(
                &engine,
                TerminalMode::Background,
                &notes_space.slug,
                Some(&notes_change.id),
                notes_change.worktree_path.to_str().unwrap(),
            )
            .unwrap(),
            notes_change.worktree_path.canonicalize().unwrap()
        );
        assert!(resolve_terminal_cwd(
            &engine,
            TerminalMode::Background,
            &notes_space.slug,
            Some(&notes_change.id),
            notes.to_str().unwrap(),
        )
        .is_err());
        assert!(resolve_terminal_cwd(
            &engine,
            TerminalMode::Background,
            &notes_space.slug,
            Some(&other_change.id),
            other_change.worktree_path.to_str().unwrap(),
        )
        .is_err());
    }

    #[test]
    fn active_background_terminals_block_close_and_closing_blocks_restart() {
        let state = TerminalState::default();
        let identity = TerminalIdentity {
            space_slug: "notes".to_string(),
            change_id: Some("change-1".to_string()),
        };
        state
            .reserve_session("terminal:active", identity.clone())
            .unwrap();

        let active_error = state.begin_change_close("notes", "change-1").unwrap_err();
        assert!(active_error.contains("active background terminal"));

        state.release_session("terminal:active");
        let closing = state.begin_change_close("notes", "change-1").unwrap();
        let restart_error = state
            .reserve_session("terminal:restart", identity.clone())
            .unwrap_err();
        assert!(restart_error.contains("being closed"));

        drop(closing);
        state.reserve_session("terminal:restart", identity).unwrap();
    }

    #[test]
    fn live_terminals_share_the_space_close_gate() {
        let state = TerminalState::default();
        let live = TerminalIdentity {
            space_slug: "notes".to_string(),
            change_id: None,
        };
        state
            .reserve_session("terminal:live", live.clone())
            .unwrap();

        let active_error = state.begin_change_close("notes", "change-1").unwrap_err();
        assert!(active_error.contains("active Live terminal"));

        state.release_session("terminal:live");
        let closing = state.begin_change_close("notes", "change-1").unwrap();
        let restart_error = state
            .reserve_session("terminal:live-restart", live)
            .unwrap_err();
        assert!(restart_error.contains("being closed"));
        assert!(state
            .reserve_session(
                "terminal:other-space",
                TerminalIdentity {
                    space_slug: "other".to_string(),
                    change_id: None,
                },
            )
            .is_ok());
        drop(closing);
    }

    #[cfg(unix)]
    #[test]
    fn stopping_terminal_blocks_close_until_exit_releases_its_slot() {
        let state = TerminalState::default();
        let pair = portable_pty::native_pty_system()
            .openpty(normalized_pty_size(None, None))
            .unwrap();
        let mut command = portable_pty::CommandBuilder::new("/bin/sh");
        command.arg("-c");
        command.arg("sleep 10");
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let writer = pair.master.take_writer().unwrap();
        let killer = child.clone_killer();
        let identity = TerminalIdentity {
            space_slug: "notes".to_string(),
            change_id: Some("change-1".to_string()),
        };
        state
            .reserve_session("terminal:stopping", identity.clone())
            .unwrap();
        state
            .install_session(
                "terminal:stopping",
                TerminalSession {
                    identity,
                    master: pair.master,
                    writer,
                    killer,
                },
            )
            .unwrap();

        state.request_stop("terminal:stopping").unwrap();
        {
            let registry = state.registry.lock().unwrap();
            assert_eq!(
                registry.sessions["terminal:stopping"].phase,
                TerminalPhase::Stopping
            );
        }

        let stopping_error = state.begin_change_close("notes", "change-1").unwrap_err();
        assert!(stopping_error.contains("active background terminal"));

        let exited = (0..100).any(|_| {
            if child.try_wait().unwrap().is_some() {
                true
            } else {
                std::thread::sleep(std::time::Duration::from_millis(10));
                false
            }
        });
        assert!(exited, "terminal child did not exit after Stop");
        // This is the exit watcher's only release point after child.wait().
        state.release_session("terminal:stopping");
        assert!(state.begin_change_close("notes", "change-1").is_ok());
    }

    #[test]
    fn closed_agent_change_cannot_restart_a_background_terminal() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let notes = temp.path().join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        let space = engine.add_space("Notes", "notes", &notes).unwrap();
        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        engine
            .discard_agent_change(&space.slug, &change.id)
            .unwrap();

        assert!(resolve_terminal_cwd(
            &engine,
            TerminalMode::Background,
            &space.slug,
            Some(&change.id),
            change.worktree_path.to_str().unwrap(),
        )
        .is_err());
    }
}
