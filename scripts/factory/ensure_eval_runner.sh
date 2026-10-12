#!/bin/sh
# Keeps the unattended specialist eval runner (run_specialist_evals.mjs) alive
# in the autonomous-worker container. A deploy replaces the pod and kills the
# runner; the sets and results live on the persistent volume, so starting it
# again continues where it was. Run on the Oracle host from root's crontab:
#
#   */15 * * * * /usr/local/bin/nexusmind-ensure-eval-runner >/dev/null 2>&1
#
# It does nothing when the runner is already running, when every set has its
# `.done` marker, or when `<dir>/STOP` exists (create it to pause the evals;
# remove the cron line to retire this). Prints no credential.
set -eu

DIR=/data/factory-evals
ORG=bdf4b2aa-3f52-422e-8bee-a1babbb7d337
NS=nexusmind
KUBECTL="kubectl"
command -v kubectl >/dev/null 2>&1 || KUBECTL="k3s kubectl"

pod=$($KUBECTL -n "$NS" get pods -l app=nexusmind-backend \
  --field-selector=status.phase=Running -o name | head -1)
[ -n "$pod" ] || exit 0

$KUBECTL -n "$NS" exec "$pod" -c autonomous-worker -- sh -c '
  dir=$1; org=$2
  [ -f "$dir/run.mjs" ] || exit 0
  [ -e "$dir/STOP" ] && exit 0
  pending=0
  for set in "$dir"/*.jsonl; do
    case "$set" in *.results.jsonl) continue ;; esac
    [ -e "$set" ] || continue
    [ -e "${set%.jsonl}.done" ] || pending=1
  done
  [ "$pending" = 1 ] || exit 0
  # Only a process that IS node counts: this script'"'"'s own sh also has the
  # runner'"'"'s path in its command line.
  for p in /proc/[0-9]*; do
    case "$(tr "\0" " " < "$p/cmdline" 2>/dev/null)" in
      node\ */run.mjs\ --org*) exit 0 ;;
    esac
  done
  cd "$dir"
  nohup node "$dir/run.mjs" --org "$org" --dir "$dir" >> "$dir/runner.out" 2>&1 &
  echo "$(date -u +%FT%TZ) ensure: started the runner" >> "$dir/runner.log"
' sh "$DIR" "$ORG"
