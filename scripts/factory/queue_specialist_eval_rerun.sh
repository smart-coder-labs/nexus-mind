#!/usr/bin/env bash
# Queue new specialist-eval sets on the production PVC without changing the
# original frozen inputs. The deploy host already has kubectl and the
# autonomous-worker has /data/factory-evals plus the persistent runner.
set -euo pipefail

sets=${1:?usage: queue_specialist_eval_rerun.sh all|docs|tests|ui RUN_ID}
run_id=${2:?usage: queue_specialist_eval_rerun.sh all|docs|tests|ui RUN_ID}
[[ "$run_id" =~ ^[0-9]+$ ]] || { echo 'invalid workflow run id' >&2; exit 2; }
temp_dir="/tmp/nexusmind-eval-rerun-${run_id}"
trap 'rm -rf "$temp_dir"' EXIT

case "$sets" in
  all) set_ids=docs,tests,ui ;;
  docs|tests|ui) set_ids=$sets ;;
  *) echo 'sets must be all, docs, tests or ui' >&2; exit 2 ;;
esac

namespace=nexusmind
if command -v kubectl >/dev/null 2>&1; then
  kubectl_cmd() { kubectl "$@"; }
elif command -v k3s >/dev/null 2>&1; then
  kubectl_cmd() { k3s kubectl "$@"; }
else
  echo 'kubectl is unavailable on the Oracle host' >&2
  exit 1
fi

pod=$(kubectl_cmd -n "$namespace" get pods -l app=nexusmind-backend \
  --field-selector=status.phase=Running -o name | head -n 1)
[[ -n "$pod" ]] || { echo 'no running NexusMind pod found' >&2; exit 1; }

kubectl_cmd -n "$namespace" exec -i "$pod" -c autonomous-worker -- \
  env NM_EVAL_SET_IDS="$set_ids" NM_EVAL_RUN_ID="$run_id" node - \
  < "$temp_dir/queue_specialist_eval_rerun.mjs"

# The installed helper checks for another runner, STOP, run.mjs and pending
# sets. It starts the existing resumable runner only when those checks pass.
/usr/local/bin/nexusmind-ensure-eval-runner
