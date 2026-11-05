//! Native Agent sessions: ownership of provider processes, Space/Change
//! binding, the run lifecycle, approvals, persistence and Change-close
//! protection. The renderer only names a Space, a mode and a Change; every
//! directory is resolved here from LocalEngine.

use super::contract::{
    keep_head, keep_tail, now_millis, AgentEvent, AgentItem, AgentMode, AgentSnapshot, AgentStatus,
    MessageRole, ToolKind, ToolStatus, MAX_ITEMS, MAX_MESSAGE_BYTES, MAX_TOOL_OUTPUT_BYTES,
};
use super::provider::{
    EventSink, McpLaunch, OpenError, Provider, ProviderConfig, ProviderEvent, ProviderLauncher,
    TurnOutcome,
};
use super::rpc::LaunchSpec;
use super::store::{SessionRecord, SessionStore, SCHEMA_VERSION};
use crate::agent_execution_gate::{AgentExecutionGate, ExecutionScope, StartLease};
use crate::local_engine::LocalEngine;
use crate::terminal::{agent_display_name, AgentKind, TerminalMode, SPACE_PROTOCOL};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

pub const MAX_PROMPT_BYTES: usize = 64 * 1024;
const EMIT_COALESCE: Duration = Duration::from_millis(40);
#[cfg(not(test))]
const STOP_WATCHDOG: Duration = Duration::from_secs(20);
#[cfg(test)]
const STOP_WATCHDOG: Duration = Duration::from_secs(3);

pub trait EventEmitter: Send + Sync {
    fn emit(&self, event: AgentEvent);
}

/// Which agents CoWiki can drive through a structured protocol.
pub fn native_protocol(agent: AgentKind) -> Option<&'static str> {
    match agent {
        AgentKind::Codex => Some("codex-app-server"),
        AgentKind::Claude => Some("claude-stream-json"),
        AgentKind::Grok | AgentKind::OpenCode | AgentKind::Hermes => Some("acp"),
        AgentKind::Antigravity => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSession {
    pub space_slug: String,
    pub mode: AgentMode,
    pub change_id: Option<String>,
    pub agent: AgentKind,
    pub title: Option<String>,
}

struct SessionState {
    snapshot: AgentSnapshot,
    space_id: String,
    space_path: PathBuf,
    cwd: PathBuf,
    provider: Option<Arc<dyn Provider>>,
    lease: Option<StartLease>,
    /// Events from any other provider process carry another generation.
    generation: u64,
    /// Source of unique generations (a provider still in its handshake has
    /// one that is not yet `generation`, so its events are ignored).
    issued: u64,
    stop_requested: bool,
    closing: bool,
    current_turn: Option<String>,
}

impl SessionState {
    /// Ignore every event of the current provider from now on.
    fn retire_provider(&mut self) {
        self.issued = self.issued.max(self.generation) + 1;
        self.generation = self.issued;
    }

    fn bump(&mut self) {
        self.snapshot.seq += 1;
        self.snapshot.updated_at = now_millis();
    }

    fn scope(&self) -> ExecutionScope {
        match self.snapshot.change_id.as_deref() {
            Some(change_id) if self.snapshot.mode == AgentMode::Background => {
                ExecutionScope::background(&self.snapshot.space_slug, change_id)
            }
            _ => ExecutionScope::live(&self.snapshot.space_slug),
        }
    }

    fn record(&self) -> SessionRecord {
        SessionRecord {
            schema_version: SCHEMA_VERSION,
            snapshot: self.snapshot.clone(),
            space_id: self.space_id.clone(),
            space_path: self.space_path.clone(),
            cwd: self.cwd.clone(),
        }
    }

    fn blocks_close(&self, space_slug: &str, change_id: &str) -> bool {
        self.snapshot.space_slug == space_slug
            && match self.snapshot.mode {
                AgentMode::Live => true,
                AgentMode::Background => self.snapshot.change_id.as_deref() == Some(change_id),
            }
    }
}

struct Session {
    state: Mutex<SessionState>,
    flush_scheduled: AtomicBool,
    last_saved: Mutex<u64>,
}

struct Inner {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    gate: AgentExecutionGate,
    engine: LocalEngine,
    store: SessionStore,
    emitter: Arc<dyn EventEmitter>,
    launcher: Arc<dyn ProviderLauncher>,
    mcp: Option<McpLaunch>,
}

#[derive(Clone)]
pub struct AgentRuntime {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    mutex
        .lock()
        .map_err(|_| "Agent session state is unavailable".to_string())
}

fn validate_prompt(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Write a message for the Agent first".into());
    }
    if text.len() > MAX_PROMPT_BYTES {
        return Err(format!(
            "The message is too long (maximum {} KB)",
            MAX_PROMPT_BYTES / 1024
        ));
    }
    if text.contains('\0') {
        return Err("The message contains an invalid null byte".into());
    }
    Ok(text.to_string())
}

