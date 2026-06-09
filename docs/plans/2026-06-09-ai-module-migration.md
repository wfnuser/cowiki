# AI Module Migration — Design Note

> Status: Implemented | Date: 2026-06-09 | Branch: `docs/compile-system-proposal`

## Decision

Remove `crates/core/src/ai/` and redistribute its contents to the crates that actually consume them.

## Rationale

`core/src/ai/` was designed for a direct-LLM architecture: cowiki calls `Llm::chat(system, user) → String`, embedded in-process, without agents. The new architecture inverts this — agents think, cowiki executes tools and proxies LLM calls. The old `Llm` trait (single-turn, no streaming, no tools, OpenAI-only) is incompatible with this model. Meanwhile, `embedder` is consumed exclusively by `crates/db/` for pgvector, and `token_usage` is a shared data type with no AI-specific logic.

## Migration Map

```
crates/core/src/ai/          → deleted
  ├── llm/                   → crates/agents/src/embedded.rs
  ├── vlm/                   → merged into embedded.rs
  ├── embedder/              → crates/db/src/embed.rs
  ├── token_usage.rs         → crates/utils/src/token_usage.rs  (shared; avoids cycle)
  └── mod.rs                 → deleted
```

## Final Module Ownership

| Module | New Home | Consumer |
|--------|----------|----------|
| `EmbeddedAgent` (ex-LLM + VLM) | `crates/agents/src/embedded.rs` | submit.rs summary generation, compile.rs (simple Text path), future dispatcher |
| `Embedder` trait + `OpenAIEmbedder` | `crates/db/src/embed.rs` | pgvector upsert (legacy Compiler → embed only) |
| `TokenUsage` + `TokenUsageTracker` | `crates/utils/src/token_usage.rs` | EmbeddedAgent, Embedder, usage endpoint — shared across all crates |

### Note: Why token_usage is in utils

Moving `token_usage` to `crates/core/` created a cyclic dependency: `core → agents` (existing) but `agents → core` (needed for token_usage). Placing it in `crates/utils/` breaks the cycle — both `agents` and `db` and `server` depend on utils with no reverse edge.

## File Changes

### Deleted
- `crates/core/src/ai/mod.rs`
- `crates/core/src/ai/llm/mod.rs`
- `crates/core/src/ai/llm/openai.rs`
- `crates/core/src/ai/llm/registry.rs`
- `crates/core/src/ai/vlm/mod.rs`
- `crates/core/src/ai/vlm/openai.rs`
- `crates/core/src/ai/vlm/registry.rs`
- `crates/core/src/ai/embedder/mod.rs`
- `crates/core/src/ai/embedder/openai.rs`
- `crates/core/src/ai/embedder/registry.rs`
- `crates/core/src/ai/token_usage.rs`

### Created / Moved
- `crates/core/src/token_usage.rs` (moved from `ai/token_usage.rs`)
- `crates/agents/src/embedded.rs` (new: simple multi-turn agent)
- `crates/db/src/embed.rs` (moved from `ai/embedder/`, merged)

### Modified
- `crates/core/src/lib.rs` — remove `pub mod ai`, add `pub mod token_usage`
- `crates/core/Cargo.toml` — remove `reqwest` dep (only used by llm/vlm now moved)
- `crates/agents/Cargo.toml` — add `reqwest`, `async-trait`, `serde` as needed
- `crates/db/Cargo.toml` — add `reqwest`, `async-trait` for embedder
- `crates/db/src/lib.rs` — add `pub mod embed`, register embed migration
- `crates/server/src/main.rs` — update imports from `core::ai::*` to new paths
- `crates/core/src/compiler/shallow.rs` — remove legacy Compiler, delegate to agents

## EmbeddedAgent Design

```rust
// crates/agents/src/embedded.rs

pub struct EmbeddedAgent {
    config: EmbeddedAgentConfig,
    tracker: Arc<Mutex<TokenUsageTracker>>,
}

pub struct EmbeddedAgentConfig {
    pub provider: String,        // "anthropic" | "openai"
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub temperature: f64,
    pub max_tokens: Option<u32>,
    pub max_rounds: u32,         // default 5
    pub token_budget: u32,       // default 50_000
}

impl EmbeddedAgent {
    /// Multi-turn chat with tool calling loop.
    /// Stateless — no session persistence, no compaction.
    /// Suitable for short Text sources (< 4KB).
    pub async fn run(
        &self,
        messages: Vec<LlmMessage>,
        tools: Vec<ToolDef>,
        tool_handler: Arc<dyn ToolHandler>,
    ) -> Result<AgentResponse, AgentError>;
}
```

## Related

- [`docs/agent-integration-design.md`](../agent-integration-design.md) — Agent runtime integration
- [`docs/compile-system-design.md`](../compile-system-design.md) — Compile pipeline
