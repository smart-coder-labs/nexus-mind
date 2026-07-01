#!/usr/bin/env bash
# Injects recent NexusMind memories as a system-reminder at session start.
# Called by the Claude Code SessionStart hook.

BASE_URL="${NEXUSMIND_BASE_URL:-http://localhost:8080}"
API_KEY="${NEXUSMIND_API_KEY:-}"

if [[ -z "$API_KEY" ]]; then
  echo "## NexusMind — NOT CONNECTED (NEXUSMIND_API_KEY not set)"
  exit 0
fi

HEALTH=$(curl -sf "${BASE_URL}/v1/health" 2>/dev/null)
if [[ -z "$HEALTH" ]]; then
  echo "## NexusMind — NOT CONNECTED (backend unreachable at ${BASE_URL})"
  exit 0
fi

MEMORIES=$(curl -sf \
  -H "Authorization: Bearer ${API_KEY}" \
  "${BASE_URL}/v1/memory?limit=20" 2>/dev/null)

if [[ -z "$MEMORIES" ]]; then
  echo "## NexusMind — ACTIVE (no memories yet)"
  exit 0
fi

COUNT=$(echo "$MEMORIES" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "?")

echo "## NexusMind — ACTIVE PROTOCOL"
echo ""
echo "You are connected to NexusMind, the team memory layer for this project."
echo "This protocol is MANDATORY. Do NOT use engram or other memory tools for project context."
echo ""
echo "### Recent memories (last ${COUNT})"
echo ""
echo "$MEMORIES" | python3 -c "
import sys, json
memories = json.load(sys.stdin)
for m in memories[:10]:
    tool = m.get('tool', '?')
    project = m.get('project', '?')
    content = m.get('content', '')[:120].replace('\n', ' ')
    print(f'- [{tool}/{project}] {content}')
" 2>/dev/null

echo ""
echo "### SAVE IMMEDIATELY after any of these (no need to be asked):"
echo "- Architecture or design decision"
echo "- Bug fixed (include root cause)"
echo "- Convention established"
echo "- Non-obvious discovery or gotcha"
echo "- Tool/library choice made"
echo "- User confirms or rejects an approach"
echo ""
echo "### HOW TO SEARCH (important):"
echo "- To load a project's context: get_context(project='...') or list_memories(project='...')."
echo "- search_memory is SEMANTIC: pass a query describing what you actually need"
echo "  (e.g. 'how auth tokens are validated', 'deploy pipeline config')."
echo "- NEVER pass a bare project/repo name as the search_memory query — it returns noise,"
echo "  not that project's context. Use get_context/list_memories for that."
echo ""
echo "Use: store_memory(tool='claude-code', project='nexusmind', content='...')"
