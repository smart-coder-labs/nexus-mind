#!/usr/bin/env bash
set -euo pipefail

run_id=${1:?usage: status_specialist_eval.sh WORKFLOW_RUN_ID}
[[ "$run_id" =~ ^[0-9]+$ ]] || { echo 'invalid workflow run id' >&2; exit 2; }
remote_dir="/tmp/nexusmind-factory-eval-status-${run_id}"
trap 'rm -rf "$remote_dir"' EXIT

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
  --field-selector=status.phase=Running -o jsonpath='{.items[0].metadata.name}')
[[ -n "$pod" ]] || { echo 'no running NexusMind pod found' >&2; exit 1; }
kubectl_cmd -n "$namespace" exec -i "$pod" -c autonomous-worker -- \
  node - < "$remote_dir/status_specialist_eval.mjs"
