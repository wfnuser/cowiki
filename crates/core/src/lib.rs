pub mod git;
pub mod models;
pub mod compiler;
pub mod dedup;

// Re-export the new compiler submodules
pub use compiler::AgentPool;
pub use compiler::SourceDispatcher;
