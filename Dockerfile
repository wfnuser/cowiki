# Stage 1: Build Rust backend
FROM rust:1.88-bookworm AS builder

WORKDIR /app

# Install system dependencies for libgit2
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev cmake libgit2-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies: copy manifests first
COPY Cargo.toml Cargo.lock ./
COPY crates/server/Cargo.toml crates/server/
COPY crates/core/Cargo.toml crates/core/
COPY crates/db/Cargo.toml crates/db/
COPY crates/utils/Cargo.toml crates/utils/
COPY crates/extractor/Cargo.toml crates/extractor/

# Create dummy source files so cargo can resolve the workspace
RUN mkdir -p crates/server/src crates/core/src crates/db/src crates/utils/src crates/extractor/src \
    && echo "fn main() {}" > crates/server/src/main.rs \
    && touch crates/core/src/lib.rs crates/db/src/lib.rs crates/utils/src/lib.rs crates/extractor/src/lib.rs

RUN cargo build --release --workspace 2>/dev/null || true

# Copy real source and build. Touch every source file so cargo rebuilds all
# workspace crates with the real code (the dummy-lib step above only primed the
# external dependency cache); otherwise stale empty-lib artifacts get reused.
COPY crates/ crates/
RUN find crates -name '*.rs' -exec touch {} + \
    && cargo build --release --workspace

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 libgit2-1.5 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/cowiki-server /usr/local/bin/cowiki-server

RUN useradd -r -s /bin/false cowiki
USER cowiki

EXPOSE 3000

ENTRYPOINT ["cowiki-server"]
