#!/usr/bin/env bash
# install.sh — NexusMind Claude Code plugin installer
# Sets up the MCP server entry and hooks in ~/.claude/settings.json
# and writes environment variables to shell rc files.
set -euo pipefail

PLUGIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_SETTINGS="${HOME}/.claude/settings.json"

# ---------------------------------------------------------------------------
# Colors
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
RESET='\033[0m'

info()    { echo -e "${BLUE}[nexusmind]${RESET} $*"; }
success() { echo -e "${GREEN}[nexusmind]${RESET} $*"; }
warn()    { echo -e "${YELLOW}[nexusmind] WARNING:${RESET} $*"; }
error()   { echo -e "${RED}[nexusmind] ERROR:${RESET} $*" >&2; }

# ---------------------------------------------------------------------------
# Dependency checks (warn, don't fail)
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}NexusMind — Claude Code Plugin Installer${RESET}"
echo "────────────────────────────────────────"
echo ""

if ! command -v node &>/dev/null; then
  warn "node is not installed. The MCP server requires Node.js 18+."
  warn "Install it from https://nodejs.org before using NexusMind."
fi

if ! command -v jq &>/dev/null; then
  warn "jq is not installed. JSON merging will use python3 as fallback."
fi

if ! command -v python3 &>/dev/null; then
  error "python3 is required for JSON merging but was not found."
  error "Install python3 and re-run this installer."
  exit 1
fi

# ---------------------------------------------------------------------------
# Collect configuration
# ---------------------------------------------------------------------------

# API key
if [[ -z "${NEXUSMIND_API_KEY:-}" ]]; then
  echo -n "Enter your NexusMind API key: "
  read -r NEXUSMIND_API_KEY
  if [[ -z "$NEXUSMIND_API_KEY" ]]; then
    warn "No API key provided. You can set it later with: export NEXUSMIND_API_KEY=<your-key>"
    NEXUSMIND_API_KEY=""
  fi
fi

# Base URL
DEFAULT_URL="https://nexusmind-backend.fly.dev"
if [[ -z "${NEXUSMIND_BASE_URL:-}" ]]; then
  echo -n "NexusMind backend URL [${DEFAULT_URL}]: "
  read -r NEXUSMIND_BASE_URL
  NEXUSMIND_BASE_URL="${NEXUSMIND_BASE_URL:-$DEFAULT_URL}"
fi

info "Using backend: ${NEXUSMIND_BASE_URL}"
echo ""

# ---------------------------------------------------------------------------
# Ensure ~/.claude directory and settings.json exist
# ---------------------------------------------------------------------------
mkdir -p "${HOME}/.claude"

if [[ ! -f "$CLAUDE_SETTINGS" ]]; then
  echo '{}' > "$CLAUDE_SETTINGS"
  info "Created ${CLAUDE_SETTINGS}"
fi

# ---------------------------------------------------------------------------
# Helper: merge JSON via python3
# ---------------------------------------------------------------------------
merge_json() {
  # merge_json <file> <python_snippet_that_modifies_variable_d>
  local file="$1"
  local snippet="$2"
  python3 -c "
import json, sys

with open('${file}', 'r') as f:
    d = json.load(f)

${snippet}

with open('${file}', 'w') as f:
    json.dump(d, f, indent=2)
    f.write('\n')
"
}

# ---------------------------------------------------------------------------
# 1. Write MCP server entry
# ---------------------------------------------------------------------------
info "Adding MCP server entry to ${CLAUDE_SETTINGS}..."

merge_json "$CLAUDE_SETTINGS" "
if 'mcpServers' not in d:
    d['mcpServers'] = {}
d['mcpServers']['nexusmind'] = {
    'command': 'npx',
    'args': ['-y', '@smart-coder-labs/nexusmind-mcp'],
    'env': {
        'NEXUSMIND_API_KEY': '\${NEXUSMIND_API_KEY}',
        'NEXUSMIND_BASE_URL': '\${NEXUSMIND_BASE_URL:-${NEXUSMIND_BASE_URL}}'
    }
}
"
success "MCP server entry added."

