#!/usr/bin/env bash
# Wires the worker's OTel export to Phoenix (deploy/oracle/k8s/phoenix.yaml).
# Run ON the k3s box (e.g. `ssh oracle 'bash -s' < scripts/factory/phoenix_wire_worker.sh`).
# Creates a Phoenix system API key and stores the endpoint and the key in
# nexusmind-env, then restarts the backend so the worker picks them up.
# The key never reaches a terminal: it goes from Phoenix to a 0600 temp file to
# the secret, and the file is removed.
set -euo pipefail
umask 077
patch_file=$(mktemp)
trap 'rm -f "$patch_file"' EXIT
pod=$(sudo kubectl -n nexusmind-observability get pods -l app=phoenix -o name | head -1)
sudo kubectl -n nexusmind-observability exec "$pod" -- python3 -c '
import json, os, urllib.request, http.cookiejar
jar = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
def post(path, body):
    request = urllib.request.Request("http://127.0.0.1:6006" + path, data=json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json"})
    return opener.open(request)
post("/auth/login", {"email": "admin@localhost", "password": os.environ["PHOENIX_DEFAULT_ADMIN_INITIAL_PASSWORD"]})
mutation = {"query": "mutation($i:CreateApiKeyInput!){createSystemApiKey(input:$i){jwt}}",
            "variables": {"i": {"name": "nexusmind-worker", "description": "factory OTel spans"}}}
jwt = json.load(post("/graphql", mutation))["data"]["createSystemApiKey"]["jwt"]
print(json.dumps({"stringData": {
    "OTEL_EXPORTER_OTLP_ENDPOINT": "http://phoenix.nexusmind-observability.svc.cluster.local:6006",
    # OTLP header values are percent-encoded: %20 is the space after Bearer.
    "OTEL_EXPORTER_OTLP_HEADERS": "Authorization=Bearer%20" + jwt}}))
' > "$patch_file"
test -s "$patch_file"
sudo kubectl -n nexusmind patch secret nexusmind-env --type merge --patch-file "$patch_file" >/dev/null
echo "nexusmind-env updated (OTEL_EXPORTER_OTLP_ENDPOINT, OTEL_EXPORTER_OTLP_HEADERS)"
sudo kubectl -n nexusmind rollout restart deploy/nexusmind-backend
sudo kubectl -n nexusmind rollout status deploy/nexusmind-backend --timeout=240s
