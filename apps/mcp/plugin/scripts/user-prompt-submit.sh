#!/usr/bin/env bash
# user-prompt-submit.sh — NexusMind Claude Code plugin: UserPromptSubmit hook
# Outputs a systemMessage to reinforce memory tool usage on first prompt and periodically.
set -euo pipefail

# ---------------------------------------------------------------------------
# Parse stdin JSON
# ---------------------------------------------------------------------------
INPUT="$(cat)"

session_id="$(echo "$INPUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('session_id','unknown'))" 2>/dev/null || echo "unknown")"

# ---------------------------------------------------------------------------
# Guard: nothing to do without an API key
# ---------------------------------------------------------------------------
if [[ -z "${NEXUSMIND_API_KEY:-}" ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# State file for this session
# ---------------------------------------------------------------------------
STATE_FILE="/tmp/nexusmind-session-${session_id}"
NOW="$(date +%s)"

# ---------------------------------------------------------------------------
# First call for this session → output initial system message
# ---------------------------------------------------------------------------
if [[ ! -f "$STATE_FILE" ]]; then
  # Create state file with: session_start_time last_reminder_time prompt_count
  echo "${NOW} ${NOW} 1" > "$STATE_FILE"
  cat <<'JSON'
{"systemMessage": "NexusMind memory tools are available. Use store_memory to save decisions, bugs, and discoveries proactively. Use search_memory to look up past context."}
JSON
  exit 0
fi

# ---------------------------------------------------------------------------
# Read existing state
# ---------------------------------------------------------------------------
read -r SESSION_START LAST_REMINDER PROMPT_COUNT < "$STATE_FILE" 2>/dev/null || {
  SESSION_START="$NOW"
  LAST_REMINDER="$NOW"
  PROMPT_COUNT=0
}

PROMPT_COUNT=$(( PROMPT_COUNT + 1 ))

SESSION_AGE=$(( NOW - SESSION_START ))      # seconds since session start
TIME_SINCE_REMINDER=$(( NOW - LAST_REMINDER ))  # seconds since last reminder

# Update state
echo "${SESSION_START} ${LAST_REMINDER} ${PROMPT_COUNT}" > "$STATE_FILE"

# ---------------------------------------------------------------------------
# Periodic reminder: 15+ min since last reminder AND session is 5+ min old
# ---------------------------------------------------------------------------
FIFTEEN_MIN=900
FIVE_MIN=300

if (( TIME_SINCE_REMINDER >= FIFTEEN_MIN && SESSION_AGE >= FIVE_MIN )); then
  # Update last reminder timestamp
  echo "${SESSION_START} ${NOW} ${PROMPT_COUNT}" > "$STATE_FILE"
  cat <<'JSON'
{"systemMessage": "MEMORY REMINDER: Save recent decisions and discoveries to NexusMind with store_memory."}
JSON
  exit 0
fi

# ---------------------------------------------------------------------------
# Nothing to output this time
# ---------------------------------------------------------------------------
exit 0
