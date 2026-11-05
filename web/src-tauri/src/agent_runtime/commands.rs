//! Tauri commands for native Agent sessions. The renderer passes ids and
//! text only; directories, processes and approvals stay in Rust.

use super::auth::{AgentAuth, LoginEvent, LoginStarted, NativeReadiness};
use super::contract::{AgentEvent, AgentSnapshot, AGENT_EVENT, AGENT_LOGIN_EVENT};
use super::registry::{AgentRuntime, CreateSession, EventEmitter};
use crate::terminal::AgentKind;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

pub struct AgentState {
    pub runtime: AgentRuntime,
    pub auth: AgentAuth,
}

pub struct TauriEmitter(pub AppHandle);

impl EventEmitter for TauriEmitter {
    fn emit(&self, event: AgentEvent) {
        let _ = self.0.emit(AGENT_EVENT, event);
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("Agent task failed: {error}"))?
}

#[tauri::command]
pub async fn agent_probe_native(
    state: State<'_, AgentState>,
    agent: AgentKind,
    force: Option<bool>,
) -> Result<NativeReadiness, String> {
    let auth = state.auth.clone();
    blocking(move || Ok(auth.probe(agent, force.unwrap_or(false)))).await
}

#[tauri::command]
pub async fn agent_login_start(
    app: AppHandle,
    state: State<'_, AgentState>,
    agent: AgentKind,
) -> Result<LoginStarted, String> {
    let auth = state.auth.clone();
    let notify = Arc::new(move |event: LoginEvent| {
        let _ = app.emit(AGENT_LOGIN_EVENT, event);
    });
    blocking(move || auth.start_login(agent, notify)).await
}

#[tauri::command]
pub async fn agent_login_cancel(
    state: State<'_, AgentState>,
    agent: AgentKind,
) -> Result<(), String> {
    let auth = state.auth.clone();
    blocking(move || auth.cancel_login(agent)).await
}

#[tauri::command]
pub fn agent_list_sessions(
    state: State<'_, AgentState>,
    space_slug: String,
) -> Result<Vec<AgentSnapshot>, String> {
    state.runtime.list_sessions(&space_slug)
}

#[tauri::command]
pub async fn agent_create_session(
    state: State<'_, AgentState>,
    request: CreateSession,
) -> Result<AgentSnapshot, String> {
    let runtime = state.runtime.clone();
    blocking(move || runtime.create_session(request)).await
}

#[tauri::command]
pub fn agent_snapshot(
    state: State<'_, AgentState>,
    session_id: String,
) -> Result<AgentSnapshot, String> {
    state.runtime.snapshot(&session_id)
}

#[tauri::command]
pub fn agent_send(
    state: State<'_, AgentState>,
    session_id: String,
    text: String,
) -> Result<AgentSnapshot, String> {
    state.runtime.send(&session_id, &text)
}

#[tauri::command]
pub fn agent_stop(
    state: State<'_, AgentState>,
    session_id: String,
) -> Result<AgentSnapshot, String> {
    state.runtime.stop(&session_id)
}

#[tauri::command]
pub fn agent_resume(
    state: State<'_, AgentState>,
    session_id: String,
) -> Result<AgentSnapshot, String> {
    state.runtime.resume(&session_id)
}

#[tauri::command]
pub fn agent_answer_approval(
    state: State<'_, AgentState>,
    session_id: String,
    approval_id: String,
    decision: String,
) -> Result<AgentSnapshot, String> {
    let allow = match decision.as_str() {
        "allow" => true,
        "deny" => false,
        _ => return Err("Unknown approval decision".into()),
    };
    state
        .runtime
        .answer_approval(&session_id, &approval_id, allow)
}

#[tauri::command]
pub async fn agent_remove_session(
    state: State<'_, AgentState>,
    session_id: String,
) -> Result<(), String> {
    let runtime = state.runtime.clone();
    blocking(move || runtime.remove_session(&session_id)).await
}
