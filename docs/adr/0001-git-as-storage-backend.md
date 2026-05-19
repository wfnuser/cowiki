# Git as the storage backend for versioned knowledge

We use a single Git repository as the storage layer for the shared wiki. Personal Spaces are branches (`user/{user_id}`) in the same repo, and the Shared Space is `main`. All user-facing operations go through the cowiki API — Git is never exposed to end users.

We chose Git over a custom event-sourcing store (like SMF v0.1) or a database-only approach because it gives us version history, diffing, branching, and merge for free, while keeping the data as plain Markdown files that humans can read with any tool. The trade-off is that Git is not ideal for high-frequency concurrent writes, but for an MVP with a small team this is acceptable. If we outgrow Git, we can migrate to CRDT-based storage later without changing the API surface.
