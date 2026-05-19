#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$SCRIPT_DIR/../apps/backend"

echo "Building seed binary..."
cd "$BACKEND_DIR"
cargo build --release --bin nexusmind-seed 2>&1

echo "Resetting demo data..."
mkdir -p ./data
./target/release/nexusmind-seed

echo ""
echo "Start the server with:"
echo "  cargo run --manifest-path $BACKEND_DIR/Cargo.toml"
echo "  Open http://localhost:8080/v1/health"
