# Build stage
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y build-essential pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null; rm -rf src

COPY src ./src
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

ARG CLAUDE_CODE_VERSION=2.1.235

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git nodejs npm \
    && npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
    && claude --version \
    && npm cache clean --force \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
ENV HOME=/claude-home \
    CLAUDE_CODE_BIN=/usr/local/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe \
    DISABLE_AUTOUPDATER=1
COPY --from=builder /app/target/release/nexusmind /app/nexusmind
COPY --from=builder /app/target/release/autonomous_worker /app/autonomous_worker
# The SDD importer has to ship with the image, or it cannot be run where it is
# actually needed. It is a one-shot binary, invoked manually:
#   import-sdd --db /data/nexusmind.db --org-id <id> --root <checkout> [--dry-run]
# Note it reads the openspec/ tree from --root, so that tree must be reachable
# (bind-mounted, or a checkout inside the container). See docs/RUNNING.md.
COPY --from=builder /app/target/release/import-sdd /app/import-sdd
# Seeder, for bringing a fresh volume up with demo data:
#   nexusmind-seed /data/nexusmind.db
COPY --from=builder /app/target/release/nexusmind-seed /app/nexusmind-seed

RUN mkdir -p /data /claude-home

EXPOSE 8080
CMD ["/app/nexusmind"]
