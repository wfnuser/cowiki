//! Agent lifecycle manager — spawn, dispatch, reap.
//!
//! The AgentManager owns agent process lifecycles via stdin/stdout communication:
//! - **OneShot**: spawn pi process per task, stdin → prompt, stdout → parse result.
//!
//! Long-lived agents are managed through the internal registry and reaper.

pub mod process;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::harness::AgentHandle;
use crate::stream::AgentEvent;
use crate::types::bridge::{AgentManifest, AgentManifestEntry, AgentStatus};
use crate::types::error::AgentError;
use crate::types::protocol::{AgentRequest, AgentResponse};

// ── Configuration ─────────────────────────────────────────────

/// Common skill files shared across all task types.
pub const DEFAULT_TOOLS_MD: &str = include_str!("../../skills/_tools.md");
pub const DEFAULT_CONVENTIONS_MD: &str = include_str!("../../skills/_conventions.md");

/// Default SKILL.md for compiler agents.
pub const DEFAULT_COMPILER_SKILL: &str = include_str!("../../skills/compiler/SKILL.md");

/// Deep-compile skill defaults
pub const DEFAULT_DEEP_COMPILE_SKILL: &str = include_str!("../../skills/deep-compile/SKILL.md");

/// Review skill defaults
pub const DEFAULT_REVIEW_SKILL: &str = include_str!("../../skills/review/SKILL.md");

// ── AgentManager ──────────────────────────────────────────────

/// Manages the lifecycle of agent processes.
///
/// - `spawn_and_wait()`: spawns pi, sends prompt via stdin, parses TaskResult from stdout
pub struct AgentManager {
    /// Agent processes keyed by agent name (OneShot)
    pub(crate) processes: Arc<RwLock<HashMap<String, AgentProcess>>>,

    /// Stream-mode agent registry (absorbed from AgentPool)
    registry: Arc<RwLock<HashMap<String, StreamAgent>>>,

    /// Event broadcast: all agent events merged into this channel.
    /// Server subscribes to forward to frontend via SSE.
    event_tx: broadcast::Sender<AgentEvent>,

    /// MCP server URL (e.g., "http://127.0.0.1:9380/mcp/sse")
    mcp_url: String,
    /// Workspace path ($COWIKI_DATA_DIR/{ws}/repo)
    pub workspace_path: PathBuf,
    /// LLM config for models.json generation
    pub llm: cowiki_utils::LlmConfig,
    /// Idle timeout for stream agents
    idle_timeout_secs: u64,
}

/// A registered long-lived (Stream-mode) agent.
#[allow(dead_code)]
struct StreamAgent {
    handle: Arc<dyn AgentHandle>,
    status: StreamAgentStatus,
    last_active: Instant,
    agent_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum StreamAgentStatus {
    Idle,
    Active,
    Stopped,
}

/// In-memory state for a single agent process.
pub struct AgentProcess {
    pub status: AgentStatus,
    pub agent_id: String,
    pub last_active: Instant,
    pub manifest: AgentManifest,
    /// Process handle (spawn, wait, kill)
    pub child: Option<tokio::process::Child>,
}

impl AgentManager {
    /// Create a new agent manager.
    pub fn new(mcp_url: String, workspace_path: PathBuf, llm: cowiki_utils::LlmConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        tracing::info!(
            mcp_url = %mcp_url,
            workspace = %workspace_path.display(),
            "created AgentManager (stdin/stdout mode)"
        );

        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            registry: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            mcp_url,
            workspace_path,
            llm,
            idle_timeout_secs: 900,
        }
    }

    // ── Accessors ─────────────────────────────────────────────

    pub fn mcp_url(&self) -> &str {
        &self.mcp_url
    }

    pub fn workspace_path(&self) -> &PathBuf {
        &self.workspace_path
    }

    // ── Manifest I/O ──────────────────────────────────────────

