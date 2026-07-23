FROM rust:1.95-bookworm AS builder

WORKDIR /build
COPY cloud/Cargo.toml cloud/Cargo.lock ./
COPY cloud/migrations ./migrations
COPY cloud/src ./src
RUN cargo build --locked --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git tini wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /home/cowiki cowiki \
    && install -d -o cowiki -g cowiki /var/lib/cowiki/repos

COPY --from=builder /build/target/release/cowiki-cloud /usr/local/bin/cowiki-cloud
COPY --chmod=755 cloud/entrypoint.sh /usr/local/bin/cowiki-entrypoint

ENV COWIKI_REPO_ROOT=/var/lib/cowiki/repos \
    COWIKI_BIND_ADDR=0.0.0.0:8787 \
    RUST_LOG=cowiki_cloud=info,tower_http=info

EXPOSE 8787
VOLUME ["/var/lib/cowiki/repos"]
USER cowiki
ENTRYPOINT ["/usr/local/bin/cowiki-entrypoint"]
