# Build stage
# kube-rs (factory sandbox executor) requires Rust >= 1.89; same toolchain as nexus-builder.
# Pinned to bookworm, the runtime's Debian release: a newer builder links against a
# newer glibc and the binaries fail to start on the runtime (GLIBC_2.39 not found).
FROM rust:1.98-slim-bookworm AS builder

RUN apt-get update && apt-get install -y build-essential pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY apps/backend/Cargo.toml apps/backend/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null; rm -rf src

COPY apps/backend/src ./src
RUN cargo build --release

FROM rust:1.98-slim-bookworm AS nexus-builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY apps/nexus-cli/Cargo.toml apps/nexus-cli/Cargo.lock ./
COPY apps/nexus-cli/src ./src
COPY apps/nexus-cli/sandbox ./sandbox
RUN cargo build --release --locked

# Runtime stage
FROM debian:bookworm-slim

ARG CLAUDE_CODE_VERSION=2.1.235
# Codex CLI for the `codex` executor (factory F3). It runs only in task pods,
# with its OpenAI key injected by the egress proxy.
ARG CODEX_VERSION=0.160.0
ARG OPENSHELL_VERSION=v0.1.2

# Client only. The OpenShell gateway is a separate service with its own
# sandbox runtime; the backend image must never need a Docker socket.
RUN set -eu; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) os_arch=x86_64;; arm64) os_arch=aarch64;; *) exit 1;; esac; \
    asset="openshell-${os_arch}-unknown-linux-musl.tar.gz"; \
    base="https://github.com/NVIDIA/OpenShell/releases/download/${OPENSHELL_VERSION}"; \
    apt-get update && apt-get install -y --no-install-recommends ca-certificates curl tar; \
    curl -fsSL "${base}/${asset}" -o "/tmp/${asset}"; \
    curl -fsSL "${base}/openshell-checksums-sha256.txt" -o /tmp/openshell-checksums; \
    grep "  ${asset}$" /tmp/openshell-checksums | (cd /tmp && sha256sum -c -); \
    tar -xzf "/tmp/${asset}" -C /usr/local/bin openshell; \
    openshell --version; \
    rm -f "/tmp/${asset}" /tmp/openshell-checksums; \
    rm -rf /var/lib/apt/lists/*

# When true (default), the runtime image bundles Playwright's chromium +
# chrome-for-testing so backend-spawned QA agents can drive a browser. The u2s
# self-host build overrides this to false: its 8 GB disk cannot fit the ~1 GB of
# browsers (the pull fills the disk and fails). Only the u2s :main image is slim;
# main-arm64 / oracle keep the default. See .github/workflows/deploy-u2s.yml.
ARG INSTALL_BROWSERS=true

# Node 22 (from NodeSource) because the pinned Claude Code release requires
# Node >= 22. Pin npm's global prefix to /usr/local so CLAUDE_CODE_BIN and the
# global bins (claude, mcp-server-playwright) resolve where the rest of the
# image expects them.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg gh git \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm config set prefix /usr/local \
    && npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" "@openai/codex@${CODEX_VERSION}" \
    && claude --version \
    && codex --version \
    && npm cache clean --force \
    && rm -rf /var/lib/apt/lists/*

# Security scanners for the `security_scan` template. The worker runs these on PATH:
# semgrep (SAST) from pip's prebuilt aarch64 wheel, osv-scanner (SCA) as a pinned
# static binary. Versions are pinned so findings are reproducible across builds.
#
# Gated on INSTALL_SCANNERS for the same reason as nuclei and the browsers: on the
# 6.7 GB u2s box this layer alone is what a deploy dies on — extracting semgrep's
# python3.11 dist-packages needs room the box does not have while the previous
# image is still in use by the running container, and the box does not run
# security-scan agents. Default true keeps every other target unchanged.
ARG SEMGREP_VERSION=1.176.0
ARG OSV_SCANNER_VERSION=2.5.1
ARG INSTALL_SCANNERS=true
RUN if [ "$INSTALL_SCANNERS" = "true" ]; then \
      apt-get update \
      && apt-get install -y --no-install-recommends python3 python3-pip \
      && pip3 install --no-cache-dir --break-system-packages "semgrep==${SEMGREP_VERSION}" \
      && ARCH="$(dpkg --print-architecture)" \
      && curl -fsSL -o /usr/local/bin/osv-scanner \
           "https://github.com/google/osv-scanner/releases/download/v${OSV_SCANNER_VERSION}/osv-scanner_linux_${ARCH}" \
      && chmod +x /usr/local/bin/osv-scanner \
      && semgrep --version \
      && osv-scanner --version \
      && rm -rf /var/lib/apt/lists/* /root/.cache ; \
    else \
      echo "INSTALL_SCANNERS=false — skipping semgrep + osv-scanner (slim u2s image)" ; \
    fi

# nuclei (active DAST) for the security_dast template. Templates are baked at build
# time into /opt/nuclei-home/nuclei-templates (world-readable: sandbox task pods
# run as a non-root UID; the public templates hold no secret) — a path NOT shadowed by the runtime
# volume mounts on /claude-home — so runs use them via `-t` with update checks
# disabled and never download templates or phone home at run time.
#
# Gated on INSTALL_NUCLEI: the binary plus its full template set is heavy, and a
# deployment that does not run the security_dast agent (u2s, whose 8 GB disk is
# already tight) skips it entirely. Default true keeps oracle/main unchanged.
ARG NUCLEI_VERSION=3.11.1
ARG INSTALL_NUCLEI=true
RUN if [ "$INSTALL_NUCLEI" = "true" ]; then \
      apt-get update \
      && apt-get install -y --no-install-recommends unzip \
      && ARCH="$(dpkg --print-architecture)" \
      && curl -fsSL -o /tmp/nuclei.zip \
           "https://github.com/projectdiscovery/nuclei/releases/download/v${NUCLEI_VERSION}/nuclei_${NUCLEI_VERSION}_linux_${ARCH}.zip" \
      && unzip -o /tmp/nuclei.zip nuclei -d /usr/local/bin \
      && chmod +x /usr/local/bin/nuclei \
      && rm -f /tmp/nuclei.zip \
      && HOME=/opt/nuclei-home nuclei -update-templates \
      && chmod -R a+rX /opt/nuclei-home \
      && nuclei -version \
      && rm -rf /var/lib/apt/lists/* ; \
    else \
      echo "INSTALL_NUCLEI=false — skipping nuclei + templates (slim u2s image)" ; \
    fi

WORKDIR /app
# Browsers live outside HOME because the k8s deployment mounts a volume over
# /claude-home at runtime, which would otherwise shadow anything baked there.
ENV HOME=/claude-home \
    CLAUDE_CODE_BIN=/usr/local/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe \
    DISABLE_AUTOUPDATER=1 \
    PLAYWRIGHT_BROWSERS_PATH=/ms-playwright

# The NexusMind MCP is ALWAYS installed — every agent uses it for project
# context, and it is small. Its symlink lets agents load it without an npx
# network fetch at spawn time.
RUN npm install -g @smart-coder-labs/nexusmind-mcp@0.15.0 \
    && PKGN="$(npm root -g)/@smart-coder-labs/nexusmind-mcp" \
    && BINN="$(node -e "const b=require('${PKGN}/package.json').bin; process.stdout.write(typeof b==='string'?b:(Object.values(b)[0]||''))")" \
    && test -n "${BINN}" && test -f "${PKGN}/${BINN}" \
    && ln -sf "${PKGN}/${BINN}" /usr/local/bin/nexusmind-mcp \
    && chmod +x "${PKGN}/${BINN}" \
    && npm cache clean --force

# Playwright: the CLI + MCP (so QA agents drive a browser via mcp__playwright__*)
# AND the ~1 GB chromium / chrome-for-testing downloads. The whole stack — npm
# packages, symlink, and browsers — is gated on INSTALL_BROWSERS: without a
# browser the Playwright MCP has nothing to launch, so on the slim u2s image
# (INSTALL_BROWSERS=false) none of it is installed. Default true keeps
# oracle/main's browser-driven QA agents working.
RUN if [ "$INSTALL_BROWSERS" = "true" ]; then \
      npm install -g playwright@latest @playwright/mcp@latest \
      && PKG="$(npm root -g)/@playwright/mcp" \
      && BIN="$(node -e "const b=require('${PKG}/package.json').bin; process.stdout.write(typeof b==='string'?b:(Object.values(b)[0]||''))")" \
      && test -n "${BIN}" && test -f "${PKG}/${BIN}" \
      && ln -sf "${PKG}/${BIN}" /usr/local/bin/mcp-server-playwright \
      && chmod +x "${PKG}/${BIN}" \
      && playwright install --with-deps chromium \
      && mcp-server-playwright install-browser chrome-for-testing \
      && chmod -R a+rX /ms-playwright \
      && ls -la /ms-playwright/ \
      && npm cache clean --force \
      && rm -rf /var/lib/apt/lists/* ; \
    else \
      echo "INSTALL_BROWSERS=false — skipping Playwright npm + chromium + chrome-for-testing (slim u2s image)" ; \
    fi

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
# Sandbox egress proxy (factory F1): runs in the nexusmind-sandbox namespace.
COPY --from=builder /app/target/release/factory-egress-proxy /app/factory-egress-proxy
# Golden task replay through the verification gate (run inside the worker):
#   factory-golden-replay <org_id> < tasks.jsonl
COPY --from=builder /app/target/release/factory-golden-replay /app/factory-golden-replay
# Jev shadow decisions over real PRs (factory F3): backfill, refresh, report.
#   factory-shadow <backfill|refresh|report> <org_id> ...
COPY --from=builder /app/target/release/factory-shadow /app/factory-shadow
# F4 frozen specialist eval (docs/factory/specialist-eval.md), run in the worker:
#   factory-specialist-eval <org_id> < tasks.jsonl
COPY --from=builder /app/target/release/factory-specialist-eval /app/factory-specialist-eval
COPY --from=nexus-builder /app/target/release/nexus /app/nexus
# The SDD importer has to ship with the image, or it cannot be run where it is
# actually needed. It is a one-shot binary, invoked manually:
#   import-sdd --db /data/nexusmind.db --org-id <id> --root <checkout> [--dry-run]
# Note it reads the openspec/ tree from --root, so that tree must be reachable
# (bind-mounted, or a checkout inside the container). See docs/RUNNING.md.
COPY --from=builder /app/target/release/import-sdd /app/import-sdd
# Seeder, for bringing a fresh volume up with demo data:
#   nexusmind-seed /data/nexusmind.db
COPY --from=builder /app/target/release/nexusmind-seed /app/nexusmind-seed
# Embedding backfill. Memories are vectorised as they are stored, so this only
# matters for the ones that were not: a database written while
# NEXUSMIND_EMBED_ENABLED was off has no vectors, and semantic search silently
# degrades to keyword for all of it. Ships here because the box that needs it is
# the box that has the database:
#   backfill-embeddings --db-path /data/nexusmind.db
# Safe to re-run; it skips memories that already have an embedding.
COPY --from=builder /app/target/release/backfill-embeddings /app/backfill-embeddings

# Fail the build, not the deploy, if any binary needs a library or glibc version
# the runtime does not have.
RUN for bin in nexusmind autonomous_worker factory-egress-proxy factory-golden-replay factory-shadow factory-specialist-eval nexus import-sdd nexusmind-seed backfill-embeddings; do \
      if ldd "/app/$bin" 2>&1 | grep -q "not found"; then \
        ldd "/app/$bin"; echo "incompatible binary: $bin"; exit 1; \
      fi; \
    done

RUN mkdir -p /data /claude-home

EXPOSE 8080
CMD ["/app/nexusmind"]
