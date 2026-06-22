//! Agent lifecycle manager — spawn, dispatch, reap.
//!
//! The AgentManager owns agent process lifecycles via stdin/stdout communication:
//! - **execute()**: one-shot session — create session, spawn agent, collect tracking.
//!
//! Long-lived agents are managed through the internal registry and reaper.

pub mod session;
pub mod skills;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::harness::AgentHandle;
use crate::types::bridge::{AgentManifest, AgentManifestEntry, AgentStatus};
use crate::types::error::AgentError;
use crate::types::protocol::AgentResult;

use self::session::SessionManager;

// ── Configuration ─────────────────────────────────────────────

/// Default SKILL.md for compiler agents.
pub const DEFAULT_COMPILER_SKILL: &str = include_str!("../../skills/compiler/SKILL.md");

// ── AgentManager ──────────────────────────────────────────────

/// Manages the lifecycle of agent processes.
///
/// - `execute()`: creates a session, spawns agent, collects per-session tracking.
pub struct AgentManager {
    /// Agent processes keyed by agent name (OneShot)
    pub(crate) processes: Arc<RwLock<HashMap<String, AgentProcess>>>,

    /// Stream-mode agent registry (absorbed from AgentPool)
    registry: Arc<RwLock<HashMap<String, StreamAgent>>>,

    /// Per-session lifecycle, tracking, and SSE management.
    pub session_manager: Arc<SessionManager>,

    /// Server HTTP API URL (e.g., "http://127.0.0.1:3000")
    /// Agent CLI uses this as COWIKI_BASE_URL.
    server_url: String,
    /// Cowiki data dir root ($COWIKI_DATA_DIR)
    data_dir: PathBuf,
    /// LLM config for models.json generation
    pub llm: cowiki_utils::LlmConfig,
    /// Idle timeout for stream agents
    idle_timeout_secs: u64,
    /// Agent runtime config (timeouts, concurrency)
    pub agent_config: cowiki_utils::AgentConfig,
    /// Per-workspace concurrency semaphores
    concurrency: RwLock<HashMap<String, Arc<tokio::sync::Semaphore>>>,
    /// GitHub token for Copilot ACP auth (COPILOT_GITHUB_TOKEN)
    github_token: Option<String>,
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
    /// OS process ID for external kill (nix signal)
    pub pid: Option<u32>,
}

impl AgentManager {
    /// Create a new agent manager.
    pub fn new(
        server_url: String,
        data_dir: PathBuf,
        llm: cowiki_utils::LlmConfig,
        agent_config: cowiki_utils::AgentConfig,
        github_token: Option<String>,
    ) -> Self {
        tracing::info!(
            server_url = %server_url,
            data_dir = %data_dir.display(),
            has_github_token = github_token.is_some(),
            hard_timeout = agent_config.hard_timeout_secs,
            max_concurrent = agent_config.max_concurrent_per_workspace,
            "created AgentManager"
        );

        let manager = Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            registry: Arc::new(RwLock::new(HashMap::new())),
            session_manager: Arc::new(SessionManager::new()),
            server_url,
            data_dir,
            llm,
            idle_timeout_secs: 900,
            agent_config,
            concurrency: RwLock::new(HashMap::new()),
            github_token,
        };

