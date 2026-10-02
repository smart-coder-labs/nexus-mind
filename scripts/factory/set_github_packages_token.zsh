#!/bin/zsh
# Adds or rotates the read-only GitHub Packages token the egress proxy injects for
# private npm scopes, without touching the proxy's other secrets (rerunning
# configure_sandbox_secrets.zsh would rotate the signing key). The token comes from
# ~/.zshrc (export FACTORY_GHPKG_TOKEN=ghp_...) and travels only on stdin.
set -euo pipefail

source ~/.zshrc >/dev/null 2>&1 || true
TOKEN="${FACTORY_GHPKG_TOKEN:-}"
if [[ "$TOKEN" != ghp_* && "$TOKEN" != github_pat_* ]]; then
  echo "FACTORY_GHPKG_TOKEN in ~/.zshrc is missing or not a GitHub token. Aborting." >&2
  exit 1
fi
ORG_ID="${1:-bdf4b2aa-3f52-422e-8bee-a1babbb7d337}"   # default: SmartCoderLabs
export TOKEN ORG_ID

python3 - <<'PY' | ssh oracle 'sudo kubectl -n nexusmind-egress patch secret factory-egress-proxy --type merge --patch-file /dev/stdin'
import json, os
tokens = {os.environ["ORG_ID"]: os.environ["TOKEN"]}
print(json.dumps({"stringData": {"FACTORY_GITHUB_PACKAGES_TOKENS": json.dumps(tokens)}}))
PY
ssh oracle 'sudo kubectl -n nexusmind-egress rollout restart deploy/factory-egress-proxy >/dev/null && sudo kubectl -n nexusmind-egress rollout status deploy/factory-egress-proxy --timeout=120s'
unset TOKEN
echo "Done: GitHub Packages token set for org $ORG_ID; proxy restarted."
