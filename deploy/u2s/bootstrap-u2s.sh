#!/usr/bin/env bash
# Idempotent first-time bootstrap for the u2s self-host box.
#
# Creates EXACTLY ONE organization ("u2s") if the database has no orgs yet.
# Safe to re-run on every deploy: if any org already exists it does nothing,
# so we never seed demo data and never create a second org.
#
# Reads its config from the ./.env written by the deploy workflow (so no
# secrets are passed on the command line). Overridable via environment.
set -euo pipefail

cd "$(dirname "$0")"
if [ -f ./.env ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

: "${SUPERUSER_KEY:?SUPERUSER_KEY is required (set it in ./.env)}"
API="${API_URL:-http://localhost:8080}"
ORG_NAME="${U2S_ORG_NAME:-u2s}"
ORG_SLUG="${U2S_ORG_SLUG:-u2s}"
ADMIN_EMAIL="${U2S_ADMIN_EMAIL:-admin@u2s.local}"
ADMIN_NAME="${U2S_ADMIN_NAME:-U2S Admin}"
KEY_FILE="${KEY_FILE:-$PWD/u2s-admin-key.txt}"

# Keep the superuser key OFF the process argv (visible via `ps` on the box):
# put the Authorization header in a 0600 curl config file and pass it with -K.
AUTH_CONF="$(mktemp)"
trap 'rm -f "$AUTH_CONF"' EXIT
umask 077
printf 'header = "Authorization: Bearer %s"\n' "$SUPERUSER_KEY" > "$AUTH_CONF"

echo "Waiting for backend health at ${API} (embedding init can take ~35s)..."
for i in $(seq 1 60); do
  if curl -fsS "${API}/v1/health" >/dev/null 2>&1; then
    echo "Backend healthy."
    break
  fi
  if [ "$i" -eq 60 ]; then
    echo "ERROR: backend did not become healthy within ~180s" >&2
    exit 1
  fi
  sleep 3
done

orgs=$(curl -fsS -K "$AUTH_CONF" "${API}/v1/orgs")
if [ "${orgs}" != "[]" ]; then
  echo "An organization already exists — skipping bootstrap (idempotent)."
  exit 0
fi

echo "Empty database detected. Creating the single org '${ORG_NAME}'..."
resp=$(curl -fsS -X POST "${API}/v1/orgs" \
  -K "$AUTH_CONF" \
  -H "Content-Type: application/json" \
  -d "{\"org_name\":\"${ORG_NAME}\",\"org_slug\":\"${ORG_SLUG}\",\"admin_email\":\"${ADMIN_EMAIL}\",\"admin_name\":\"${ADMIN_NAME}\"}")

# Extract "api_key" without depending on jq (may not be installed on the box).
api_key=$(printf '%s' "$resp" | sed -n 's/.*"api_key"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
if [ -z "$api_key" ]; then
  echo "ERROR: org creation did not return an api_key. Raw response:" >&2
  echo "$resp" >&2
  exit 1
fi

umask 077
printf '%s\n' "$api_key" > "$KEY_FILE"
echo "Org '${ORG_NAME}' created."
echo "Admin API key written to ${KEY_FILE} (mode 600)."
echo "Retrieve it with:  cat ${KEY_FILE}"
