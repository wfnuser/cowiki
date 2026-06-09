use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Events emitted during compile execution, streamed to frontend via SSE.
///
/// Event types follow the SSE protocol: each event has a `type` field
/// and is serialized as a JSON line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CompileEvent {
    /// A phase has started or completed.
    #[serde(rename = "phase")]
    Phase {
        /// "shallow" or "deep"
        phase: String,
        /// "started" or "completed"
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pages_count: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        timestamp: DateTime<Utc>,
    },

    /// An agent has started processing.
    #[serde(rename = "agent-start")]
    AgentStart {
        agent_id: String,
        harness_type: String,
        timestamp: DateTime<Utc>,
    },

    /// An LLM round completed within an agent.
    #[serde(rename = "llm-round")]
    LlmRound {
        round: u32,
        token_count: u64,
        timestamp: DateTime<Utc>,
    },

    /// A tool was called by the agent.
    #[serde(rename = "tool-call")]
    ToolCall {
        tool: String,
        args: serde_json::Value,
        round: u32,
        timestamp: DateTime<Utc>,
    },

    /// A tool call result was received.
    #[serde(rename = "tool-result")]
    ToolResult {
        tool: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        timestamp: DateTime<Utc>,
    },

    /// An entity was discovered.
    #[serde(rename = "entity-discovered")]
    EntityDiscovered {
        entity: String,
        entity_type: String,
        confidence: f32,
        timestamp: DateTime<Utc>,
    },

    /// A fact/relationship was extracted.
    #[serde(rename = "fact")]
    Fact {
        subject: String,
        predicate: String,
        object: String,
        timestamp: DateTime<Utc>,
    },

    /// A DeepCompile issue was detected.
    #[serde(rename = "deep-compile-issue")]
    DeepCompileIssue {
        issue_type: String, // "contradiction" | "duplicate" | "orphan" | "broken_link" | "missing_backlink" | "stale"
        #[serde(skip_serializing_if = "Option::is_none")]
        pages: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        timestamp: DateTime<Utc>,
    },

    /// An error occurred (may be recoverable).
    #[serde(rename = "error")]
    Error {
        message: String,
        recoverable: bool,
        timestamp: DateTime<Utc>,
    },
}

impl CompileEvent {
    pub fn now() -> DateTime<Utc> {
        Utc::now()
    }

    // ── Phase events ────────────────────────────────────────

    pub fn phase_started(phase: &str) -> Self {
        Self::Phase {
            phase: phase.to_string(),
            status: "started".to_string(),
            pages_count: None,
            message: None,
            timestamp: Self::now(),
        }
    }

    pub fn phase_completed(phase: &str, pages_count: usize) -> Self {
        Self::Phase {
            phase: phase.to_string(),
            status: "completed".to_string(),
            pages_count: Some(pages_count),
            message: None,
            timestamp: Self::now(),
        }
    }

    pub fn phase_message(phase: &str, message: &str) -> Self {
        Self::Phase {
            phase: phase.to_string(),
            status: "progress".to_string(),
            pages_count: None,
            message: Some(message.to_string()),
            timestamp: Self::now(),
        }
    }

    // ── Agent events ────────────────────────────────────────

    pub fn agent_start(agent_id: &str, harness: &str) -> Self {
        Self::AgentStart {
            agent_id: agent_id.to_string(),
            harness_type: harness.to_string(),
            timestamp: Self::now(),
        }
    }

    // ── LLM round events ────────────────────────────────────

    pub fn llm_round(round: u32, token_count: u64) -> Self {
        Self::LlmRound {
            round,
            token_count,
            timestamp: Self::now(),
        }
    }

    // ── Tool events ─────────────────────────────────────────

    pub fn tool_call(tool: &str, args: serde_json::Value, round: u32) -> Self {
        Self::ToolCall {
            tool: tool.to_string(),
            args,
            round,
            timestamp: Self::now(),
        }
    }

    pub fn tool_result(tool: &str, success: bool, summary: Option<&str>) -> Self {
        Self::ToolResult {
            tool: tool.to_string(),
            success,
            summary: summary.map(|s| s.to_string()),
            timestamp: Self::now(),
        }
    }

    // ── Entity / Fact events ────────────────────────────────

    pub fn entity_found(name: &str, entity_type: &str, confidence: f32) -> Self {
        Self::EntityDiscovered {
            entity: name.to_string(),
            entity_type: entity_type.to_string(),
            confidence,
            timestamp: Self::now(),
        }
    }

    pub fn fact_extracted(subject: &str, predicate: &str, object: &str) -> Self {
        Self::Fact {
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            timestamp: Self::now(),
        }
    }

    // ── DeepCompile issue events ────────────────────────────

    pub fn contradiction(pages: Vec<String>, description: Option<&str>) -> Self {
        Self::DeepCompileIssue {
            issue_type: "contradiction".into(),
            pages: Some(pages),
            description: description.map(|s| s.to_string()),
            timestamp: Self::now(),
        }
    }

    pub fn duplicate(pages: Vec<String>, description: Option<&str>) -> Self {
        Self::DeepCompileIssue {
            issue_type: "duplicate".into(),
            pages: Some(pages),
            description: description.map(|s| s.to_string()),
            timestamp: Self::now(),
        }
    }

    pub fn orphan(entity: &str) -> Self {
        Self::DeepCompileIssue {
            issue_type: "orphan".into(),
            pages: Some(vec![entity.to_string()]),
            description: None,
            timestamp: Self::now(),
        }
    }

    pub fn broken_link(target: &str, source_page: &str) -> Self {
        Self::DeepCompileIssue {
            issue_type: "broken_link".into(),
            pages: Some(vec![source_page.to_string()]),
            description: Some(format!("[[{target}]] points to non-existent page")),
            timestamp: Self::now(),
        }
    }

    // ── Error events ────────────────────────────────────────

    pub fn error(message: &str, recoverable: bool) -> Self {
        Self::Error {
            message: message.to_string(),
            recoverable,
            timestamp: Self::now(),
        }
    }

    pub fn recoverable_error(message: &str) -> Self {
        Self::error(message, true)
    }

    pub fn fatal_error(message: &str) -> Self {
        Self::error(message, false)
    }
}
