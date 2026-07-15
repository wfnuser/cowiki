//! Desktop-owned pseudo terminals for local agent CLIs.
//!
//! The renderer receives an opaque session id and can only write, resize, or
//! kill that session. Commands are intentionally restricted to supported
//! agents; arbitrary command execution is left to the interactive shell.

use crate::local_engine::LocalEngine;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const MIN_COLS: u16 = 20;
const MAX_COLS: u16 = 500;
const MIN_ROWS: u16 = 5;
const MAX_ROWS: u16 = 200;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Codex,
    Claude,
    Grok,
    Gemini,
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
            Self::Gemini => "gemini",
            Self::OpenCode => "opencode",
            Self::Hermes => "hermes",
        }
    }
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
    initial_command: Option<String>,
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
    // Keeping the master alive owns the PTY and enables resize.
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Clone, Default)]
pub struct TerminalState {
    sessions: Arc<Mutex<HashMap<String, TerminalSession>>>,
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
    let size = normalized_pty_size(request.cols, request.rows);

    let pair = native_pty_system()
        .openpty(size)
        .map_err(|error| format!("failed to create terminal: {error}"))?;
    let shell = resolve_shell();
    let mut command = CommandBuilder::new(&shell);
    add_shell_args(&mut command, &shell);
    command.cwd(&cwd);

    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("failed to start terminal shell: {error}"))?;
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to open terminal output: {error}"))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("failed to open terminal input: {error}"))?;
    let killer = child.clone_killer();
    // Queue a fully controlled command in the login shell. Codex and Claude
    // receive CoWiki's read-only MCP tools directly; every CLI that supports
    // an initial instruction also receives the maintenance protocol. The
    // renderer cannot provide arbitrary flags or commands.
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate CoWiki MCP executable: {error}"))?;
    let agent_command = build_agent_command(request.agent, request.mode, &space.slug, &executable);
    writer
        .write_all(format!("{agent_command}\r").as_bytes())
        .and_then(|_| writer.flush())
        .map_err(|error| format!("failed to start {}: {error}", request.agent.command()))?;

    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| "terminal state is unavailable".to_string())?;
    if sessions.contains_key(&session_id) {
        return Err("terminal session already exists".to_string());
    }
    sessions.insert(
        session_id.clone(),
        TerminalSession {
            master: pair.master,
            writer,
            killer,
        },
    );
    drop(sessions);

    spawn_output_forwarder(app.clone(), session_id.clone(), reader);
    spawn_exit_watcher(app, session_id.clone(), state.sessions.clone(), child);

    Ok(TerminalCreated { session_id })
}

fn validate_session_id(value: &str) -> Result<String, String> {
    let uuid = value
        .strip_prefix("terminal:")
        .ok_or_else(|| "invalid terminal session id".to_string())?;
    uuid::Uuid::parse_str(uuid).map_err(|_| "invalid terminal session id".to_string())?;
    Ok(value.to_string())
}

const SPACE_PROTOCOL: &str = "You are maintaining a CoWiki knowledge Space. Markdown files are the source of truth. Follow the Open Knowledge Format and the Space's own rules. Before answering about the Space, use the cowiki MCP search tools and read the relevant pages; cite their relative paths. Before claiming that knowledge is absent, search and list evidence first. Raw sources are immutable. Integrate durable knowledge into the maintained wiki, reconcile contradictions, keep links/index/log consistent, and make every change reversible. Re-read a file immediately before editing it; never silently overwrite concurrent human changes. Do not commit, checkout, merge, push, or edit CoWiki SQLite metadata unless the user explicitly asks.";

