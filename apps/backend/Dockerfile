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

RUN apt-get update && apt-get install -y ca-certificates git && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/nexusmind /app/nexusmind

RUN mkdir -p /data

EXPOSE 8080
CMD ["/app/nexusmind"]
