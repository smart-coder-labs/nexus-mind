#!/bin/zsh
# Retrieval eval for the factory F2 exit gate (Recall@K, Hit@K, MRR on questions
# derived from the golden tasks). Everything stays on this machine: the golden
# set (~/.nexusmind/evals/golden/v1), the snapshots and the eval databases live
# outside the repository.
#
# Usage: eval_retrieval.zsh [--dense] [eval_dir]
#   default: BM25 and ContextPack variants (lexical index, seconds)
#   --dense: also the embedding and RRF variants (embeds every chunk: hours on
#            a laptop CPU; resumable)
set -euo pipefail

DENSE=0
if [[ "${1:-}" == "--dense" ]]; then DENSE=1; shift; fi
EVAL_DIR="${1:-${EVAL_DIR:-$HOME/.nexusmind/evals/retrieval}}"
GOLDEN="${GOLDEN_DIR:-$HOME/.nexusmind/evals/golden/v1}"
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# repo name : local checkout with history : pinned snapshot commit : golden file
REPOS=(
  "nexus-mind:$REPO_ROOT:3de6eb345ad5b5bf294dc8aa016ba102d9c9be82:smart-coder-labs__nexus-mind"
  "kasymir-app-ui:${KASYMIR_REPO:-$REPO_ROOT/../../kasymir/kasymir-app-ui}:7e93b88353b8a5bffbec6922951c25406695b296:kasymir__kasymir-app-ui"
)

mkdir -p "$EVAL_DIR/snapshots"
(cd "$REPO_ROOT/apps/backend" && cargo build --release --bin factory-retrieval-eval)
BIN="$REPO_ROOT/apps/backend/target/release/factory-retrieval-eval"

for spec in "${REPOS[@]}"; do
  IFS=: read -r name checkout sha golden <<<"$spec"
  snapshot="$EVAL_DIR/snapshots/$name-${sha:0:12}"
  if [[ ! -d "$snapshot" ]]; then
    mkdir -p "$snapshot"
    git -C "$checkout" archive "$sha" | tar -x -C "$snapshot"
  fi
  if (( DENSE )); then
    db="$EVAL_DIR/dense-$name.db"; flags=(--variants dense,bm25,rrf,pack)
  else
    db="$EVAL_DIR/lexical-$name.db"; flags=(--no-embed --variants bm25,pack)
  fi
  (cd "$EVAL_DIR" && "$BIN" --db "$db" --repo "$snapshot" --history "$checkout" \
    --project "$name" "${flags[@]}" < "$GOLDEN/$golden.jsonl" > "$EVAL_DIR/$name.jsonl")
  grep '"summary"' "$EVAL_DIR/$name.jsonl"
done