fn build_agent_command(
    agent: AgentKind,
    mode: TerminalMode,
    space_slug: &str,
    executable: &Path,
) -> String {
    let executable_text = executable.to_string_lossy();
    let mcp_args = vec!["--mcp", "--space", space_slug];
    let prompt = format!("{SPACE_PROTOCOL} {}", mode.prompt());
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
            format!(
                "claude --mcp-config {} --append-system-prompt {}",
                shell_quote(&config.to_string()),
                shell_quote(&prompt),
            )
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
            format!(
                "codex -c {} -c {} {}",
                shell_quote(&format!("mcp_servers.cowiki.command={command_toml}")),
                shell_quote(&format!("mcp_servers.cowiki.args={args_toml}")),
                shell_quote(&prompt),
            )
        }
        AgentKind::Grok => format!("grok --rules {}", shell_quote(&prompt)),
        AgentKind::Gemini => format!("gemini --prompt-interactive {}", shell_quote(&prompt)),
        AgentKind::OpenCode => {
            format!("opencode --prompt {}", shell_quote(&prompt))
        }
        // Hermes discovers the Space's AGENTS.md and skills from its working
        // directory. Its interactive command has no system-prompt override.
        AgentKind::Hermes => "hermes chat".to_string(),
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| "terminal state is unavailable".to_string())?;
    let session = sessions
        .get_mut(&session_id)
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
    let sessions = state
        .sessions
        .lock()
        .map_err(|_| "terminal state is unavailable".to_string())?;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("unknown terminal session: {session_id}"))?;
    session
        .master
        .resize(normalized_pty_size(Some(cols), Some(rows)))
        .map_err(|error| format!("failed to resize terminal: {error}"))
}

#[tauri::command]
pub fn terminal_kill(state: State<'_, TerminalState>, session_id: String) -> Result<(), String> {
    let session = state
        .sessions
        .lock()
        .map_err(|_| "terminal state is unavailable".to_string())?
        .remove(&session_id);
    if let Some(mut session) = session {
        session
            .killer
            .kill()
            .map_err(|error| format!("failed to stop terminal: {error}"))?;
    }
    Ok(())
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
    sessions: Arc<Mutex<HashMap<String, TerminalSession>>>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
) {
    std::thread::spawn(move || {
        let exit_code = child.wait().ok().map(|status| status.exit_code());
        if let Ok(mut sessions) = sessions.lock() {
            sessions.remove(&session_id);
        }
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

fn add_shell_args(command: &mut CommandBuilder, shell: &Path) {
    #[cfg(not(windows))]
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
    }

    #[cfg(windows)]
    let _ = (command, shell);
}

#[cfg(test)]
mod tests {
    use super::{
        build_agent_command, normalized_pty_size, resolve_terminal_cwd, validate_initial_command,
        validate_session_id, AgentKind, TerminalMode,
    };
    use crate::local_engine::LocalEngine;
    use std::path::Path;

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
        assert!(validate_initial_command(AgentKind::Gemini, Some("gemini")).is_ok());
        assert!(validate_initial_command(AgentKind::OpenCode, Some("opencode")).is_ok());
        assert!(validate_initial_command(AgentKind::Hermes, Some("hermes")).is_ok());
        assert!(validate_initial_command(AgentKind::Codex, Some("rm -rf ~")).is_err());
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
            "research-space",
            executable,
        );
        let claude = build_agent_command(
            AgentKind::Claude,
            TerminalMode::Background,
            "research-space",
            executable,
        );

        assert!(codex.contains("mcp_servers.cowiki.command"));
        assert!(codex.contains("--space"));
        assert!(codex.contains("research-space"));
        assert!(codex.contains("Before claiming that knowledge is absent"));
        assert!(codex.contains("current Draft working tree"));
        assert!(claude.contains("--mcp-config"));
        assert!(claude.contains("cowiki"));
        assert!(claude.contains("--append-system-prompt"));
        assert!(claude.contains("managed background worktree"));
        assert!(claude.contains("Do not checkout, merge, commit, or push"));

        let grok = build_agent_command(
            AgentKind::Grok,
            TerminalMode::Live,
            "research-space",
            executable,
        );
        let gemini = build_agent_command(
            AgentKind::Gemini,
            TerminalMode::Live,
            "research-space",
            executable,
        );
        let opencode = build_agent_command(
            AgentKind::OpenCode,
            TerminalMode::Live,
            "research-space",
            executable,
        );
        let hermes = build_agent_command(
            AgentKind::Hermes,
            TerminalMode::Live,
            "research-space",
            executable,
        );

        assert!(grok.starts_with("grok --rules "));
        assert!(grok.contains("Before claiming that knowledge is absent"));
        assert!(gemini.starts_with("gemini --prompt-interactive "));
        assert!(gemini.contains("Before claiming that knowledge is absent"));
        assert!(opencode.starts_with("opencode --prompt "));
        assert!(opencode.contains("Before claiming that knowledge is absent"));
        assert_eq!(hermes, "hermes chat");
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
}
