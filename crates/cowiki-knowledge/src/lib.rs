//! Core local-first knowledge engine for CoWiki:
//! - Open Knowledge Format (OKF) v0.3 frontmatter, trust reset, and validation (`okf`)
//! - Pluggable local/remote dense text embeddings (`embeddings`)
//! - Hash- and span-anchored verifiable citations (`citations`)
//! - Pluggable shared-context evidence scoring (`scorer`)
//! - Derived SQLite FTS5 + dense vector + link-graph + lexical-bridge index (`knowledge_index`)

pub mod citations;
pub mod embeddings;
pub mod knowledge_index;
pub mod okf;
pub mod scorer;
