#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR/.."
BACKEND_DIR="$REPO_DIR/apps/backend"
DB_PATH="$BACKEND_DIR/data/nexusmind.db"

echo "Building seed binary..."
cd "$BACKEND_DIR"
cargo build --release --bin nexusmind-seed 2>&1

echo "Resetting demo data..."
mkdir -p "$BACKEND_DIR/data"
./target/release/nexusmind-seed "$DB_PATH"

echo ""
echo "Start the server with:"
echo "  cd $BACKEND_DIR && cargo run"
echo "  Open http://localhost:8080/v1/health"
