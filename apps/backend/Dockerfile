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

# When true (default), the runtime image bundles Playwright's chromium +
# chrome-for-testing so backend-spawned QA agents can drive a browser. The u2s
# self-host build overrides this to false: its 8 GB disk cannot fit the ~1 GB of
# browsers (the pull fills the disk and fails). Only the u2s :main image is slim;
# main-arm64 / oracle keep the default. See .github/workflows/deploy-u2s.yml.
ARG INSTALL_BROWSERS=true

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

# Security scanners for the `security_scan` template. The worker runs these on PATH:
# semgrep (SAST) from pip's prebuilt aarch64 wheel, osv-scanner (SCA) as a pinned
# static binary. Versions are pinned so findings are reproducible across builds.
ARG SEMGREP_VERSION=1.176.0
ARG OSV_SCANNER_VERSION=2.5.1
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-pip \
    && pip3 install --no-cache-dir --break-system-packages "semgrep==${SEMGREP_VERSION}" \
    && ARCH="$(dpkg --print-architecture)" \
    && curl -fsSL -o /usr/local/bin/osv-scanner \
         "https://github.com/google/osv-scanner/releases/download/v${OSV_SCANNER_VERSION}/osv-scanner_linux_${ARCH}" \
    && chmod +x /usr/local/bin/osv-scanner \
    && semgrep --version \
    && osv-scanner --version \
    && rm -rf /var/lib/apt/lists/* /root/.cache

# nuclei (active DAST) for the security_dast template. Templates are baked at build
# time into /opt/nuclei-home/nuclei-templates — a path NOT shadowed by the runtime
# volume mounts on /claude-home — so runs use them via `-t` with update checks
# disabled and never download templates or phone home at run time.
ARG NUCLEI_VERSION=3.11.1
RUN apt-get update \
    && apt-get install -y --no-install-recommends unzip \
    && ARCH="$(dpkg --print-architecture)" \
    && curl -fsSL -o /tmp/nuclei.zip \
         "https://github.com/projectdiscovery/nuclei/releases/download/v${NUCLEI_VERSION}/nuclei_${NUCLEI_VERSION}_linux_${ARCH}.zip" \
    && unzip -o /tmp/nuclei.zip nuclei -d /usr/local/bin \
    && chmod +x /usr/local/bin/nuclei \
    && rm -f /tmp/nuclei.zip \
    && HOME=/opt/nuclei-home nuclei -update-templates \
    && nuclei -version \
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
# The npm packages + symlinks are ALWAYS installed (small, and the MCP configs
# below reference them). Only the heavy browser downloads (~1 GB) are gated on
# INSTALL_BROWSERS, so the slim u2s image still boots with the MCP binaries
# present — it just has no browser to launch.
RUN npm install -g playwright@latest @playwright/mcp@latest @smart-coder-labs/nexusmind-mcp@0.15.0 \
    # @playwright/mcp's real bin name isn't `mcp-server-playwright`; resolve the
    # actual entry from its package.json and expose it under a stable name so the
    # MCP config below works without npx (no network) or bin-name guessing.
    && PKG="$(npm root -g)/@playwright/mcp" \
    && BIN="$(node -e "const b=require('${PKG}/package.json').bin; process.stdout.write(typeof b==='string'?b:(Object.values(b)[0]||''))")" \
    && test -n "${BIN}" && test -f "${PKG}/${BIN}" \
    && ln -sf "${PKG}/${BIN}" /usr/local/bin/mcp-server-playwright \
    && chmod +x "${PKG}/${BIN}" \
    # Same for the NexusMind MCP so agents (issue-resolver, lead-generation) can
    # load project context without an npx network fetch at spawn time.
    && PKGN="$(npm root -g)/@smart-coder-labs/nexusmind-mcp" \
    && BINN="$(node -e "const b=require('${PKGN}/package.json').bin; process.stdout.write(typeof b==='string'?b:(Object.values(b)[0]||''))")" \
    && test -n "${BINN}" && test -f "${PKGN}/${BINN}" \
    && ln -sf "${PKGN}/${BINN}" /usr/local/bin/nexusmind-mcp \
    && chmod +x "${PKGN}/${BINN}" \
    # Browser downloads are the heavy part (~1 GB). --with-deps also apt-installs
    # the OS libs chrome-for-testing needs, so both browser steps live together
    # inside the guard. Skipped entirely when INSTALL_BROWSERS=false (u2s).
    && if [ "$INSTALL_BROWSERS" = "true" ]; then \
         playwright install --with-deps chromium \
         && mcp-server-playwright install-browser chrome-for-testing \
         && ls -la /ms-playwright/ ; \
       else \
         echo "INSTALL_BROWSERS=false — skipping chromium + chrome-for-testing (slim u2s image)" ; \
       fi \
    && npm cache clean --force \
    && rm -rf /var/lib/apt/lists/*

# MCP config wired into QA runs via `claude --mcp-config`. --no-sandbox is
# required because chromium runs as root inside the container.
RUN printf '%s' '{"mcpServers":{"playwright":{"command":"mcp-server-playwright","args":["--headless","--isolated","--no-sandbox","--browser","chromium"]}}}' > /app/qa-mcp.json

# MCP config wired into the issue-resolver / lead-generation runs via
# `claude --mcp-config`. The server is named `plugin_nexusmind_nexusmind` so its
# tools surface as `mcp__plugin_nexusmind_nexusmind__*`, matching those templates'
# allowedTools. NEXUSMIND_API_KEY + NEXUSMIND_MCP_TOOL_PROFILE are inherited from
# the worker-set command env; the base URL is pinned here.
RUN printf '%s' '{"mcpServers":{"plugin_nexusmind_nexusmind":{"command":"nexusmind-mcp","args":[],"env":{"NEXUSMIND_BASE_URL":"https://api.nexusmind.smartcoderlabs.com","NEXUSMIND_MCP_TOOL_PROFILE":"essential"}}}}' > /app/nexusmind-mcp.json

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
