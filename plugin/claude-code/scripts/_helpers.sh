#!/usr/bin/env bash
# _helpers.sh — shared helper functions for NexusMind Claude Code plugin scripts

# detect_project: determines the project name from git or directory context.
# Priority: git remote origin repo name → git root basename → cwd basename.
detect_project() {
  local project=""

  # 1. Try git remote origin URL → extract repo name
  if git rev-parse --is-inside-work-tree &>/dev/null 2>&1; then
    local remote_url
    remote_url="$(git remote get-url origin 2>/dev/null || true)"
    if [[ -n "$remote_url" ]]; then
      # Strip trailing .git, then extract last path component
      project="$(basename "$remote_url" .git)"
    fi

    # 2. Fallback: git root directory basename
    if [[ -z "$project" ]]; then
      project="$(basename "$(git rev-parse --show-toplevel 2>/dev/null || true)")"
    fi
  fi

  # 3. Final fallback: current working directory basename
  if [[ -z "$project" ]]; then
    project="$(basename "$PWD")"
  fi

  echo "$project"
}