        manager
    }

    // ── Accessors ─────────────────────────────────────────────

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    fn agent_home_for(&self, workspace: &str, name: &str) -> PathBuf {
        self.data_dir
            .join(workspace)
            .join("repo")
            .join("agents")
            .join(name)
    }

    // ── Manifest I/O ──────────────────────────────────────────

    /// Load an agent manifest from disk for a workspace, or lazily create a default.
    pub async fn load_or_create_manifest(
        &self,
        workspace: &str,
        name: &str,
    ) -> Result<AgentManifest, AgentError> {
        let toml_path = self.agent_home_for(workspace, name).join("agent.toml");
        if toml_path.exists() {
            let content = std::fs::read_to_string(&toml_path)
                .map_err(|e| AgentError::InvalidConfig(format!("read agent.toml: {e}")))?;
            return toml::from_str(&content)
                .map_err(|e| AgentError::InvalidConfig(format!("parse agent.toml: {e}")));
        }

        tracing::info!(%name, "no agent.toml found, lazily creating default manifest");
        let manifest = AgentManifest {
            agent: AgentManifestEntry {
                name: name.to_string(),
                agent_type: "stdio".into(),
                task: String::new(), // No task inference — agents crate is pure infra
                mode: "function".into(),
            },
            state: None,
        };
        self.write_manifest(workspace, &manifest).await?;
        Ok(manifest)
    }

    /// Write an agent manifest to disk for a workspace.
    pub async fn write_manifest(
        &self,
        workspace: &str,
        manifest: &AgentManifest,
    ) -> Result<(), AgentError> {
        let dir = self.agent_home_for(workspace, &manifest.agent.name);
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
        self.write_manifest("default", manifest).await
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

    // ── Execute (primary path) ────────────────────────────────

    /// Execute an agent session. This is the primary entry point.
    ///
    /// Creates a volatile session, spawns the agent with the given prompt and skills,
    /// collects per-session tracking, and returns the result.
    ///
    /// `agent_name` — matches `agents/{name}/agent.toml`
    /// `prompt` — final prompt with slash command (built by dispatcher)
    /// `skills` — skill file names to install (e.g., ["cowiki-compiler"])
    /// `workspace` — workspace slug
    /// `extra_args` — passed as COWIKI_EXTRA_ARGS env var (base64 JSON)
    /// `branch` — git branch for this execution
    /// `source_scope` — paths the agent is allowed to read
    pub async fn execute(
        &self,
        agent_name: &str,
        prompt: &str,
        skills: &[&str],
        workspace: &str,
        extra_args: HashMap<String, String>,
    ) -> Result<AgentResult, AgentError> {
        // 1. Create session
        let session_id = self
            .session_manager
            .create(agent_name.to_string(), extra_args.clone())
            .await;

        let agent_home = self.agent_home_for(workspace, agent_name);
        crate::manager::skills::install_skills(&agent_home, skills).await?;

        self.session_manager
            .start_tracking(&session_id, workspace)
            .await;

        let result = self
            .spawn_and_wait(agent_name, prompt, workspace, &session_id, &extra_args)
            .await;

        // 5. Stop tracking and collect results
        let tracking = self.session_manager.stop_tracking(&session_id).await;

        match result {
            Ok(state) => {
                self.session_manager.complete(&session_id).await;

                let (written_files, removed_files) = tracking
                    .map(|t| (t.written_files, t.removed_files))
                    .unwrap_or_default();

                Ok(AgentResult {
                    success: state.success,
                    rounds: state.rounds,
                    usage: state.usage.clone(),
                    written_files,
                    removed_files,
                    error: state.error.clone(),
                })
            }
            Err(e) => {
                let err_msg = e.to_string();
                self.session_manager.fail(&session_id, &err_msg).await;
                Err(e)
            }
        }
    }

    /// Spawn agent process, send prompt, wait for result.
    /// Internal method called by execute().
    ///
    /// `session_id` is used as execution_id for tracking correlation.
    async fn spawn_and_wait(
        &self,
        name: &str,
        prompt: &str,
        workspace: &str,
        session_id: &str,
        extra_args: &HashMap<String, String>,
    ) -> Result<crate::harness::AgentState, AgentError> {
        let sem = {
            let mut map = self.concurrency.write().await;
            map.entry(workspace.to_string())
                .or_insert_with(|| {
                    Arc::new(tokio::sync::Semaphore::new(
                        self.agent_config.max_concurrent_per_workspace,
                    ))
                })
                .clone()
        };
        let _permit = sem.acquire().await.map_err(|_| AgentError::PoolExhausted {
            task_type: "unknown".into(),
        })?;

        let agent_type = resolve_agent_type(&self.data_dir, workspace, name).await;
        let agent_home = self.agent_home_for(workspace, name);
        let api_key = extra_args.get("api_key").cloned().unwrap_or_default();

        let ctx = crate::harness::AgentContext {
            execution_id: session_id.to_string(),
            workspace: workspace.to_string(),
            server_url: self.server_url().to_string(),
            api_key,
        };
        let start = crate::harness::AgentStart {
            prompt: prompt.to_string(),
            source_paths: vec![],
            max_rounds: 20,
        };

        let state = match agent_type {
            AgentType::Copilot => {
                let handle = crate::harness::copilot::CopilotAgentHandle::new(
                    agent_home,
                    self.server_url().to_string(),
                    self.llm.clone(),
                    self.agent_config.hard_timeout_secs,
                    self.github_token.clone(),
                );
                handle.connect(ctx, start).await?
            }
            AgentType::Pi => {
                let handle = crate::harness::pi::PiAgentHandle::new(
                    agent_home,
                    self.server_url().to_string(),
                    self.llm.clone(),
                    self.agent_config.hard_timeout_secs,
                    self.agent_config.soft_timeout_secs,
                )
                .with_process_map(self.processes.clone());
                handle.connect(ctx, start).await?
            }
        };

        Ok(state)
    }

    /// Stop a running agent: SIGTERM → wait → SIGKILL.
    pub async fn stop_agent(&self, name: &str) -> Result<(), AgentError> {
        let proc_entry = {
            let mut procs = self.processes.write().await;
            procs.remove(name)
        };
        let mut proc = match proc_entry {
            Some(p) => p,
            None => return Ok(()),
        };
        if matches!(proc.status, AgentStatus::Stopped) {
            return Ok(());
        }
        proc.status = AgentStatus::Stopped;
        if let Some(ref mut c) = proc.child {
            tracing::info!(%name, "sending SIGTERM");
            let _ = c.start_kill();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            match c.try_wait() {
                Ok(Some(s)) => tracing::info!(%name, %s, "agent exited"),
                Ok(None) => {
                    tracing::warn!(%name, "SIGKILL");
                    let _ = c.kill().await;
                }
                Err(e) => tracing::error!(%name, %e, "wait error"),
            }
        } else if let Some(pid) = proc.pid {
            let s = pid.to_string();
            let _ = tokio::process::Command::new("kill").arg(&s).spawn();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let _ = tokio::process::Command::new("kill")
                .arg("-9")
                .arg(&s)
                .spawn();
        }
        tracing::info!(%name, "agent stopped");
        Ok(())
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
            tracing::info!(%name, "stream agent unregistered");
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

// ── Agent type resolution ─────────────────────────────────────

/// Agent runtime type for harness selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    Pi,
    Copilot,
}

impl AgentType {
    pub fn as_str(&self) -> &str {
        match self {
            AgentType::Pi => "pi",
            AgentType::Copilot => "copilot",
        }
    }
}

/// Resolve which agent type to use for a given agent name.
///
/// Resolution order:
/// 1. Read from `agents/{name}/agent.toml` manifest
/// 2. "auto" mode: try Copilot if CLI is available, fallback to pi
/// 3. Default: Pi (always available)
async fn resolve_agent_type(data_dir: &std::path::Path, workspace: &str, name: &str) -> AgentType {
    let toml_path = data_dir
        .join(workspace)
        .join("repo")
        .join("agents")
        .join(name)
        .join("agent.toml");
    if let Ok(content) = tokio::fs::read_to_string(&toml_path).await {
        if let Ok(manifest) = toml::from_str::<crate::types::bridge::AgentManifest>(&content) {
            let at = manifest.agent.agent_type.as_str();
            tracing::info!(%name, agent_type=%at, "resolved agent type from manifest");
            return agent_type_from_str(at);
        }
    }

    // 2. Default: pi
    tracing::info!(%name, "no agent.toml found, defaulting to pi agent");
    AgentType::Pi
}

/// Parse agent type string, with "auto" detection logic.
fn agent_type_from_str(s: &str) -> AgentType {
    match s {
        "copilot" => AgentType::Copilot,
        "pi" => AgentType::Pi,
        "auto" => {
            // Use which to detect copilot CLI availability (non-blocking, no process spawn)
            match std::process::Command::new("which").arg("copilot").output() {
                Ok(output) if output.status.success() => {
                    tracing::info!("auto agent: copilot detected, using Copilot");
                    AgentType::Copilot
                }
                _ => {
                    tracing::info!("auto agent: copilot not found, falling back to pi");
                    AgentType::Pi
                }
            }
        }
        other => {
            tracing::warn!(agent_type=%other, "unknown agent_type, falling back to pi");
            AgentType::Pi
        }
    }
}
