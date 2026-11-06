#!/bin/zsh
# CodeRankEmbed retrieval eval: dense, BM25 and RRF variants on the golden
# questions, against eval databases whose chunk vectors coderank_embed_eval.py
# replaced. Only the retriever differs from eval_retrieval.zsh --dense.
#
# Usage: coderank_eval.zsh <text-variant: skeleton|raw>
#   env: EVAL_DIR (dense DBs, snapshots), CODERANK_DIR (eval-bin-coderank, runs/),
#        GOLDEN_DIR, NEXUS_HISTORY, KASYMIR_HISTORY
set -euo pipefail

TEXT="${1:-skeleton}"
E="${EVAL_DIR:-/Volumes/external/Documents/nexu-loop-agents/.evals/retrieval}"
C="${CODERANK_DIR:-/Volumes/external/Documents/nexu-loop-agents/.evals/coderank}"
GOLDEN="${GOLDEN_DIR:-$HOME/.nexusmind/evals/golden/v1}"
REPOS=(
  "nexus-mind:nexus-mind-3de6eb345ad5:${NEXUS_HISTORY:-/Volumes/external/Documents/nexu-loop-agents/nexusmind}:smart-coder-labs__nexus-mind"
  "kasymir-app-ui:kasymir-app-ui-7e93b88353b8:${KASYMIR_HISTORY:-/Volumes/external/Documents/kasymir/kasymir-app-ui}:kasymir__kasymir-app-ui"
)
for spec in "${REPOS[@]}"; do
  IFS=: read -r name snap hist golden <<<"$spec"
  out="$C/runs/$TEXT-$name.eval.jsonl"
  "$C/eval-bin-coderank" --db "$C/runs/$TEXT-$name.db" --repo "$E/snapshots/$snap" \
    --history "$hist" --project "$name" --query-vectors "$C/runs/$TEXT-$name.q.jsonl" \
    --variants dense,bm25,rrf,rrf:0.5,rrf:2.0 \
    < "$GOLDEN/$golden.jsonl" > "$out" 2> "${out%.jsonl}.err"
  grep '"summary"' "$out"
done