    /// Load an agent manifest from disk, or lazily create a default.
    pub async fn load_or_create_manifest(&self, name: &str) -> Result<AgentManifest, AgentError> {
        let toml_path = self
            .workspace_path
            .join("agents")
            .join(name)
            .join("agent.toml");
        if toml_path.exists() {
            let content = std::fs::read_to_string(&toml_path)
                .map_err(|e| AgentError::InvalidConfig(format!("read agent.toml: {e}")))?;
            return toml::from_str(&content)
                .map_err(|e| AgentError::InvalidConfig(format!("parse agent.toml: {e}")));
        }

        let task = Self::infer_task(name);
        tracing::info!(%name, %task, "no agent.toml found, lazily creating default manifest");
        let manifest = AgentManifest {
            agent: AgentManifestEntry {
                name: name.to_string(),
                agent_type: "stdio".into(),
                task,
                mode: "function".into(),
            },
            state: None,
        };
        self.write_manifest(&manifest).await?;
        Ok(manifest)
    }

    /// Write an agent manifest to disk.
    pub async fn write_manifest(&self, manifest: &AgentManifest) -> Result<(), AgentError> {
        let dir = self
            .workspace_path
            .join("agents")
            .join(&manifest.agent.name);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| AgentError::InvalidConfig(format!("create agent dir: {e}")))?;

        let toml_path = dir.join("agent.toml");
        let content = toml::to_string_pretty(manifest)
            .map_err(|e| AgentError::InvalidConfig(format!("serialize agent.toml: {e}")))?;

