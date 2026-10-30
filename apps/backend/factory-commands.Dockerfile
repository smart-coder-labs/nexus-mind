# Verification pods inherit the backend's runtime tools and security posture,
# with Cargo added only to this image. The serving backend stays slim.
ARG BACKEND_IMAGE=ghcr.io/smart-coder-labs/nexusmind-backend:main-arm64
FROM rust:1.98-slim-bookworm AS rust-toolchain

FROM ${BACKEND_IMAGE}
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-toolchain /usr/local/cargo /usr/local/cargo
COPY --from=rust-toolchain /usr/local/rustup /usr/local/rustup

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/tmp/.cargo \
    CARGO_BUILD_JOBS=2 \
    RUST_TEST_THREADS=2 \
    PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
