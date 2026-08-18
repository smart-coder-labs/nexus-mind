# Deploy: oracle (Oracle Cloud Ampere A1, ARM64)

Self-host target for the NexusMind **backend**, running **in parallel with Fly**
during migration. Admin/landing/backoffice stay on Cloudflare Pages and keep
calling `https://api.nexusmind.smartcoderlabs.com`; only the backend moves here.

- Box: `agency-os-production`, Oracle Cloud Ampere A1, Ubuntu 22.04, **aarch64**.
- Stack: `backend` (Rust) behind `caddy` (auto-TLS via Cloudflare DNS-01).
- Images: built natively for `linux/arm64` on a `ubuntu-24.04-arm` hosted runner
  (no QEMU emulation), pushed to GHCR as `nexusmind-backend:main-arm64` (distinct
  from u2s's amd64 `:main`). The gha build cache speeds up repeat runs.
- CI: the `backend-oracle-*` jobs in `.github/workflows/deploy.yml` run on every
  push to `main`, alongside the existing Fly `backend` job.

## Order of operations (IMPORTANT)

**Deploy first with an empty DB, validate, then restore the data manually.**
Do not seed during deploy — the restore is a deliberate, separate step.

```
1. Prerequisites  ->  2. First deploy (empty DB)  ->  3. Validate TLS + health
                                                          |
                          5. DNS cutover  <-  4. MANUAL restore of Fly backup
```

## 1. Prerequisites (one-time)

### 1a. GitHub **Repository** secrets
Add these under Settings → Secrets and variables → Actions → *Repository secrets*
(no Environment is used — free-tier private repos can't attach protection rules
anyway). Values mirror the Fly deployment (`fly.toml [env]` + `fly secrets`); Fly
won't reveal secret values, so source them from your own store.

Only the box-access secrets keep the `ORACLE_` prefix; everything else is shared
app config. GitHub forbids secret names starting with `GITHUB_`, so the OAuth
secrets are named `GH_*` (the workflow maps them to the app's `GITHUB_*` vars).

| Secret | Notes |
|---|---|
| `ORACLE_SSH_HOST` | `149.130.166.34` |
| `ORACLE_SSH_USER` | `ubuntu` |
| `ORACLE_SSH_KEY` | private key body for the box |
| `GHCR_USERNAME` / `GHCR_TOKEN` | PAT with `read:packages` (box pulls the image) |
| `CLOUDFLARE_API_TOKEN` | Zone:DNS:Edit on `smartcoderlabs.com` (Caddy TLS). Distinct scope from the Pages `CF_API_TOKEN`. |
| `SUPERUSER_KEY` | backend superuser key |
| `NEXUSMIND_TOKEN_ENCRYPTION_KEY` | **must equal Fly's exactly** — see restore note |
| `ADMIN_ORIGIN` | `https://admin.nexusmind.smartcoderlabs.com` |
| `GH_CLIENT_ID` / `GH_CLIENT_SECRET` / `GH_REDIRECT_URI` | GitHub OAuth app |
| `SMTP_HOST` / `SMTP_PORT` / `SMTP_USERNAME` / `SMTP_PASSWORD` / `SMTP_FROM` | email |
| `APP_BASE_URL` / `CORS_ORIGINS` | URLs / CORS |
| `NEXUSMIND_EMBED_ENABLED` | embeddings toggle |
| `BACKUP_DATABASE_URL` / `BACKUP_INTERVAL_HOURS` | backup cadence |

See `env.template` for the shape of the `.env` the workflow writes on the box.

### 1b. Open the firewalls (BOTH are required)
1. **OCI console** → Networking → Security List: allow ingress **TCP 80 and 443**.
2. **Host iptables** (Oracle images REJECT by default):
   ```
   ssh oracle 'bash -s' < ../../infrastructure/scripts/bootstrap-oracle.sh
   ```

Caddy uses the Cloudflare **DNS-01** challenge, so it obtains the cert for
`api.nexusmind...` even while that record still points at Fly — no inbound-80
challenge needed. But 443 must be reachable for clients once you cut over.

## 2. First deploy (empty DB)

Push to `main` (or re-run the `Deploy` workflow). The `backend-oracle-build` job
compiles the arm64 image; `backend-oracle-deploy` ships the compose + Caddyfile,
writes `.env`, and runs `docker compose up -d`. The backend comes up with an
**empty** volume. That's expected — data comes in step 4.

## 3. Validate before touching data

```bash
# Backend healthy inside the box:
ssh oracle 'curl -fsS http://localhost:8080/v1/health'

# TLS + routing via Caddy, without moving DNS yet (cert exists via DNS-01):
curl -fsS --resolve api.nexusmind.smartcoderlabs.com:443:149.130.166.34 \
  https://api.nexusmind.smartcoderlabs.com/v1/health
```

## 4. Restore the Fly backup — MANUAL, after a good deploy

The backend must be **stopped** while the DB file is swapped. Two ways:

**Option A — helper script** (optional, has an anti-clobber guard):
```bash
infrastructure/scripts/seed-oracle-db.sh \
  ../../backups/nexusmind-YYYYMMDD-HHMMSS/nexusmind-consolidated.db oracle
```

**Option B — by hand:**
```bash
scp backups/.../nexusmind-consolidated.db oracle:/tmp/seed.db
ssh oracle '
  cd ~/oracle-deploy && docker compose stop backend
  docker run --rm -v oracle-deploy_nexusmind_data:/data -v /tmp:/src alpine sh -c \
    "cp /src/seed.db /data/nexusmind.db && rm -f /data/nexusmind.db-wal /data/nexusmind.db-shm"
  rm -f /tmp/seed.db
  docker compose start backend'
```

> **Encryption key:** the restored DB holds GitHub-connection tokens encrypted
> with Fly's `NEXUSMIND_TOKEN_ENCRYPTION_KEY`. If `ORACLE_NEXUSMIND_TOKEN_ENCRYPTION_KEY`
> differs, memories/text are fine but those encrypted tokens become unreadable.
> Keep the keys identical.

## 5. Cutover

When health + data check out, point Cloudflare DNS
`api.nexusmind.smartcoderlabs.com` → `149.130.166.34`. Once traffic is served
from oracle and verified, delete the Fly `backend` job from `deploy.yml` (and
retire the Fly app). Frontends need no change — the hostname is unchanged.

## Rollback

DNS still on Fly until step 5, so rolling back is just "don't cut over". After
cutover, point the record back to Fly; the Fly app and its volume remain intact
until you explicitly retire them.
