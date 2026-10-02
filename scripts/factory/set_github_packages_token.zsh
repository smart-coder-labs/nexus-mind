#!/bin/zsh
# Adds or rotates one org's read-only GitHub Packages token the egress proxy injects
# for private npm scopes, without touching the proxy's other secrets (rerunning
# configure_sandbox_secrets.zsh would rotate the signing key). Other orgs' entries
# are kept: the merge happens on the server, so their tokens never reach this Mac.
#
# Usage: set_github_packages_token.zsh <org_id> <@scope,@scope,...>
# The token comes from ~/.zshrc (export FACTORY_GHPKG_TOKEN=ghp_...) and travels
# only on stdin. It must be a classic PAT with read:packages only: fine-grained
# tokens cannot read GitHub Packages npm registries.
set -euo pipefail

source ~/.zshrc >/dev/null 2>&1 || true
TOKEN="${FACTORY_GHPKG_TOKEN:-}"
if [[ "$TOKEN" != ghp_* ]]; then
  echo "FACTORY_GHPKG_TOKEN in ~/.zshrc is missing or not a classic PAT (ghp_...). Aborting." >&2
  exit 1
fi
ORG_ID="${1:-}"
SCOPES="${2:-}"
if [[ ! "$ORG_ID" =~ ^[0-9a-f-]{36}$ || -z "$SCOPES" ]]; then
  echo "Usage: $0 <org_id> <@scope,@scope,...>" >&2
  exit 1
fi
export TOKEN ORG_ID SCOPES

# Runs on the server: reads the current map, replaces this org's entry, patches.
MERGE='
import base64, json, subprocess, sys
entry = json.load(sys.stdin)
kubectl = ["sudo", "kubectl", "-n", "nexusmind-egress"]
current = subprocess.run(kubectl + ["get", "secret", "factory-egress-proxy", "-o",
    "jsonpath={.data.FACTORY_GITHUB_PACKAGES_TOKENS}"], check=True, capture_output=True, text=True).stdout
tokens = json.loads(base64.b64decode(current)) if current else {}
tokens[entry["org_id"]] = {"token": entry["token"], "scopes": entry["scopes"]}
patch = json.dumps({"stringData": {"FACTORY_GITHUB_PACKAGES_TOKENS": json.dumps(tokens)}})
subprocess.run(kubectl + ["patch", "secret", "factory-egress-proxy", "--type", "merge",
    "--patch-file", "/dev/stdin"], input=patch, check=True, text=True, stdout=subprocess.DEVNULL)
print("orgs with a GitHub Packages token:", len(tokens))
'

python3 - <<'PY' | ssh oracle "python3 -c $(printf %q "$MERGE")"
import json, os, re, sys
scopes = sorted({s.strip().lower() for s in os.environ["SCOPES"].split(",") if s.strip()})
if not scopes or not all(re.fullmatch(r"@[a-z0-9][a-z0-9._-]*", s) for s in scopes):
    sys.exit("Scopes must look like @scope,@other.")
print(json.dumps({"org_id": os.environ["ORG_ID"], "token": os.environ["TOKEN"], "scopes": scopes}))
PY
ssh oracle 'sudo kubectl -n nexusmind-egress rollout restart deploy/factory-egress-proxy >/dev/null && sudo kubectl -n nexusmind-egress rollout status deploy/factory-egress-proxy --timeout=120s'
unset TOKEN
echo "Done: GitHub Packages token set for org $ORG_ID ($SCOPES); proxy restarted."
