FROM ghcr.io/nvidia/openshell-community/sandboxes/base@sha256:aeef1c63f00e2913ea002ccb3aaf925f338b5c5d70e63576f0d95c16a138044e
USER root
ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright
RUN npm install --global @openai/codex@0.154.0 @anthropic-ai/claude-code@2.1.235 @smart-coder-labs/nexusmind-mcp@0.15.0 playwright@1.63.0 @playwright/mcp@0.0.82 \
    && PKGN="$(npm root -g)/@smart-coder-labs/nexusmind-mcp" \
    && BINN="$(node -e "const b=require('${PKGN}/package.json').bin; process.stdout.write(typeof b==='string'?b:(Object.values(b)[0]||''))")" \
    && test -n "$BINN" && ln -sf "${PKGN}/${BINN}" /usr/local/bin/nexusmind-mcp \
    && PKGP="$(npm root -g)/@playwright/mcp" \
    && BINP="$(node -e "const b=require('${PKGP}/package.json').bin; process.stdout.write(typeof b==='string'?b:(Object.values(b)[0]||''))")" \
    && test -n "$BINP" && ln -sf "${PKGP}/${BINP}" /usr/local/bin/mcp-server-playwright \
    && command -v claude && claude --version \
    && playwright install chromium
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
RUN curl -fsSL https://sh.rustup.rs -o /tmp/nexus-rustup-init.sh \
    && sh /tmp/nexus-rustup-init.sh -y --profile minimal --default-toolchain 1.98.1 --no-modify-path \
    && rm /tmp/nexus-rustup-init.sh
RUN printf '\n  cargo_registry:\n    name: cargo-registry\n    endpoints:\n      - { host: index.crates.io, port: 443 }\n      - { host: static.crates.io, port: 443 }\n      - { host: crates.io, port: 443 }\n    binaries:\n      - { path: /usr/local/cargo/bin/cargo }\n      - { path: "/usr/local/rustup/toolchains/**/bin/cargo" }\n  onnx_runtime_build:\n    name: onnx-runtime-build\n    endpoints:\n      - { host: parcel.pyke.io, port: 443, tls: skip }\n    binaries:\n      - { path: "/sandbox/**/target/debug/build/ort-sys-*/build-script-build" }\n' >> /etc/openshell/policy.yaml
ENV CARGO_HOME=/sandbox/.cargo
ENV PATH=/usr/local/cargo/bin:/sandbox/.venv/bin:/usr/local/bin:/usr/bin:/bin
RUN printf '\nexport CARGO_HOME=/sandbox/.cargo\nexport RUSTUP_HOME=/usr/local/rustup\nexport PATH=/usr/local/cargo/bin:$PATH\n' >> /sandbox/.bashrc
USER sandbox
