#!/usr/bin/env bash
# Enables all GCP APIs required by NexusMind (infra + CI/CD + WIF).
# Usage: ./enable-apis.sh [project-id]

set -euo pipefail

PROJECT="${1:-$(gcloud config get-value project 2>/dev/null)}"

if [[ -z "$PROJECT" ]]; then
  echo "ERROR: no GCP project set. Run: gcloud config set project YOUR_PROJECT_ID" >&2
  exit 1
fi

echo "==> Enabling APIs on project: $PROJECT"

APIS=(
  compute.googleapis.com                # GCE VM
  artifactregistry.googleapis.com       # Docker image registry
  iam.googleapis.com                    # IAM management
  iamcredentials.googleapis.com         # WIF token exchange (required for OIDC)
  sts.googleapis.com                    # Security Token Service (required for WIF)
  firebase.googleapis.com               # Firebase project
  firebasehosting.googleapis.com        # Firebase Hosting sites
)

gcloud services enable "${APIS[@]}" --project "$PROJECT"

echo ""
echo "==> All APIs enabled. Propagation may take 1-2 minutes."
