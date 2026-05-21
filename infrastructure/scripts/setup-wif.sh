#!/usr/bin/env bash
# Sets up Workload Identity Federation for GitHub Actions.
# No SA keys are created — GitHub gets short-lived OIDC tokens instead.
#
# Usage: ./setup-wif.sh <github-owner/repo>
# Example: ./setup-wif.sh smart-coder-labs/nexus-mind

set -euo pipefail

GITHUB_REPO="${1:?Usage: setup-wif.sh <owner/repo>  e.g. smart-coder-labs/nexus-mind}"
PROJECT="${2:-$(gcloud config get-value project 2>/dev/null)}"

if [[ -z "$PROJECT" ]]; then
  echo "ERROR: no GCP project set. Run: gcloud config set project YOUR_PROJECT_ID" >&2
  exit 1
fi

SA_NAME="nexusmind-ci"
SA_EMAIL="$SA_NAME@$PROJECT.iam.gserviceaccount.com"
POOL_ID="github-actions"
PROVIDER_ID="github"
PROJECT_NUMBER=$(gcloud projects describe "$PROJECT" --format="value(projectNumber)")

echo "==> Project        : $PROJECT ($PROJECT_NUMBER)"
echo "==> SA             : $SA_EMAIL"
echo "==> GitHub repo    : $GITHUB_REPO"
echo ""

# ── Service account ────────────────────────────────────────────────────────────

if gcloud iam service-accounts describe "$SA_EMAIL" --project "$PROJECT" &>/dev/null; then
  echo "==> Service account already exists, skipping"
else
  gcloud iam service-accounts create "$SA_NAME" \
    --display-name "NexusMind CI" \
    --project "$PROJECT"
  echo "==> Service account created — waiting for propagation..."
  for i in $(seq 1 20); do
    gcloud iam service-accounts describe "$SA_EMAIL" --project "$PROJECT" &>/dev/null && break
    sleep 3
  done
fi

# ── IAM roles ─────────────────────────────────────────────────────────────────

for role in \
  roles/artifactregistry.writer \
  roles/compute.osLogin \
  roles/firebase.admin; do
  echo "==> Granting $role"
  gcloud projects add-iam-policy-binding "$PROJECT" \
    --member "serviceAccount:$SA_EMAIL" \
    --role "$role" \
    --quiet
done

# ── Workload Identity Pool ─────────────────────────────────────────────────────

if gcloud iam workload-identity-pools describe "$POOL_ID" \
     --location global --project "$PROJECT" &>/dev/null; then
  echo "==> Pool '$POOL_ID' already exists, skipping"
else
  gcloud iam workload-identity-pools create "$POOL_ID" \
    --location global \
    --display-name "GitHub Actions" \
    --project "$PROJECT"
  echo "==> Pool created"
fi

# ── OIDC Provider ──────────────────────────────────────────────────────────────

if gcloud iam workload-identity-pools providers describe "$PROVIDER_ID" \
     --workload-identity-pool "$POOL_ID" \
     --location global --project "$PROJECT" &>/dev/null; then
  echo "==> Provider '$PROVIDER_ID' already exists, skipping"
else
  gcloud iam workload-identity-pools providers create-oidc "$PROVIDER_ID" \
    --workload-identity-pool "$POOL_ID" \
    --location global \
    --issuer-uri "https://token.actions.githubusercontent.com" \
    --attribute-mapping "google.subject=assertion.sub,attribute.repository=assertion.repository,attribute.actor=assertion.actor,attribute.ref=assertion.ref" \
    --attribute-condition "assertion.repository == '${GITHUB_REPO}'" \
    --project "$PROJECT"
  echo "==> OIDC provider created"
fi

# ── Bind SA to the pool (allow GitHub repo to impersonate the SA) ──────────────

MEMBER="principalSet://iam.googleapis.com/projects/${PROJECT_NUMBER}/locations/global/workloadIdentityPools/${POOL_ID}/attribute.repository/${GITHUB_REPO}"

gcloud iam service-accounts add-iam-policy-binding "$SA_EMAIL" \
  --role roles/iam.workloadIdentityUser \
  --member "$MEMBER" \
  --project "$PROJECT" \
  --quiet

echo "==> WIF binding created"

# ── Output secrets ─────────────────────────────────────────────────────────────

WIF_PROVIDER="projects/${PROJECT_NUMBER}/locations/global/workloadIdentityPools/${POOL_ID}/providers/${PROVIDER_ID}"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Add these secrets to GitHub → Settings → Secrets → Actions"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  GCP_PROJECT_ID          = $PROJECT"
echo "  GCP_WIF_PROVIDER        = $WIF_PROVIDER"
echo "  GCP_WIF_SA_EMAIL        = $SA_EMAIL"
echo "  GCE_SSH_PRIVATE_KEY     = cat ~/.ssh/nexusmind"
echo "  VITE_API_URL            = http://<backend-ip>:8080"
echo "  PUBLIC_SUPABASE_URL     = (opcional)"
echo "  PUBLIC_SUPABASE_ANON_KEY= (opcional)"
echo ""

# Copy WIF provider to clipboard on macOS
if command -v pbcopy &>/dev/null; then
  echo "$WIF_PROVIDER" | pbcopy
  echo "==> GCP_WIF_PROVIDER copied to clipboard"
fi

echo "==> Done"