        tokio::fs::write(&toml_path, content)
            .await
            .map_err(|e| AgentError::InvalidConfig(format!("write agent.toml: {e}")))?;
        Ok(())
    }

    /// Write manifest for test setup.
    pub async fn write_manifest_for_test(
        &self,
        manifest: &AgentManifest,
    ) -> Result<(), AgentError> {
        self.write_manifest(manifest).await
    }

    /// Infer the task type from the agent name.
    fn infer_task(name: &str) -> String {
        if name.contains("review") {
            crate::types::protocol::task_type::REVIEW.to_string()
        } else if name.contains("deep") || name.contains("lint") {
            crate::types::protocol::task_type::DEEP_COMPILE.to_string()
        } else {
            crate::types::protocol::task_type::COMPILE.to_string()
        }
    }

    // ── Process map access ────────────────────────────────────

    pub async fn insert_process(&self, name: &str, proc: AgentProcess) {
        let mut procs = self.processes.write().await;
        procs.insert(name.to_string(), proc);
    }

    pub async fn remove_process(&self, name: &str) {
        let mut procs = self.processes.write().await;
        procs.remove(name);
    }

    pub async fn set_status(&self, name: &str, status: AgentStatus) {
        let mut procs = self.processes.write().await;
        if let Some(proc) = procs.get_mut(name) {
            proc.status = status;
            proc.last_active = Instant::now();
        }
    }

    pub async fn list_agents(&self) -> Vec<AgentInfo> {
        let procs = self.processes.read().await;
        procs
            .iter()
            .map(|(name, proc)| AgentInfo {
                name: name.clone(),
                agent_id: proc.agent_id.clone(),
                agent_type: proc.manifest.agent.agent_type.clone(),
                task: proc.manifest.agent.task.clone(),
                status: format!("{:?}", proc.status).to_lowercase(),
            })
            .collect()
    }

    // ── OneShot dispatch (primary path) ───────────────────────

    /// Dispatch a task to an agent via spawn_and_wait (OneShot mode).
    /// This is the primary dispatch path — each task spawns a new pi process.
    pub async fn dispatch_agent(
        &self,
        name: &str,
        request: AgentRequest,
    ) -> Result<AgentResponse, AgentError> {
        let task = process::TaskRequest {
            task_id: Uuid::new_v4().to_string(),
            user_input: request.user_input,
            workspace: request.workspace_path,
            branch: request.branch,
            source_scope: request.source_scope,
            task_type: request.task_type,
        };
        self.spawn_and_wait(name, task).await
    }

    /// Spawn agent process, send task, wait for result.
    /// Uses RPC mode to parse tool_start/tool_end events from pi stdout.
    pub async fn spawn_and_wait(
        &self,
        name: &str,
        task: process::TaskRequest,
    ) -> Result<AgentResponse, AgentError> {
        let agent_home = self.workspace_path.join("agents").join(name);
        let handle = crate::harness::pi::PiAgentHandle::new(
            agent_home,
            self.mcp_url().to_string(),
            self.llm.clone(),
        )
        .with_event_tx(self.event_tx.clone());

        let ctx = crate::harness::AgentContext {
            execution_id: Uuid::new_v4().to_string(),
            workspace: task.workspace,
            branch: task.branch,
            source_scope: task.source_scope,
            mcp_url: self.mcp_url().to_string(),
            task_type: task.task_type,
        };
        let start = crate::harness::AgentStart {
            prompt: task.user_input,
            source_paths: vec![],
            max_rounds: 20,
        };

        let state = handle.connect(ctx, start).await?;

        // Emit task completion event
        self.broadcast_event(AgentEvent::TaskCompleted {
            agent: name.to_string(),
            task_id: task.task_id.clone(),
            success: state.success,
            rounds: state.rounds,
        });

        Ok(AgentResponse {
            success: state.success,
            output: None,
            usage: Some(state.usage),
            rounds: state.rounds,
            error: state.error,
            written_pages: vec![],
            removed_pages: vec![],
        })
    }

    /// Stop a running agent (kill process).
    pub async fn stop_agent(&self, name: &str) -> Result<(), AgentError> {
        process::stop_agent(self, name).await
    }

    // ── Visibility (absorbed from AgentPool) ─────────────────

    /// Subscribe to agent events (SSE feed for server → frontend).
    pub fn subscribe_events(&self) -> broadcast::Receiver<AgentEvent> {
        self.event_tx.subscribe()
    }

    /// Emit an event to all subscribers.
    pub fn broadcast_event(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event);
    }

    // ── Stream-mode registry (absorbed from AgentPool) ───────

    /// Register a long-lived agent (future use).
    pub async fn register_stream_agent(&self, name: &str, handle: Arc<dyn AgentHandle>) {
        let mut registry = self.registry.write().await;
        registry.insert(
            name.to_string(),
            StreamAgent {
                handle,
                status: StreamAgentStatus::Idle,
                last_active: Instant::now(),
                agent_id: format!("{}-{}", name, Uuid::new_v4()),
            },
        );
        tracing::info!(%name, "stream agent registered");
    }

    /// Unregister a stream agent.
    pub async fn unregister_stream_agent(&self, name: &str) {
        let mut registry = self.registry.write().await;
        if let Some(agent) = registry.remove(name) {
            let _ = agent.handle.close().await;
            self.broadcast_event(AgentEvent::AgentStopped {
                agent: name.to_string(),
                reason: "unregistered".into(),
            });
        }
    }

    /// Start idle agent reaper (spawns background task).
    pub fn start_reaper(self: &Arc<Self>) {
        let mgr = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                let mut to_remove = Vec::new();

                {
                    let registry = mgr.registry.read().await;
                    for (name, agent) in registry.iter() {
                        if agent.status == StreamAgentStatus::Idle
                            && agent.last_active.elapsed().as_secs() > mgr.idle_timeout_secs
                        {
                            to_remove.push(name.clone());
                        }
                    }
                }

                for name in &to_remove {
                    let mut registry = mgr.registry.write().await;
                    if let Some(agent) = registry.remove(name) {
                        let _ = agent.handle.close().await;
                        mgr.broadcast_event(AgentEvent::AgentStopped {
                            agent: name.clone(),
                            reason: "idle timeout".into(),
                        });
                        tracing::info!(%name, "stream agent reaped (idle timeout)");
                    }
                }
            }
        });
    }
}

// ── Public Types ──────────────────────────────────────────────

/// Agent status for API responses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentInfo {
    pub name: String,
    pub agent_id: String,
    pub agent_type: String,
    pub task: String,
    pub status: String,
}
