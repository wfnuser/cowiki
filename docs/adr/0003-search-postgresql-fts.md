# Search uses PostgreSQL Full-Text Search

We use PostgreSQL's built-in tsvector/tsquery for keyword search instead of grep, Elasticsearch, or Meilisearch. Pages are indexed on write (compile/create) with a GIN-indexed tsvector column combining title, summary, and body text.

GitHub showed that grep-style file scanning doesn't scale (their 115TB codebase needed a custom Rust search engine). For CoWiki's scale (thousands of pages, not millions), PostgreSQL FTS with GIN index gives <10ms query times with zero additional infrastructure. We already have pgvector for semantic search — FTS and vector search coexist in the same table, queries can combine both.

If we outgrow PostgreSQL FTS (hundreds of thousands of pages), the migration path is Meilisearch or Tantivy (Rust-native full-text search). The API surface stays the same.
