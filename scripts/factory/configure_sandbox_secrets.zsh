#!/bin/zsh
# Creates (or rotates) the factory egress proxy secret and gives the worker the same signing
# key. Secret values travel only on stdin to the server: never in argv, shell
# history or terminal output.
set -euo pipefail

source ~/.zshrc >/dev/null 2>&1 || true
: "${CLAUDE_FACTORY_OAUTH_TOKEN:?CLAUDE_FACTORY_OAUTH_TOKEN is not set in ~/.zshrc}"

ORG_ID="bdf4b2aa-3f52-422e-8bee-a1babbb7d337"   # SmartCoderLabs

# The bot key comes from ~/.zshrc (export FACTORY_BOT_KEY=nm_...), like the other
# factory secrets, so it never appears in a command line or a transcript.
BOT_KEY="${FACTORY_BOT_KEY:-}"
if [[ "$BOT_KEY" != nm_* ]]; then
  echo "FACTORY_BOT_KEY in ~/.zshrc is missing or not a NexusMind key (expected nm_...). Aborting." >&2
  exit 1
fi

SIGNING_KEY="$(openssl rand -hex 32)"
GHPKG_TOKEN="${FACTORY_GHPKG_TOKEN:-}"
export SIGNING_KEY BOT_KEY ORG_ID CLAUDE_FACTORY_OAUTH_TOKEN GHPKG_TOKEN

# 1. Proxy secret (nexusmind-sandbox).
python3 - <<'PY' | ssh oracle 'sudo kubectl apply -f -'
import json, os
print(json.dumps({
    "apiVersion": "v1",
    "kind": "Secret",
    "metadata": {"name": "factory-egress-proxy", "namespace": "nexusmind-egress"},
    "type": "Opaque",
    "stringData": {
        "FACTORY_PROXY_SIGNING_KEY": os.environ["SIGNING_KEY"],
        "FACTORY_ANTHROPIC_OAUTH_TOKEN": os.environ["CLAUDE_FACTORY_OAUTH_TOKEN"],
        "FACTORY_NEXUSMIND_KEYS": json.dumps({os.environ["ORG_ID"]: os.environ["BOT_KEY"]}),
        # Optional: read-only GitHub Packages token for private npm scopes.
        **({"FACTORY_GITHUB_PACKAGES_TOKENS": json.dumps({os.environ["ORG_ID"]: os.environ["GHPKG_TOKEN"]})}
           if os.environ.get("GHPKG_TOKEN") else {}),
    },
}))
PY

# 2. Same signing key for the worker (merged into nexusmind-env; other keys untouched).
python3 - <<'PY' | ssh oracle 'sudo kubectl -n nexusmind patch secret nexusmind-env --type merge --patch-file /dev/stdin'
import json, os
print(json.dumps({"stringData": {"FACTORY_PROXY_SIGNING_KEY": os.environ["SIGNING_KEY"]}}))
PY

unset SIGNING_KEY BOT_KEY GHPKG_TOKEN
echo "Done: factory-egress-proxy secret created and nexusmind-env patched."
