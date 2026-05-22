#!/usr/bin/env bash
# subagent-stop.sh — NexusMind Claude Code plugin: SubagentStop hook (async)
# Passively captures subagent output as a memory entry for team context.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_helpers.sh
source "${SCRIPT_DIR}/_helpers.sh"

# ---------------------------------------------------------------------------
# Guard: nothing to do without an API key
# ---------------------------------------------------------------------------
if [[ -z "${NEXUSMIND_API_KEY:-}" ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# Parse stdin JSON
# ---------------------------------------------------------------------------
INPUT="$(cat)"

subagent_output="$(echo "$INPUT" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('stdout', ''))
except Exception:
    pass
" 2>/dev/null || true)"

# Skip if output is empty or very short (not worth storing)
if [[ -z "$subagent_output" || "${#subagent_output}" -lt 50 ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
NEXUSMIND_BASE_URL="${NEXUSMIND_BASE_URL:-https://nexusmind-backend.fly.dev}"

# ---------------------------------------------------------------------------
# Detect project
# ---------------------------------------------------------------------------
PROJECT="$(detect_project)"

# ---------------------------------------------------------------------------
# Fire-and-forget: POST to /v1/memory/store
# ---------------------------------------------------------------------------
PAYLOAD="$(python3 -c "
import json, sys
content = sys.argv[1]
project = sys.argv[2]
# Truncate to avoid oversized payloads
if len(content) > 2000:
    content = content[:2000] + '... [truncated]'
print(json.dumps({
    'content': content,
    'tool': 'claude-code-subagent',
    'project': project,
    'metadata': {
        'source': 'subagent-stop-hook',
        'passive_capture': True
    }
}))
" "$subagent_output" "$PROJECT" 2>/dev/null || true)"

if [[ -n "$PAYLOAD" ]]; then
  curl -sf --max-time 10 \
    -X POST \
    -H "Authorization: Bearer ${NEXUSMIND_API_KEY}" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD" \
    "${NEXUSMIND_BASE_URL}/v1/memory/store" &>/dev/null || true
fi

exit 0
