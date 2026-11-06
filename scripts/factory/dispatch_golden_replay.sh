#!/usr/bin/env bash
# Dispatch the frozen F1 replay through the production GitHub workflow. Only
# the sanitized replay manifest is sent; task text and accepted answers stay
# outside the repository and the Actions event.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DATASET_DIR="${GOLDEN_DIR:-}"
ORG_ID="${1:-bdf4b2aa-3f52-422e-8bee-a1babbb7d337}"

if [[ -z "$DATASET_DIR" ]]; then
  DATASET_DIR="$(python3 -c 'from pathlib import Path; print(Path.home() / ".nexusmind/evals/golden/v1")')"
fi
if [[ ! "$ORG_ID" =~ ^[0-9a-fA-F-]{36}$ ]]; then
  echo "ORG_ID must be a UUID." >&2
  exit 1
fi
if [[ ! -d "$DATASET_DIR" ]]; then
  echo "Golden dataset directory not found: $DATASET_DIR" >&2
  exit 1
fi

found=0
while IFS= read -r -d '' dataset_file; do
  found=1
  python3 "$SCRIPT_DIR/golden_replay_dispatch_payload.py" "$dataset_file" "$ORG_ID" |
    gh workflow run factory-golden-replay.yml --repo smart-coder-labs/nexus-mind --ref main --json
done < <(find "$DATASET_DIR" -maxdepth 1 -type f -name '*.jsonl' -print0 | sort -z)

if [[ "$found" -eq 0 ]]; then
  echo "No golden JSONL files found in $DATASET_DIR" >&2
  exit 1
fi
