# Deploy NexusMind on the oracle k3s cluster (Option A)

NexusMind runs as a **native k3s workload** behind the **existing Traefik**, so it
coexists with the `vend` platform on the same box. No separate Caddy, no host
80/443 grab. Traefik routes by Host header:
- `nexusmind.149.130.166.34.nip.io` — testing (Traefik self-signed cert)
- `api.nexusmind.smartcoderlabs.com` — production (DNS cutover target)

All commands run **on the box** (`ssh oracle`), where `ubuntu` has `kubectl`.
Manifests: `nexusmind.yaml` in this dir. Copy it to the box (or `git pull` there).

> The old docker-compose deploy is retired. Its CI jobs are paused (`if: false`)
> and its stack was `docker compose down`ed. The data volume is gone from the
> equation — we seed the new PVC from the verified backup instead.

## 0. Prerequisites (already true)
- Backend image built: `ghcr.io/smart-coder-labs/nexusmind-backend:main-arm64`.
- Box already did `docker login ghcr.io` (creds in `~/.docker/config.json`).
- Box still has the env at `~/oracle-deploy/.env` (from the docker deploy).
- Verified backup on your Mac:
  `backups/nexusmind-20260818-094103/nexusmind-consolidated.db` (3628 memories).

## 1. Namespace + secrets
```bash
ssh oracle 'kubectl create namespace nexusmind --dry-run=client -o yaml | kubectl apply -f -'

# imagePullSecret — reuse the existing docker login (no need to re-enter the PAT)
ssh oracle 'kubectl -n nexusmind create secret generic ghcr-pull \
  --type=kubernetes.io/dockerconfigjson \
  --from-file=.dockerconfigjson=$HOME/.docker/config.json \
  --dry-run=client -o yaml | kubectl apply -f -'

# env secret — source ~/oracle-deploy/.env (strips the quotes docker used) and
# re-emit only the backend keys, unquoted, into the k8s secret. Values are not
# printed to the terminal.
ssh oracle 'set -a; . ~/oracle-deploy/.env; set +a; \
  umask 077; : > /tmp/nm.env; \
  for k in DB_PATH RUST_LOG ADMIN_ORIGIN SUPERUSER_KEY NEXUSMIND_TOKEN_ENCRYPTION_KEY \
           GITHUB_CLIENT_ID GITHUB_CLIENT_SECRET GITHUB_REDIRECT_URI \
           SMTP_HOST SMTP_PORT SMTP_USERNAME SMTP_PASSWORD SMTP_FROM \
           APP_BASE_URL CORS_ORIGINS NEXUSMIND_EMBED_ENABLED \
           BACKUP_DATABASE_URL BACKUP_INTERVAL_HOURS; do \
    printf "%s=%s\n" "$k" "$(printenv "$k")" >> /tmp/nm.env; done; \
  kubectl -n nexusmind create secret generic nexusmind-env \
    --from-env-file=/tmp/nm.env --dry-run=client -o yaml | kubectl apply -f -; \
  rm -f /tmp/nm.env'
```

## 2. Apply the workload (starts with an EMPTY DB — expected)
```bash
# ship the manifest to the box (or `git pull` in a checkout there)
scp deploy/oracle/k8s/nexusmind.yaml oracle:/tmp/nexusmind.yaml
ssh oracle 'kubectl apply -f /tmp/nexusmind.yaml'
ssh oracle 'kubectl -n nexusmind rollout status deploy/nexusmind-backend --timeout=180s'
```

## 3. Seed the DB (manual restore — same as the docker flow)
Stop the pod so the RWO volume is free, load the backup, start again.
```bash
# 3a. copy the backup to the box
scp backups/nexusmind-20260818-094103/nexusmind-consolidated.db oracle:/tmp/seed.db

# 3b. scale to 0, seed via a throwaway pod that mounts the same PVC, scale back
ssh oracle 'kubectl -n nexusmind scale deploy/nexusmind-backend --replicas=0'
ssh oracle 'kubectl -n nexusmind delete pod nexusmind-seed --ignore-not-found; \
  kubectl -n nexusmind run nexusmind-seed --image=alpine:latest --restart=Never \
    --overrides="{\"spec\":{\"containers\":[{\"name\":\"seed\",\"image\":\"alpine:latest\",\"command\":[\"sleep\",\"600\"],\"volumeMounts\":[{\"name\":\"data\",\"mountPath\":\"/data\"}]}],\"volumes\":[{\"name\":\"data\",\"persistentVolumeClaim\":{\"claimName\":\"nexusmind-data\"}}]}}" \
    --command -- sleep 600; \
  kubectl -n nexusmind wait --for=condition=Ready pod/nexusmind-seed --timeout=60s'
ssh oracle 'kubectl -n nexusmind cp /tmp/seed.db nexusmind-seed:/data/nexusmind.db; \
  kubectl -n nexusmind exec nexusmind-seed -- sh -c "rm -f /data/nexusmind.db-wal /data/nexusmind.db-shm; ls -la /data/nexusmind.db"; \
  kubectl -n nexusmind delete pod nexusmind-seed; \
  rm -f /tmp/seed.db'
ssh oracle 'kubectl -n nexusmind scale deploy/nexusmind-backend --replicas=1; \
  kubectl -n nexusmind rollout status deploy/nexusmind-backend --timeout=180s'
```

## 4. Validate
```bash
# healthy pod
ssh oracle 'kubectl -n nexusmind get pods'
# data present (read-only count from the pod's volume)
ssh oracle 'kubectl -n nexusmind exec deploy/nexusmind-backend -- sh -c "echo ok"'  # pod up
# via Traefik on the nip.io host (self-signed cert, so -k)
curl -k -m 15 --resolve nexusmind.149.130.166.34.nip.io:443:149.130.166.34 \
  https://nexusmind.149.130.166.34.nip.io/v1/health
# and confirm vend still fine
curl -k -m 10 -o /dev/null -w "vend backoffice=%{http_code}\n" \
  -H "Host: backoffice.149.130.166.34.nip.io" https://149.130.166.34/
```
Preview the admin against this backend from your Mac by adding to `/etc/hosts`:
`149.130.166.34  api.nexusmind.smartcoderlabs.com` then open the admin.

## 5. Cutover (later)
1. **Real TLS for `api.nexusmind.smartcoderlabs.com`**: the ingress currently uses
   Traefik's self-signed cert. Before cutover, provision a real cert — either
   install cert-manager with a Cloudflare DNS-01 ClusterIssuer, or drop in a TLS
   secret and reference it in the ingress `spec.tls`.
2. **Do a final fresh restore** (repeat step 3 with a new backup) to capture any
   writes Fly took since the seed.
3. Point Cloudflare `api.nexusmind.smartcoderlabs.com` → `149.130.166.34`.
4. Retire the Fly app.

## Rollback
`kubectl -n nexusmind delete -f nexusmind.yaml` removes everything (the PVC too if
listed). Fly stays authoritative until the DNS cutover, so there is always a way back.
