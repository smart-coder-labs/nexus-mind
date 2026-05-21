#!/usr/bin/env bash
# Pulls the latest backend image and restarts the container on the GCE VM.
# Called by CI after pushing the image to Artifact Registry.
# Usage: ./deploy-backend.sh <image-url>

set -euo pipefail

IMAGE_URL="${1:?Usage: deploy-backend.sh <image-url>}"

echo "==> Pulling $IMAGE_URL"
docker pull "$IMAGE_URL"

echo "==> Restarting nexusmind-backend"
docker stop nexusmind-backend 2>/dev/null || true
docker rm   nexusmind-backend 2>/dev/null || true

docker run -d \
  --name nexusmind-backend \
  --restart always \
  -p 8080:8080 \
  -v /data:/data \
  -e DB_PATH=/data/nexusmind.db \
  -e RUST_LOG=info \
  "$IMAGE_URL"

echo "==> Waiting for health check..."
for i in $(seq 1 30); do
  curl -sf http://localhost:8080/v1/health && break
  sleep 1
done
curl -sf http://localhost:8080/v1/health || { echo "FAIL: backend not healthy after deploy"; exit 1; }

echo "==> Deploy complete"
