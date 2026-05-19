#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR/.."
BACKEND_DIR="$REPO_DIR/apps/backend"
DB_PATH="$BACKEND_DIR/data/nexusmind.db"

cd "$BACKEND_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  echo "Building release binaries..."
  cargo build --release 2>&1
fi

echo "Resetting demo data..."
mkdir -p "$BACKEND_DIR/data"
./target/release/nexusmind-seed "$DB_PATH"

echo ""
echo "Start the server with:"
echo "  cd $BACKEND_DIR && cargo run"
echo "  Open http://localhost:8080/v1/health"
