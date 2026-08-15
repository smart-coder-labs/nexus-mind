# u2s self-host deploy (AWS t3.medium)

Manual, single-tenant deployment of NexusMind (backend + admin) to the `u2s`
AWS box. Always deploys `main`. Full stack, no external services: embeddings
(`nomic-embed-text-v1.5`), semantic/vector search, FTS and code indexing all run
in-process over a single SQLite file.

## What runs

| Component | Port | Notes |
|-----------|------|-------|
| backend   | 8080 | API + MCP base URL. `http://<box-ip>:8080` |
| admin     | 3000 | Panel. Proxies `/v1` to backend internally (IP-agnostic image) |

Data persists in the Docker volume `nexusmind_data` (`/data/nexusmind.db`).

## One-time box preparation

On the t3.medium (`ssh u2s-nexus`):

```bash
# 1. Docker Engine + compose plugin
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker "$USER"   # re-login after this

# 2. Swap — 4 GiB RAM is tight while indexing code; add 4 GiB swap as a cushion
sudo fallocate -l 4G /swapfile && sudo chmod 600 /swapfile
sudo mkswap /swapfile && sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

**AWS Security Group** — open inbound to your audience:
`22` (deploy SSH), `8080` (API/MCP), `3000` (admin).

**Elastic IP (strongly recommended):** without it the box's public IP changes on
every stop/start and breaks `U2S_SSH_HOST`. Allocate an EIP and associate it.

## GitHub secrets (all prefixed `U2S_`)

Create them in an **Environment** named `u2s-production`
(Settings → Environments → New environment), not at repo level — the `deploy`
job references `environment: u2s-production`. There, add protection rules:
**Required reviewers** (manual approval before the box is touched) and
**Deployment branches → `main` only**.

| Secret | Value |
|--------|-------|
| `U2S_SSH_HOST` | Box public IP / Elastic IP / DNS |
| `U2S_SSH_USER` | SSH user, e.g. `ubuntu` |
| `U2S_SSH_KEY` | Full contents of the private key (`nexusmind.pem`) |
| `U2S_GHCR_USERNAME` | GitHub username that owns the pull token below |
| `U2S_GHCR_TOKEN` | PAT (classic) with **`read:packages`** only — for `docker login` on the box |
| `U2S_SUPERUSER_KEY` | A strong secret you choose; the backend's superuser key (auth for org creation) |

The push to GHCR uses the workflow's built-in `GITHUB_TOKEN` (no PAT needed).
The box pull needs `U2S_GHCR_TOKEN` because `GITHUB_TOKEN` does not exist off-runner.

> **Org-owned package access (first deploy).** The repo owner is the
> `smart-coder-labs` org, so the first push creates the packages
> `ghcr.io/smart-coder-labs/nexusmind-{backend,admin}` as **private under the
> org**. The `U2S_GHCR_TOKEN` user must be granted read access or the box pull
> 403s. After the first `build-and-push` run: Org → Packages → each package →
> **Package settings → Manage Actions access / Manage access** → link the repo
> (inherit access) or add the token's user with Read. Re-run the workflow once
> access is set.

## Running it

GitHub → Actions → **Deploy U2S (self-host)** → Run workflow → type `deploy`.
(Optionally set the admin email/name — used only on the very first deploy.)

The workflow builds + pushes images, then over SSH: `docker compose pull && up -d`
and runs `bootstrap-u2s.sh`, which creates **exactly one** org `u2s` **only if the
database is empty** (idempotent — safe on every subsequent deploy).

## Retrieve the u2s admin API key (first deploy only)

```bash
ssh u2s-nexus 'cat ~/u2s-deploy/u2s-admin-key.txt'
```

Use it as the MCP `NEXUSMIND_API_KEY` and point clients at `http://<box-ip>:8080`.

## Caveats

- **Admin UI login needs a password.** Org creation issues a password-setup token
  but, with no SMTP configured, it is only written to the backend logs
  (`docker compose logs backend`). The MCP `api_key` above works regardless.
- **No TLS** — internal IP:port only, as chosen. Put it behind a reverse proxy
  with certs before exposing to the public internet.
- **`COOKIE_SECURE=false` — SECURITY DEBT, remove once TLS is in place.**
  Because the box is served over plain HTTP, the session cookie cannot carry the
  `Secure` attribute: browsers silently drop such a cookie on an insecure origin,
  which makes login return `200` and then bounce the user straight back to
  `/login`. The override in `docker-compose.yml` is what makes login work at all
  here. The price is that **the session token travels in cleartext** — anyone on
  the network path can capture it and impersonate the user, so keep the AWS
  Security Group restricted to known source IPs. See "Getting to TLS" below.

## Getting to TLS (removes the `COOKIE_SECURE` debt)

`COOKIE_SECURE=false` exists only because there is no certificate. To retire it:

1. Point a **domain** at the box (e.g. `nexus.u2s.dev` → the elastic IP).
   Let's Encrypt will not issue a certificate for a bare IP, so a DNS name is a
   hard prerequisite.
2. Put a reverse proxy with automatic certs in front (Caddy is the least work —
   it handles ACME issuance and renewal itself) terminating `:443` and proxying
   to the `admin` and `backend` containers.
3. Delete the `COOKIE_SECURE` line from `docker-compose.yml`. The backend
   defaults to `true`, so removing the override is the whole change.
4. Close `3000`/`8080` in the Security Group and expose only `443`.

Until step 3 lands, treat any session on this box as interceptable.
- **`main` scope** — Clients admin and usage metrics live on
  `customization/u2s-company-brain`, not `main`, so they are absent here by design.