# ---------------------------------------------------------------------------
# 2. Read hooks from hooks.json and merge into settings.json
# ---------------------------------------------------------------------------
HOOKS_JSON="${PLUGIN_DIR}/hooks/hooks.json"

if [[ ! -f "$HOOKS_JSON" ]]; then
  warn "hooks.json not found at ${HOOKS_JSON}. Skipping hooks installation."
else
  info "Merging hooks into ${CLAUDE_SETTINGS}..."

  PLUGIN_ROOT_ESCAPED="$(echo "${PLUGIN_DIR}/scripts" | sed 's/[\/&]/\\&/g')"

  python3 -c "
import json

with open('${CLAUDE_SETTINGS}', 'r') as f:
    settings = json.load(f)

with open('${HOOKS_JSON}', 'r') as f:
    hooks_data = json.load(f)

plugin_root = '${PLUGIN_DIR}'

if 'hooks' not in settings:
    settings['hooks'] = {}

# Merge each hook event
for event, entries in hooks_data.get('hooks', {}).items():
    if event not in settings['hooks']:
        settings['hooks'][event] = []
    for entry in entries:
        # Replace \${CLAUDE_PLUGIN_ROOT}/scripts with actual path
        resolved_entry = json.loads(
            json.dumps(entry).replace('\${CLAUDE_PLUGIN_ROOT}', plugin_root)
        )
        # Avoid duplicates based on command
        existing_commands = [
            h.get('command', '')
            for group in settings['hooks'][event]
            for h in (group.get('hooks', []) if isinstance(group, dict) and 'hooks' in group else [group])
        ]
        new_command = resolved_entry.get('command', '') or (
            resolved_entry.get('hooks', [{}])[0].get('command', '')
            if isinstance(resolved_entry, dict) and 'hooks' in resolved_entry else ''
        )
        if new_command not in existing_commands:
            settings['hooks'][event].append(resolved_entry)

with open('${CLAUDE_SETTINGS}', 'w') as f:
    json.dump(settings, f, indent=2)
    f.write('\n')
"
  success "Hooks merged."
fi

# ---------------------------------------------------------------------------
# 3. Write env vars to shell rc files
# ---------------------------------------------------------------------------
write_env_var() {
  local rc_file="$1"
  local var_name="$2"
  local var_value="$3"

  if [[ ! -f "$rc_file" ]]; then
    return 0
  fi

  # Skip if already set in the file
  if grep -q "export ${var_name}=" "$rc_file" 2>/dev/null; then
    warn "${var_name} already set in ${rc_file} — skipping."
    return 0
  fi

  echo "" >> "$rc_file"
  echo "# NexusMind" >> "$rc_file"
  echo "export ${var_name}=\"${var_value}\"" >> "$rc_file"
  success "Wrote ${var_name} to ${rc_file}"
}

if [[ -n "$NEXUSMIND_API_KEY" ]]; then
  write_env_var "${HOME}/.bashrc" "NEXUSMIND_API_KEY" "$NEXUSMIND_API_KEY"
  write_env_var "${HOME}/.zshrc"  "NEXUSMIND_API_KEY" "$NEXUSMIND_API_KEY"
fi

write_env_var "${HOME}/.bashrc" "NEXUSMIND_BASE_URL" "$NEXUSMIND_BASE_URL"
write_env_var "${HOME}/.zshrc"  "NEXUSMIND_BASE_URL" "$NEXUSMIND_BASE_URL"

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}${GREEN}Installation complete!${RESET}"
echo ""
echo "Next steps:"
echo "  1. Restart your shell or run: source ~/.zshrc (or ~/.bashrc)"
echo "  2. Open Claude Code — NexusMind will connect automatically"
echo "  3. The MCP tools (store_memory, search_memory, list_memories) are now available"
echo ""
if [[ -z "${NEXUSMIND_API_KEY:-}" ]]; then
  echo -e "${YELLOW}Remember to set your API key:${RESET}"
  echo "  export NEXUSMIND_API_KEY=<your-key>"
  echo ""
fi
echo "Documentation: https://nexusmind.smartcoderlabs.com"
echo ""
