#!/usr/bin/env bash
# One-time seed of the oracle box's SQLite volume with a backup pulled from Fly.
# This is how the existing "company brain" data (3.6k+ memories) lands on oracle.
# Run LOCALLY (from your Mac); it drives the box over SSH.
#
#   infrastructure/scripts/seed-oracle-db.sh <path-to-backup.db> [ssh-alias]
#
# Example:
#   infrastructure/scripts/seed-oracle-db.sh \
#     ../backups/nexusmind-20260817-200628/nexusmind-consolidated.db oracle
#
# Prerequisites: the stack has been deployed at least once (the compose volume
# `oracle-deploy_nexusmind_data` exists on the box). SAFETY: this OVERWRITES the
# box DB — it refuses to run unless the target volume currently holds no org data
# (empty DB), unless you pass FORCE=1.
set -euo pipefail

BACKUP="${1:?usage: seed-oracle-db.sh <backup.db> [ssh-alias]}"
SSH_ALIAS="${2:-oracle}"
REMOTE_DIR="~/oracle-deploy"
VOLUME="oracle-deploy_nexusmind_data"

[ -f "$BACKUP" ] || { echo "ERROR: backup not found: $BACKUP" >&2; exit 1; }

# Validate the backup locally before shipping it.
if command -v sqlite3 >/dev/null 2>&1; then
  echo "==> Verifying backup integrity locally..."
  ok=$(sqlite3 "$BACKUP" "PRAGMA integrity_check;" | head -1)
  [ "$ok" = "ok" ] || { echo "ERROR: integrity_check failed: $ok" >&2; exit 1; }
  echo "  integrity_check: ok  ($(sqlite3 "$BACKUP" 'SELECT count(*) FROM memories;') memories)"
fi

echo "==> Confirming compose volume exists on ${SSH_ALIAS}..."
ssh "$SSH_ALIAS" "docker volume inspect ${VOLUME} >/dev/null" \
  || { echo "ERROR: volume ${VOLUME} missing — deploy the stack once first." >&2; exit 1; }

# Guard: refuse to clobber a non-empty DB unless FORCE=1.
if [ "${FORCE:-0}" != "1" ]; then
  echo "==> Checking the box DB is empty (guard; set FORCE=1 to override)..."
  existing=$(ssh "$SSH_ALIAS" "docker run --rm -v ${VOLUME}:/data keinos/sqlite3 \
    sqlite3 /data/nexusmind.db 'SELECT count(*) FROM sqlite_master;' 2>/dev/null || echo 0")
  if [ "${existing:-0}" != "0" ]; then
    echo "ERROR: box DB already has ${existing} objects. Re-run with FORCE=1 to overwrite." >&2
    exit 1
  fi
fi

echo "==> Shipping backup to ${SSH_ALIAS}:/tmp/seed.db ..."
scp "$BACKUP" "${SSH_ALIAS}:/tmp/seed.db"

echo "==> Stopping backend, loading DB into volume, restarting..."
ssh "$SSH_ALIAS" bash -s <<REMOTE
set -euo pipefail
cd ${REMOTE_DIR}
docker compose stop backend
# Copy into the named volume and drop any stale WAL/SHM so SQLite opens clean.
docker run --rm -v ${VOLUME}:/data -v /tmp:/src alpine sh -c \
  'cp /src/seed.db /data/nexusmind.db && rm -f /data/nexusmind.db-wal /data/nexusmind.db-shm && chmod 644 /data/nexusmind.db'
rm -f /tmp/seed.db
docker compose start backend
REMOTE

echo
echo "Seed complete. Verify:"
echo "  ssh ${SSH_ALIAS} 'curl -fsS http://localhost:8080/v1/health'"
