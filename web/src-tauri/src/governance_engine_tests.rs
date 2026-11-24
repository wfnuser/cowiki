//! Headless entry point for exercising the production LocalEngine with real Git worktrees.
pub use cowiki_knowledge::{knowledge_index, okf};
pub mod agent_execution_gate;
pub mod cloud_sync;
pub mod extract;
pub mod html_assets;
pub mod link_rewrite;
pub mod local_engine;
pub mod obsidian;
pub mod personal;
pub mod web_source;
