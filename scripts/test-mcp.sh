#!/usr/bin/env bash
# MCP smoke test — verifies the full store→search cycle via the REST API
# (same path the MCP server uses internally)
# Usage: NEXUSMIND_API_KEY=nm_demo_acme_sarah ./scripts/test-mcp.sh

set -euo pipefail

BASE_URL="${NEXUSMIND_BASE_URL:-http://localhost:8080}"
API_KEY="${NEXUSMIND_API_KEY:?Set NEXUSMIND_API_KEY before running this script}"

auth() { curl -sf -H "Authorization: Bearer $API_KEY" "$@"; }

echo "==> Checking backend health..."
health=$(auth "$BASE_URL/v1/health")
echo "$health" | grep -q '"status":"ok"' || { echo "FAIL: backend not healthy"; exit 1; }
echo "    OK"

echo "==> Storing a test memory..."
# Project must already exist: implicit project creation was removed in the
# project-membership hardening (see commit 38f0238). The seed pre-creates
# "nexusmind" for every org and enrolls all non-viewer users as members.
store=$(auth -X POST "$BASE_URL/v1/memory/store" \
  -H "Content-Type: application/json" \
  -d '{"content":"MCP smoke test memory","project":"nexusmind","tool":"test-script","tags":["smoke-test"]}')
id=$(echo "$store" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
echo "    Stored id: $id"

echo "==> Searching for the memory..."
results=$(auth -X POST "$BASE_URL/v1/memory/search" \
  -H "Content-Type: application/json" \
  -d '{"query":"MCP smoke test","limit":5}')
count=$(echo "$results" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))")
[ "$count" -ge 1 ] || { echo "FAIL: search returned 0 results"; exit 1; }
echo "    Found $count result(s)"

echo "==> Cleaning up..."
auth -X DELETE "$BASE_URL/v1/memory/$id" > /dev/null
echo "    Deleted $id"

echo ""
echo "All MCP smoke tests passed."
