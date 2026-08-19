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

# Node 20 (from NodeSource, not Debian's Node 18) because Playwright requires
# Node >= 20. Pin npm's global prefix to /usr/local so CLAUDE_CODE_BIN and the
# global bins (claude, mcp-server-playwright) resolve where the rest of the
# image expects them.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg gh git \
    && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm config set prefix /usr/local \
    && npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
    && claude --version \
    && npm cache clean --force \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
# Browsers live outside HOME because the k8s deployment mounts a volume over
# /claude-home at runtime, which would otherwise shadow anything baked there.
ENV HOME=/claude-home \
    CLAUDE_CODE_BIN=/usr/local/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe \
    DISABLE_AUTOUPDATER=1 \
    PLAYWRIGHT_BROWSERS_PATH=/ms-playwright

# Playwright CLI + browsers (so the QA template's `npx playwright test` runs) and
# the Playwright MCP (so QA agents can drive a browser via mcp__playwright__*).
RUN npm install -g playwright@latest @playwright/mcp@latest \
    && playwright install --with-deps chromium \
    # @playwright/mcp's real bin name isn't `mcp-server-playwright`; resolve the
    # actual entry from its package.json and expose it under a stable name so the
    # MCP config below works without npx (no network) or bin-name guessing.
    && PKG="$(npm root -g)/@playwright/mcp" \
    && BIN="$(node -e "const b=require('${PKG}/package.json').bin; process.stdout.write(typeof b==='string'?b:(Object.values(b)[0]||''))")" \
    && test -n "${BIN}" && test -f "${PKG}/${BIN}" \
    && ln -sf "${PKG}/${BIN}" /usr/local/bin/mcp-server-playwright \
    && chmod +x "${PKG}/${BIN}" \
    && npm cache clean --force \
    && rm -rf /var/lib/apt/lists/*

# MCP config wired into QA runs via `claude --mcp-config`. --no-sandbox is
# required because chromium runs as root inside the container.
RUN printf '%s' '{"mcpServers":{"playwright":{"command":"mcp-server-playwright","args":["--headless","--isolated","--no-sandbox","--browser","chromium"]}}}' > /app/qa-mcp.json

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
