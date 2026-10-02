#!/bin/zsh
# Replays the local golden task set through the F1 verification gate in
# production. Only id, repository, merge_sha and changed_files leave this
# machine (never the task text or the answers' content). The replay runs in the
# worker container in the background; follow it with:
#   ssh oracle 'sudo kubectl -n nexusmind exec deploy/nexusmind-backend -c autonomous-worker -- cat /tmp/golden-replay.out'
set -euo pipefail

DATASET="${GOLDEN_DIR:-$HOME/.nexusmind/evals/golden/v1}"
ORG_ID="${1:-bdf4b2aa-3f52-422e-8bee-a1babbb7d337}"   # default: SmartCoderLabs

python3 - "$DATASET" <<'PY' | ssh oracle 'sudo kubectl -n nexusmind exec -i deploy/nexusmind-backend -c autonomous-worker -- sh -c "cat > /tmp/golden-tasks.jsonl"'
import glob, json, os, sys
for path in sorted(glob.glob(os.path.join(sys.argv[1], "*.jsonl"))):
    for line in open(path):
        if line.strip():
            task = json.loads(line)
            print(json.dumps({k: task[k] for k in ("id", "repository", "merge_sha", "changed_files")}))
PY
ssh oracle "sudo kubectl -n nexusmind exec deploy/nexusmind-backend -c autonomous-worker -- sh -c 'wc -l < /tmp/golden-tasks.jsonl; nohup /app/factory-golden-replay $ORG_ID < /tmp/golden-tasks.jsonl > /tmp/golden-replay.out 2>/tmp/golden-replay.err &'"
echo "Replay started for org $ORG_ID."
