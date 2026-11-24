//! Core local-first knowledge engine for CoWiki:
//! - Open Knowledge Format (OKF) v0.3 frontmatter, trust reset, and validation (`okf`)
//! - Pluggable local/remote dense text embeddings (`embeddings`)
//! - Derived SQLite FTS5 + dense vector + link-graph index (`knowledge_index`)

pub mod embeddings;
pub mod governance;
pub mod knowledge_index;
pub mod okf;
