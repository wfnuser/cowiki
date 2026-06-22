//! SessionManager — per-session lifecycle and opt-in file tracking.
//!
//! Each `AgentManager::execute()` call creates one volatile Session.
//! Sessions are NOT persisted — one-shot mode only.
//!
//! ## Tracking
//!
//! Tracking is opt-in per session.  Call `start_tracking(session_id, workspace)`
//! to begin recording writes/removes.  Server routes call `track_write` /
//! `track_remove` — they only record when a tracking session is active for that
//! workspace.  `stop_tracking()` returns the accumulated tracking and disables
//! further recording.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use uuid::Uuid;

// ── SessionId ──────────────────────────────────────────────────

pub type SessionId = String;

// ── Session ────────────────────────────────────────────────────

pub struct Session {
    pub id: SessionId,
    pub agent_name: String,
    pub status: SessionStatus,
    pub tracking: ExecutionTracking,
    /// Whether tracking is currently active for this session.
    pub tracking_active: bool,
    /// Workspace being tracked (set when start_tracking is called).
    pub tracked_workspace: Option<String>,
    pub extra_args: HashMap<String, String>,
    pub created_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionTracking {
    pub written_files: Vec<String>,
    pub removed_files: Vec<String>,
}

// ── SessionManager ─────────────────────────────────────────────

/// Manages all active sessions with per-session isolation.
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>,
    /// Workspace → SessionId index for fast lookup during tracking.
    ws_to_session: Arc<RwLock<HashMap<String, SessionId>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        tracing::info!("SessionManager created");
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            ws_to_session: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ── Lifecycle ──────────────────────────────────────────────

    pub async fn create(
        &self,
        agent_name: String,
        extra_args: HashMap<String, String>,
    ) -> SessionId {
        let id = Uuid::new_v4().to_string();
        let session = Session {
            id: id.clone(),
            agent_name,
            status: SessionStatus::Running,
            tracking: ExecutionTracking::default(),
            tracking_active: false,
            tracked_workspace: None,
            extra_args,
            created_at: Instant::now(),
        };
        self.sessions
            .write()
            .await
            .insert(id.clone(), Arc::new(RwLock::new(session)));
        tracing::info!(session_id = %id, "session created");
        id
    }

    pub async fn status(&self, id: &SessionId) -> Option<SessionStatus> {
        self.sessions
            .read()
            .await
            .get(id)
            .and_then(|s| s.try_read().ok())
            .map(|s| s.status)
    }

    pub async fn complete(&self, id: &SessionId) {
        if let Some(session) = self.sessions.read().await.get(id) {
            let mut s = session.write().await;
            s.status = SessionStatus::Succeeded;
        }
    }

    pub async fn fail(&self, id: &SessionId, _error: &str) {
        if let Some(session) = self.sessions.read().await.get(id) {
            let mut s = session.write().await;
            s.status = SessionStatus::Failed;
        }
    }

    // ── Opt-in tracking ────────────────────────────────────────

    /// Enable tracking for a session+workspace. While active, `track_write` /
    /// `track_remove` calls for this workspace will be recorded.
    pub async fn start_tracking(&self, session_id: &SessionId, workspace: &str) {
        let mut ws_map = self.ws_to_session.write().await;
        ws_map.insert(workspace.to_string(), session_id.clone());

        if let Some(session) = self.sessions.read().await.get(session_id) {
            let mut s = session.write().await;
            s.tracking_active = true;
            s.tracked_workspace = Some(workspace.to_string());
        }
        tracing::info!(session_id = %session_id, %workspace, "tracking started");
    }

    /// Stop tracking for a session and return accumulated results.
    pub async fn stop_tracking(&self, session_id: &SessionId) -> Option<ExecutionTracking> {
        // Remove workspace→session mapping
        if let Some(session) = self.sessions.read().await.get(session_id) {
            let s = session.read().await;
            if let Some(ref ws) = s.tracked_workspace {
                self.ws_to_session.write().await.remove(ws);
            }
        }

        // Take tracking data
        if let Some(session) = self.sessions.read().await.get(session_id) {
            let mut s = session.write().await;
            s.tracking_active = false;
            s.tracked_workspace = None;
            let tracking = std::mem::take(&mut s.tracking);
            tracing::info!(session_id = %session_id,
                written = tracking.written_files.len(),
                removed = tracking.removed_files.len(),
                "tracking stopped");
            return Some(tracking);
        }
        None
    }

    /// Record a file write. Only records if there's an active tracking session
    /// for this workspace. Returns true if recorded.
    pub async fn track_write(&self, workspace: &str, path: &str) -> bool {
        let session_id = {
            let ws_map = self.ws_to_session.read().await;
            ws_map.get(workspace).cloned()
        };

        if let Some(ref sid) = session_id {
            if let Some(session) = self.sessions.read().await.get(sid) {
                let mut s = session.write().await;
                if s.tracking_active {
                    s.tracking.written_files.push(path.to_string());
                    return true;
                }
            }
        }
        false
    }

    /// Record a file removal. Only records if there's an active tracking session
    /// for this workspace. Returns true if recorded.
    pub async fn track_remove(&self, workspace: &str, path: &str) -> bool {
        let session_id = {
            let ws_map = self.ws_to_session.read().await;
            ws_map.get(workspace).cloned()
        };

        if let Some(ref sid) = session_id {
            if let Some(session) = self.sessions.read().await.get(sid) {
                let mut s = session.write().await;
                if s.tracking_active {
                    s.tracking.removed_files.push(path.to_string());
                    return true;
                }
            }
        }
        false
    }

    // ── Scoped token ──────────────────────────────────────────

    /// Mint a scoped Bearer token for agent CLI auth.
    /// Format: "cowiki_ses_{session_id}" — simple prefix for now.
    pub fn mint_scoped_token(&self, id: &SessionId) -> String {
        format!("cowiki_ses_{}", id)
    }

    /// Verify a scoped token and return the session_id if valid.
    pub fn verify_token(&self, token: &str) -> Option<SessionId> {
        let id = token.strip_prefix("cowiki_ses_")?;
        // Check that session exists and is still running
        let sessions = self.sessions.blocking_read();
        let session = sessions.get(id)?;
        let s = session.blocking_read();
        if s.status != SessionStatus::Running {
            return None;
        }
        Some(id.to_string())
    }

    // ── Cleanup ────────────────────────────────────────────────

    pub async fn cleanup_stale(&self, ttl: Duration) {
        let mut to_remove = Vec::new();
        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                if let Ok(s) = session.try_read() {
                    if s.created_at.elapsed() > ttl
                        && matches!(s.status, SessionStatus::Succeeded | SessionStatus::Failed)
                    {
                        to_remove.push(id.clone());
                    }
                }
            }
        }
        if !to_remove.is_empty() {
            let mut sessions = self.sessions.write().await;
            for id in &to_remove {
                sessions.remove(id);
            }
            tracing::info!(count = to_remove.len(), "cleaned up stale sessions");
        }
    }
}
