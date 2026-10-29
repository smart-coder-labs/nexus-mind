#!/usr/bin/env bash
# One-time, idempotent bootstrap of the private Phoenix service and OTel worker
# credentials. Run only through the reviewed GitHub Actions dispatch on main.
set -euo pipefail

run_id=${1:?usage: bootstrap_phoenix.sh WORKFLOW_RUN_ID}
[[ "$run_id" =~ ^[0-9]+$ ]] || { echo 'invalid workflow run id' >&2; exit 2; }
temp_dir="/tmp/nexusmind-phoenix-${run_id}"
trap 'rm -rf "$temp_dir"' EXIT

if command -v kubectl >/dev/null 2>&1; then
  kubectl_cmd() { kubectl "$@"; }
elif command -v k3s >/dev/null 2>&1; then
  kubectl_cmd() { k3s kubectl "$@"; }
else
  echo 'kubectl is unavailable on the Oracle host' >&2
  exit 1
fi

namespace=nexusmind-observability
kubectl_cmd create namespace "$namespace" --dry-run=client -o yaml | kubectl_cmd apply -f -

if ! kubectl_cmd -n "$namespace" get secret phoenix >/dev/null 2>&1; then
  secret_file=$(mktemp)
  trap 'rm -f "$secret_file"; rm -rf "$temp_dir"' EXIT
  umask 077
  printf 'PHOENIX_SECRET=%s\nPHOENIX_DEFAULT_ADMIN_INITIAL_PASSWORD=%s\n' \
    "$(openssl rand -hex 32)" "$(openssl rand -base64 24 | tr -d '\n')" > "$secret_file"
  kubectl_cmd -n "$namespace" create secret generic phoenix \
    --from-env-file="$secret_file" --dry-run=client -o yaml | kubectl_cmd apply -f -
  rm -f "$secret_file"
  trap 'rm -rf "$temp_dir"' EXIT
fi

kubectl_cmd apply -f "$temp_dir/phoenix.yaml"
kubectl_cmd -n "$namespace" rollout status deploy/phoenix --timeout=360s

# Never replace a different exporter target silently. A second dispatch is a
# no-op once the worker already points at this Phoenix instance.
otel_state=$(kubectl_cmd -n nexusmind get secret nexusmind-env -o json | python3 -c '
import base64, json, sys
data = json.load(sys.stdin).get("data", {})
endpoint = base64.b64decode(data.get("OTEL_EXPORTER_OTLP_ENDPOINT", "")).decode()
headers = base64.b64decode(data.get("OTEL_EXPORTER_OTLP_HEADERS", "")).decode()
expected = "http://phoenix.nexusmind-observability.svc.cluster.local:6006"
if not endpoint and not headers:
    print("none")
elif endpoint == expected and headers.startswith("Authorization=Bearer%20"):
    print("phoenix")
else:
    print("other")
')

case "$otel_state" in
  phoenix)
    echo 'worker already points at Phoenix; leaving its API key unchanged'
    ;;
  none)
    bash "$temp_dir/phoenix_wire_worker.sh"
    ;;
  *)
    echo 'worker already has a different or partial OTel configuration; refusing to replace it' >&2
    exit 1
    ;;
esac

worker_pod=$(kubectl_cmd -n nexusmind get pods -l app=nexusmind-backend \
  --field-selector=status.phase=Running -o name | head -n 1)
[[ -n "$worker_pod" ]] || { echo 'no running NexusMind worker pod found' >&2; exit 1; }
kubectl_cmd -n nexusmind exec "$worker_pod" -c autonomous-worker -- node -e '
fetch("http://phoenix.nexusmind-observability.svc.cluster.local:6006/healthz")
  .then(response => {
    console.log(`worker reached Phoenix health endpoint (HTTP ${response.status})`)
    if (!response.ok) process.exitCode = 1
  })
  .catch(error => {
    console.error(`worker could not reach Phoenix: ${error.message}`)
    process.exitCode = 1
  })
'