fn item_key(turn_id: Option<&str>, item_id: &str) -> String {
    match turn_id {
        Some(turn) if !turn.is_empty() => format!("{turn}/{item_id}"),
        _ => item_id.to_string(),
    }
}

fn trim_items(items: &mut Vec<AgentItem>) {
    if items.len() > MAX_ITEMS {
        let excess = items.len() - MAX_ITEMS;
        items.drain(..excess);
    }
}

fn upsert_message(
    items: &mut Vec<AgentItem>,
    key: String,
    turn_id: Option<String>,
    text: &str,
    append: bool,
) {
    let existing = items.iter_mut().rev().find(|item| item.id() == key);
    match existing {
        Some(AgentItem::Message { text: current, .. }) => {
            if append {
                current.push_str(text);
            } else {
                *current = text.to_string();
            }
            if current.len() > MAX_MESSAGE_BYTES {
                *current = keep_head(current, MAX_MESSAGE_BYTES).0;
            }
        }
        _ => {
            items.push(AgentItem::Message {
                id: key,
                role: MessageRole::Assistant,
                text: keep_head(text, MAX_MESSAGE_BYTES).0,
                turn_id,
            });
            trim_items(items);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn upsert_tool(
    items: &mut Vec<AgentItem>,
    key: String,
    turn_id: Option<String>,
    name: Option<String>,
    kind: ToolKind,
    status: Option<ToolStatus>,
    output: Option<String>,
    append: Option<String>,
) {
    let existing = items.iter_mut().rev().find(|item| item.id() == key);
    match existing {
        Some(AgentItem::Tool {
            name: current_name,
            status: current_status,
            output: current_output,
            truncated,
            tool_kind,
            ..
        }) => {
            if let Some(name) = name.filter(|name| !name.is_empty()) {
                *current_name = name;
            }
            if let Some(status) = status {
                // A finished step never goes back to running.
                if *current_status == ToolStatus::Running || status != ToolStatus::Running {
                    *current_status = status;
                }
            }
            if *tool_kind == ToolKind::Other {
                *tool_kind = kind;
            }
            if let Some(output) = output {
                *current_output = output;
                *truncated = false;
            }
            if let Some(append) = append {
                current_output.push_str(&append);
            }
            if keep_tail(current_output, MAX_TOOL_OUTPUT_BYTES) {
                *truncated = true;
            }
        }
        _ => {
            let mut text = output.unwrap_or_default();
            if let Some(append) = append {
                text.push_str(&append);
            }
            let truncated = keep_tail(&mut text, MAX_TOOL_OUTPUT_BYTES);
            items.push(AgentItem::Tool {
                id: key,
                turn_id,
                name: name.unwrap_or_else(|| "Tool".into()),
                tool_kind: kind,
                status: status.unwrap_or(ToolStatus::Running),
                output: text,
                truncated,
            });
            trim_items(items);
        }
    }
}

/// After a turn: thinking steps are done; when the turn was stopped or the
/// Agent exited (`ended`), steps still marked running are marked stopped.
fn settle_running_tools(items: &mut [AgentItem], ended: Option<ToolStatus>) {
    for item in items.iter_mut() {
        if let AgentItem::Tool {
            status, tool_kind, ..
        } = item
        {
            if *status != ToolStatus::Running {
                continue;
            }
            if *tool_kind == ToolKind::Think {
                *status = ToolStatus::Completed;
            } else if let Some(ended) = ended {
                *status = ended;
            }
        }
    }
}

/// Clears a Change-close reservation of sessions on drop.
pub struct ClosePreparation {
    runtime: AgentRuntime,
    sessions: Vec<Arc<Session>>,
    change_id: String,
}

impl ClosePreparation {
    /// The Change was merged or discarded: its background sessions can be
    /// read but never run again.
    pub fn commit(self, outcome: &str) {
        for session in &self.sessions {
            let event = {
                let Ok(mut state) = session.state.lock() else {
                    continue;
                };
                if state.snapshot.mode == AgentMode::Background
                    && state.snapshot.change_id.as_deref() == Some(self.change_id.as_str())
                {
                    state.snapshot.closed = true;
                    state.snapshot.notice = Some(format!(
                        "This Agent Change was {outcome}. Start a new task to keep working."
                    ));
                    state.bump();
                }
                state.closing = false;
                state.snapshot.clone()
            };
            self.runtime.publish(session, event);
        }
    }
}

impl Drop for ClosePreparation {
    fn drop(&mut self) {
        for session in &self.sessions {
            if let Ok(mut state) = session.state.lock() {
                state.closing = false;
            }
        }
    }
}

impl AgentRuntime {
    pub fn new(
        engine: LocalEngine,
        gate: AgentExecutionGate,
        store: SessionStore,
        emitter: Arc<dyn EventEmitter>,
        launcher: Arc<dyn ProviderLauncher>,
        mcp: Option<McpLaunch>,
    ) -> Self {
        let mut sessions = HashMap::new();
        for record in store.load_all() {
            let id = record.snapshot.session_id.clone();
            let changed = record.snapshot.status == AgentStatus::Interrupted;
            let session = Arc::new(Session {
                state: Mutex::new(SessionState {
                    snapshot: record.snapshot,
                    space_id: record.space_id,
                    space_path: record.space_path,
                    cwd: record.cwd,
                    provider: None,
                    lease: None,
                    generation: 0,
                    issued: 0,
                    stop_requested: false,
                    closing: false,
                    current_turn: None,
                }),
                flush_scheduled: AtomicBool::new(false),
                last_saved: Mutex::new(0),
            });
            if changed {
                if let Ok(state) = session.state.lock() {
                    let _ = store.save(&state.record());
                }
            }
            sessions.insert(id, session);
        }
        Self {
            inner: Arc::new(Inner {
                sessions: Mutex::new(sessions),
                gate,
                engine,
                store,
                emitter,
                launcher,
                mcp,
            }),
        }
    }

    fn session(&self, session_id: &str) -> Result<Arc<Session>, String> {
        lock(&self.inner.sessions)?
            .get(session_id)
            .cloned()
            .ok_or_else(|| "This Agent session no longer exists".to_string())
    }

    fn publish(&self, session: &Arc<Session>, snapshot: AgentSnapshot) {
        self.save_snapshot(session, &snapshot);
        self.inner.emitter.emit(AgentEvent {
            session_id: snapshot.session_id.clone(),
            seq: snapshot.seq,
            snapshot,
        });
    }

    fn save_snapshot(&self, session: &Arc<Session>, snapshot: &AgentSnapshot) {
        let record = {
            let Ok(state) = session.state.lock() else {
                return;
            };
            SessionRecord {
                schema_version: SCHEMA_VERSION,
                snapshot: snapshot.clone(),
                space_id: state.space_id.clone(),
                space_path: state.space_path.clone(),
                cwd: state.cwd.clone(),
            }
        };
        let Ok(mut last) = session.last_saved.lock() else {
            return;
        };
        if record.snapshot.seq <= *last && *last != 0 {
            return;
        }
        if self.inner.store.save(&record).is_ok() {
            *last = record.snapshot.seq;
        }
    }

    /// Streaming deltas are coalesced: at most one snapshot per ~40 ms.
    fn schedule_flush(&self, session: &Arc<Session>) {
        if session.flush_scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let runtime = self.clone();
        let session = session.clone();
        std::thread::spawn(move || {
            std::thread::sleep(EMIT_COALESCE);
            session.flush_scheduled.store(false, Ordering::Release);
            let snapshot = match session.state.lock() {
                Ok(state) => state.snapshot.clone(),
                Err(_) => return,
            };
            runtime.inner.emitter.emit(AgentEvent {
                session_id: snapshot.session_id.clone(),
                seq: snapshot.seq,
                snapshot,
            });
        });
    }

    fn resolve_directories(
        &self,
        space_slug: &str,
        mode: AgentMode,
        change_id: Option<&str>,
    ) -> Result<(String, PathBuf, PathBuf), String> {
        let space = self.inner.engine.find_space(space_slug)?;
        let space_path = space
            .local_path
            .canonicalize()
            .map_err(|error| format!("The Space directory is unavailable: {error}"))?;
        let cwd = match mode {
            AgentMode::Live => {
                if change_id.is_some() {
                    return Err("A Current Draft session cannot claim an Agent Change".into());
                }
                space_path.clone()
            }
            AgentMode::Background => {
                let change_id = change_id
                    .ok_or_else(|| "A background session needs an Agent Change".to_string())?;
                self.inner
                    .engine
                    .agent_change_worktree(space_slug, change_id)?
                    .canonicalize()
                    .map_err(|error| {
                        format!("The Agent Change directory is unavailable: {error}")
                    })?
            }
        };
        Ok((space.id, space_path, cwd))
    }

    pub fn create_session(&self, request: CreateSession) -> Result<AgentSnapshot, String> {
        if native_protocol(request.agent).is_none() {
            return Err(format!(
                "{} cannot run in the CoWiki sidebar yet. Choose another Agent.",
                agent_display_name(request.agent)
            ));
        }
        let (space_id, space_path, cwd) = self.resolve_directories(
            &request.space_slug,
            request.mode,
            request.change_id.as_deref(),
        )?;
        let session_id = format!("agent-{}", uuid::Uuid::new_v4().simple());
        let now = now_millis();
        let title = request
            .title
            .map(|title| keep_head(title.trim(), 200).0)
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| agent_display_name(request.agent).to_string());
        let snapshot = AgentSnapshot {
            session_id: session_id.clone(),
            provider: request.agent,
            title,
            space_slug: request.space_slug,
            mode: request.mode,
            change_id: request.change_id,
            thread_id: None,
            model: None,
            seq: 1,
            status: AgentStatus::Idle,
            items: Vec::new(),
            approvals: Vec::new(),
            error: None,
            notice: None,
            closed: false,
            created_at: now,
            updated_at: now,
        };
        let session = Arc::new(Session {
            state: Mutex::new(SessionState {
                snapshot: snapshot.clone(),
                space_id,
                space_path,
                cwd,
                provider: None,
                lease: None,
                generation: 0,
                issued: 0,
                stop_requested: false,
                closing: false,
                current_turn: None,
            }),
            flush_scheduled: AtomicBool::new(false),
            last_saved: Mutex::new(0),
        });
        lock(&self.inner.sessions)?.insert(session_id, session.clone());
        self.publish(&session, snapshot.clone());
        Ok(snapshot)
    }

    pub fn list_sessions(&self, space_slug: &str) -> Result<Vec<AgentSnapshot>, String> {
        let sessions = lock(&self.inner.sessions)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut snapshots = sessions
            .iter()
            .filter_map(|session| {
                session
                    .state
                    .lock()
                    .ok()
                    .map(|state| state.snapshot.clone())
            })
            .filter(|snapshot| snapshot.space_slug == space_slug)
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.updated_at));
        Ok(snapshots)
    }

    pub fn snapshot(&self, session_id: &str) -> Result<AgentSnapshot, String> {
        Ok(lock(&self.session(session_id)?.state)?.snapshot.clone())
    }

    pub fn send(&self, session_id: &str, text: &str) -> Result<AgentSnapshot, String> {
        let text = validate_prompt(text)?;
        let session = self.session(session_id)?;
        let snapshot = {
            let mut state = lock(&session.state)?;
            if state.snapshot.closed {
                return Err(
                    "This Agent Change was merged or discarded. Start a new task to keep working."
                        .into(),
                );
            }
            if state.closing {
                return Err("This Agent Change is being merged or discarded".into());
            }
            if state.snapshot.status.is_busy() {
                return Err(
                    "The Agent is still working. Stop it or wait before sending another message."
                        .into(),
                );
            }
            let alive = state
                .provider
                .as_ref()
                .is_some_and(|provider| provider.is_alive());
            state.snapshot.items.push(AgentItem::Message {
                id: format!("user-{}", uuid::Uuid::new_v4().simple()),
                role: MessageRole::User,
                text: text.clone(),
                turn_id: None,
            });
            trim_items(&mut state.snapshot.items);
            state.snapshot.status = if alive {
                AgentStatus::Running
            } else {
                AgentStatus::Starting
            };
            state.snapshot.error = None;
            state.snapshot.notice = None;
            state.stop_requested = false;
            state.bump();
            state.snapshot.clone()
        };
        self.publish(&session, snapshot.clone());
        let runtime = self.clone();
        std::thread::spawn(move || runtime.run_turn(session, text));
        Ok(snapshot)
    }

    fn fail(&self, session: &Arc<Session>, generation: Option<u64>, message: String) {
        let snapshot = {
            let Ok(mut state) = session.state.lock() else {
                return;
            };
            if generation.is_some_and(|generation| generation != state.generation) {
                return;
            }
            state.snapshot.status = AgentStatus::Failed;
            state.snapshot.error = Some(message);
            state.snapshot.approvals.clear();
            state.stop_requested = false;
            state.current_turn = None;
            state.bump();
            state.snapshot.clone()
        };
        self.publish(session, snapshot);
    }

    fn run_turn(&self, session: Arc<Session>, text: String) {
        let (provider, generation) = match self.ensure_provider(&session) {
            Ok(opened) => opened,
            Err(error) => {
                let Ok(agent) = session.state.lock().map(|state| {
                    state
                        .snapshot
                        .status
                        .is_busy()
                        .then_some(state.snapshot.provider)
                }) else {
                    return;
                };
                // Stop already settled the session (for example the watchdog).
                let Some(agent) = agent else { return };
                let message = match &error {
                    OpenError::SignedOut(_) => format!(
                        "{} is not signed in. Sign in from the Agent menu, then send the message again.",
                        agent_display_name(agent)
                    ),
                    other => other.message().to_string(),
                };
                self.fail(&session, None, message);
                return;
            }
        };
        let snapshot = {
            let Ok(mut state) = session.state.lock() else {
                return;
            };
            if state.generation != generation {
                return;
            }
            if state.stop_requested {
                state.stop_requested = false;
                state.snapshot.status = AgentStatus::Stopped;
                state.bump();
                Some(state.snapshot.clone())
            } else {
                if state.snapshot.status == AgentStatus::Starting {
                    state.snapshot.status = AgentStatus::Running;
                    state.bump();
                }
                None
            }
        };
        if let Some(snapshot) = snapshot {
            self.publish(&session, snapshot);
            return;
        }
        let turn_id = format!("turn-{}", uuid::Uuid::new_v4().simple());
        if let Err(error) = provider.start_turn(&turn_id, &text) {
            // Also replace a generic "exited" error with the specific reason.
            let applies = session.state.lock().is_ok_and(|state| {
                state.generation == generation
                    && (state.snapshot.status.is_busy()
                        || state.snapshot.status == AgentStatus::Failed)
            });
            if applies {
                self.fail(&session, Some(generation), error);
            }
        }
    }

    /// Returns the live provider, starting (and resuming) one when needed.
    fn ensure_provider(
        &self,
        session: &Arc<Session>,
    ) -> Result<(Arc<dyn Provider>, u64), OpenError> {
        let (lease, (previous, generation), resume, mode, agent, slug, change_id, space_id, cwd) = {
            let mut state = lock(&session.state).map_err(OpenError::Failed)?;
            if let Some(provider) = state
                .provider
                .clone()
                .filter(|provider| provider.is_alive())
            {
                return Ok((provider, state.generation));
            }
            if state.snapshot.closed || state.closing {
                return Err(OpenError::Failed("This Agent Change is closed".into()));
            }
            state.provider = None;
            let lease = match state.lease.take() {
                Some(lease) => lease,
                None => self
                    .inner
                    .gate
                    .reserve_start(state.scope())
                    .map_err(OpenError::Failed)?,
            };
            state.issued = state.issued.max(state.generation) + 1;
            (
                lease,
                (state.generation, state.issued),
                state.snapshot.thread_id.clone(),
                state.snapshot.mode,
                state.snapshot.provider,
                state.snapshot.space_slug.clone(),
                state.snapshot.change_id.clone(),
                state.space_id.clone(),
                state.cwd.clone(),
            )
        };
        // Re-check the binding: the Space may have moved or the Change closed.
        let (current_space_id, _, current_cwd) = self
            .resolve_directories(&slug, mode, change_id.as_deref())
            .map_err(OpenError::Failed)?;
        if current_space_id != space_id || current_cwd != cwd {
            return Err(OpenError::Failed(
                "The Space or Agent Change directory changed since this session started. Start a new task there."
                    .into(),
            ));
        }
        let terminal_mode = match mode {
            AgentMode::Live => TerminalMode::Live,
            AgentMode::Background => TerminalMode::Background,
        };
        let config = ProviderConfig {
            agent,
            launch: LaunchSpec {
                program: PathBuf::new(),
                args: Vec::new(),
                cwd: cwd.clone(),
                env: Vec::new(),
            },
            instructions: format!("{SPACE_PROTOCOL} {}", terminal_mode.prompt()),
            mcp: self.inner.mcp.clone().map(|mut mcp| {
                mcp.args.extend(["--space".to_string(), slug.clone()]);
                mcp
            }),
        };
        let sink = self.sink(session, generation);
        let opened = self.inner.launcher.open(config, resume, sink)?;
        let snapshot = {
            let mut state = lock(&session.state).map_err(OpenError::Failed)?;
            if state.generation != previous || state.closing {
                drop(state);
                let _ = opened.provider.shutdown();
                return Err(OpenError::Failed(
                    "The session was closed while the Agent started".into(),
                ));
            }
            // From now on this provider's events apply to the session.
            state.generation = generation;
            state.provider = Some(opened.provider.clone());
            state.lease = Some(lease);
            state.snapshot.thread_id = Some(opened.thread_id);
            if opened.model.is_some() {
                state.snapshot.model = opened.model;
            }
            state.bump();
            state.snapshot.clone()
        };
        self.publish(session, snapshot);
        Ok((opened.provider, generation))
    }

    fn sink(&self, session: &Arc<Session>, generation: u64) -> EventSink {
        let weak_session: Weak<Session> = Arc::downgrade(session);
        let weak_runtime = Arc::downgrade(&self.inner);
        Arc::new(move |event| {
            let (Some(session), Some(inner)) = (weak_session.upgrade(), weak_runtime.upgrade())
            else {
                return;
            };
            AgentRuntime { inner }.apply(&session, generation, event);
        })
    }

    fn apply(&self, session: &Arc<Session>, generation: u64, event: ProviderEvent) {
        let mut urgent = true;
        let mut released: Option<StartLease> = None;
        let snapshot = {
            let Ok(mut state) = session.state.lock() else {
                return;
            };
            if state.generation != generation {
                return;
            }
            match event {
                ProviderEvent::TurnStarted { turn_id } => {
                    state.current_turn = Some(turn_id);
                    if matches!(
                        state.snapshot.status,
                        AgentStatus::Starting
                            | AgentStatus::Idle
                            | AgentStatus::Stopped
                            | AgentStatus::Failed
                            | AgentStatus::Interrupted
                    ) {
                        state.snapshot.status = AgentStatus::Running;
                    }
                }
                ProviderEvent::TurnFinished { outcome, error, .. } => {
                    state.current_turn = None;
                    state.snapshot.approvals.clear();
                    let stopped = std::mem::take(&mut state.stop_requested);
                    state.snapshot.status = match outcome {
                        TurnOutcome::Completed if stopped => AgentStatus::Stopped,
                        TurnOutcome::Completed => AgentStatus::Idle,
                        TurnOutcome::Interrupted => AgentStatus::Stopped,
                        TurnOutcome::Failed => AgentStatus::Failed,
                    };
                    if let Some(error) = error {
                        state.snapshot.error = Some(error);
                    }
                    let ended = if outcome == TurnOutcome::Completed {
                        None
                    } else {
                        Some(ToolStatus::Stopped)
                    };
                    settle_running_tools(&mut state.snapshot.items, ended);
                }
                ProviderEvent::Message {
                    item_id,
                    turn_id,
                    text,
                    append,
                } => {
                    urgent = !append;
                    let key = item_key(turn_id.as_deref(), &item_id);
                    upsert_message(&mut state.snapshot.items, key, turn_id, &text, append);
                }
                ProviderEvent::Tool {
                    item_id,
                    turn_id,
                    name,
                    kind,
                    status,
                    output,
                    append,
                } => {
                    urgent = name.is_some()
                        || status.is_some_and(|status| status != ToolStatus::Running);
                    let key = item_key(turn_id.as_deref(), &item_id);
                    upsert_tool(
                        &mut state.snapshot.items,
                        key,
                        turn_id,
                        name,
                        kind,
                        status,
                        output,
                        append,
                    );
                }
                ProviderEvent::Approval(approval) => {
                    if !state
                        .snapshot
                        .approvals
                        .iter()
                        .any(|existing| existing.id == approval.id)
                    {
                        state.snapshot.approvals.push(approval);
                    }
                    if matches!(
                        state.snapshot.status,
                        AgentStatus::Running | AgentStatus::Starting
                    ) {
                        state.snapshot.status = AgentStatus::Waiting;
                    }
                }
                ProviderEvent::ApprovalResolved { approval_id } => {
                    state
                        .snapshot
                        .approvals
                        .retain(|approval| approval.id != approval_id);
                    if state.snapshot.approvals.is_empty()
                        && state.snapshot.status == AgentStatus::Waiting
                    {
                        state.snapshot.status = AgentStatus::Running;
                    }
                }
                ProviderEvent::Model(model) => state.snapshot.model = Some(model),
                ProviderEvent::Notice(notice) => state.snapshot.notice = Some(notice),
                ProviderEvent::Error(error) => state.snapshot.error = Some(error),
                ProviderEvent::Exited { reason } => {
                    state.provider = None;
                    // The process group is gone: the Change may close again.
                    released = state.lease.take();
                    state.current_turn = None;
                    state.snapshot.approvals.clear();
                    settle_running_tools(&mut state.snapshot.items, Some(ToolStatus::Stopped));
                    if state.snapshot.status.is_busy() {
                        state.snapshot.status = if state.stop_requested {
                            AgentStatus::Stopped
                        } else {
                            AgentStatus::Failed
                        };
                        if state.snapshot.status == AgentStatus::Failed {
                            state.snapshot.error = Some(reason);
                        }
                    } else {
                        state.snapshot.notice = Some(format!(
                            "{} exited. The next message reconnects to the same conversation.",
                            agent_display_name(state.snapshot.provider)
                        ));
                    }
                    state.stop_requested = false;
                }
            }
            state.bump();
            urgent.then(|| state.snapshot.clone())
        };
        drop(released);
        match snapshot {
            Some(snapshot) => self.publish(session, snapshot),
            None => self.schedule_flush(session),
        }
    }

    pub fn stop(&self, session_id: &str) -> Result<AgentSnapshot, String> {
        let session = self.session(session_id)?;
        let (snapshot, provider, generation) = {
            let mut state = lock(&session.state)?;
            match state.snapshot.status {
                AgentStatus::Starting | AgentStatus::Running | AgentStatus::Waiting => {
                    state.stop_requested = true;
                    state.snapshot.status = AgentStatus::Stopping;
                    state.bump();
                }
                _ => return Ok(state.snapshot.clone()),
            }
            (
                state.snapshot.clone(),
                state.provider.clone(),
                state.generation,
            )
        };
        self.publish(&session, snapshot.clone());
        let runtime = self.clone();
        let watched = session.clone();
        std::thread::spawn(move || {
            if let Some(provider) = provider {
                if provider.interrupt().is_err() {
                    runtime.force_stop(
                        &watched,
                        generation,
                        "The Agent did not accept Stop, so CoWiki ended its process.",
                    );
                    return;
                }
            }
            std::thread::sleep(STOP_WATCHDOG);
            let stuck = watched.state.lock().is_ok_and(|state| {
                state.generation == generation && state.snapshot.status == AgentStatus::Stopping
            });
            if stuck {
                runtime.force_stop(
                    &watched,
                    generation,
                    "The Agent did not stop in time, so CoWiki ended its process.",
                );
            }
        });
        Ok(snapshot)
    }

    /// Ends the provider process group and only then reports Stopped.
    fn force_stop(&self, session: &Arc<Session>, generation: u64, notice: &str) {
        let (provider, lease) = {
            let Ok(mut state) = session.state.lock() else {
                return;
            };
            if state.generation != generation {
                return;
            }
            state.retire_provider();
            (state.provider.take(), state.lease.take())
        };
        let result = provider
            .map(|provider| provider.shutdown())
            .unwrap_or(Ok(()));
        let snapshot = {
            let Ok(mut state) = session.state.lock() else {
                return;
            };
            state.snapshot.status = AgentStatus::Stopped;
            state.snapshot.approvals.clear();
            settle_running_tools(&mut state.snapshot.items, Some(ToolStatus::Stopped));
            state.current_turn = None;
            state.stop_requested = false;
            match result {
                Ok(()) => {
                    state.snapshot.notice = Some(notice.to_string());
                    drop(lease);
                }
                Err(error) => {
                    // Keep blocking merge/discard: something may still write.
                    state.lease = lease;
                    state.snapshot.error = Some(format!("{notice} {error}"));
                }
            }
            state.bump();
            state.snapshot.clone()
        };
        self.publish(session, snapshot);
    }

    pub fn answer_approval(
        &self,
        session_id: &str,
        approval_id: &str,
        allow: bool,
    ) -> Result<AgentSnapshot, String> {
        let session = self.session(session_id)?;
        let (snapshot, provider) = {
            let mut state = lock(&session.state)?;
            let position = state
                .snapshot
                .approvals
                .iter()
                .position(|approval| approval.id == approval_id)
                .ok_or_else(|| "This approval is no longer pending".to_string())?;
            let provider = state
                .provider
                .clone()
                .ok_or_else(|| "The Agent is no longer running".to_string())?;
            state.snapshot.approvals.remove(position);
            if state.snapshot.approvals.is_empty() && state.snapshot.status == AgentStatus::Waiting
            {
                state.snapshot.status = AgentStatus::Running;
            }
            state.bump();
            (state.snapshot.clone(), provider)
        };
        self.publish(&session, snapshot.clone());
        if let Err(error) = provider.answer(approval_id, allow) {
            let snapshot = {
                let mut state = lock(&session.state)?;
                state.snapshot.error = Some(error.clone());
                state.bump();
                state.snapshot.clone()
            };
            self.publish(&session, snapshot);
            return Err(error);
        }
        Ok(snapshot)
    }

    /// Explicit "continue" after an interruption: reconnect to the saved
    /// conversation without sending anything.
    pub fn resume(&self, session_id: &str) -> Result<AgentSnapshot, String> {
        let session = self.session(session_id)?;
        let snapshot = {
            let mut state = lock(&session.state)?;
            if state.snapshot.closed {
                return Err(
                    "This Agent Change was merged or discarded. Start a new task to keep working."
                        .into(),
                );
            }
            if state.snapshot.status.is_busy() {
                return Ok(state.snapshot.clone());
            }
            if state
                .provider
                .as_ref()
                .is_some_and(|provider| provider.is_alive())
            {
                return Ok(state.snapshot.clone());
            }
            state.snapshot.status = AgentStatus::Starting;
            state.snapshot.error = None;
            state.snapshot.notice = None;
            state.bump();
            state.snapshot.clone()
        };
        self.publish(&session, snapshot.clone());
        let runtime = self.clone();
        std::thread::spawn(move || match runtime.ensure_provider(&session) {
            Ok((_, generation)) => {
                let snapshot = {
                    let Ok(mut state) = session.state.lock() else {
                        return;
                    };
                    if state.generation != generation {
                        return;
                    }
                    state.snapshot.status = if std::mem::take(&mut state.stop_requested) {
                        AgentStatus::Stopped
                    } else {
                        AgentStatus::Idle
                    };
                    state.bump();
                    state.snapshot.clone()
                };
                runtime.publish(&session, snapshot);
            }
            Err(error) => runtime.fail(&session, None, error.message().to_string()),
        });
        Ok(snapshot)
    }

    pub fn remove_session(&self, session_id: &str) -> Result<(), String> {
        let session = self.session(session_id)?;
        let (provider, lease) = {
            let mut state = lock(&session.state)?;
            if state.snapshot.status.is_busy() {
                return Err("Stop the Agent before removing this session".into());
            }
            state.retire_provider();
            state.closing = true;
            (state.provider.take(), state.lease.take())
        };
        if let Some(provider) = provider {
            if let Err(error) = provider.shutdown() {
                if let Ok(mut state) = session.state.lock() {
                    state.lease = lease;
                    state.closing = false;
                }
                return Err(error);
            }
        }
        drop(lease);
        lock(&self.inner.sessions)?.remove(session_id);
        self.inner.store.remove(session_id)
    }

    /// Before a Change merge/discard: refuse while a related session may still
    /// write; otherwise end idle related provider processes (confirming their
    /// process groups are gone) and keep new starts out until the guard drops.
    pub fn prepare_change_close(
        &self,
        space_slug: &str,
        change_id: &str,
    ) -> Result<ClosePreparation, String> {
        let sessions = lock(&self.inner.sessions)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        // Dropping `preparation` on any error clears the closing marks.
        let mut preparation = ClosePreparation {
            runtime: self.clone(),
            sessions: Vec::new(),
            change_id: change_id.to_string(),
        };
        for session in sessions {
            let mut state = lock(&session.state)?;
            if !state.blocks_close(space_slug, change_id) {
                continue;
            }
            if state.snapshot.status.is_busy() {
                let live = state.snapshot.mode == AgentMode::Live;
                drop(state);
                return Err(if live {
                    "Stop the running Live Agent in this Space before merging or discarding an Agent Change.".to_string()
                } else {
                    "Stop the running Agent before merging or discarding this Agent Change."
                        .to_string()
                });
            }
            // From here on `send` and `resume` refuse this session.
            state.closing = true;
            drop(state);
            preparation.sessions.push(session);
        }
        for session in &preparation.sessions {
            let (provider, lease) = {
                let mut state = lock(&session.state)?;
                if state.provider.is_none() && state.lease.is_none() {
                    continue;
                }
                state.retire_provider();
                (state.provider.take(), state.lease.take())
            };
            if let Some(provider) = provider {
                if let Err(error) = provider.shutdown() {
                    if let Ok(mut state) = session.state.lock() {
                        state.lease = lease;
                    }
                    return Err(format!(
                        "CoWiki could not confirm that the Agent stopped, so the Change was not closed: {error}"
                    ));
                }
            }
            drop(lease);
        }
        Ok(preparation)
    }

    /// App exit: running work becomes Interrupted (never replayed) and every
    /// provider process group CoWiki started is ended.
    pub fn shutdown_all(&self) {
        let sessions = match self.inner.sessions.lock() {
            Ok(sessions) => sessions.values().cloned().collect::<Vec<_>>(),
            Err(_) => return,
        };
        let mut providers = Vec::new();
        for session in &sessions {
            let snapshot = {
                let Ok(mut state) = session.state.lock() else {
                    continue;
                };
                state.retire_provider();
                let provider = state.provider.take();
                let lease = state.lease.take();
                if state.snapshot.status.is_busy() {
                    state.snapshot.status = AgentStatus::Interrupted;
                    state.snapshot.notice = Some(
                        "CoWiki closed while this Agent was working. Nothing was re-run; continue when you are ready."
                            .into(),
                    );
                    state.snapshot.approvals.clear();
                    state.bump();
                }
                if let Some(provider) = provider {
                    providers.push((provider, lease));
                }
                state.snapshot.clone()
            };
            self.save_snapshot(session, &snapshot);
        }
        let handles = providers
            .into_iter()
            .map(|(provider, lease)| {
                std::thread::spawn(move || {
                    let _ = provider.shutdown();
                    drop(lease);
                })
            })
            .collect::<Vec<_>>();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        for handle in handles {
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
