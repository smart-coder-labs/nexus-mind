# Infrastructure

NexusMind deployment configuration.

| Component | Platform | URL |
|-----------|----------|-----|
| Backend (Rust + SQLite) | Fly.io | `https://nexusmind-api.fly.dev` |
| Admin panel (React) | Cloudflare Pages | `https://nexusmind-admin.pages.dev` |
| Landing (Astro) | Cloudflare Pages | `https://nexusmind-landing.pages.dev` |
| MCP server | Local (stdio) | runs on each dev's machine |

---

## First-time setup

### 1. Prerequisites

```bash
brew install terraform flyctl
```

### 2. Fly.io — create account + login

```bash
fly auth signup   # or: fly auth login
fly tokens create deploy -x 999999h  # generate deploy token for CI
```

### 3. Cloudflare — create API token

1. Go to [dash.cloudflare.com/profile/api-tokens](https://dash.cloudflare.com/profile/api-tokens)
2. Create token → use "Edit Cloudflare Pages" template
3. Copy the token

### 4. Configure Terraform

```bash
cd infrastructure/terraform
cp terraform.tfvars.example terraform.tfvars
# Edit terraform.tfvars with your tokens
```

### 5. Apply infrastructure

```bash
terraform init
terraform plan    # review what will be created
terraform apply
```

This creates:
- Fly.io app + 1GB volume + shared IPv4 + IPv6
- Cloudflare Pages project for admin panel (auto-deploys from main)
- Cloudflare Pages project for landing (auto-deploys from main)

### 6. First backend deploy + seed

```bash
# Deploy the backend image to Fly
flyctl deploy --config infrastructure/fly.toml --remote-only

# Seed demo data on the live instance
flyctl ssh console --config infrastructure/fly.toml \
  --command "/app/nexusmind-seed /data/nexusmind.db"
```

### 7. Add CI secret

In GitHub → Settings → Secrets → Actions:

| Secret | Value |
|--------|-------|
| `FLY_API_TOKEN` | token from `fly tokens create deploy` |

Cloudflare Pages deploys automatically from git push — no CI secret needed (it reads from the Pages project GitHub connection).

---

## Ongoing deploys

| What | How |
|------|-----|
| Backend | Push to `main` → CI runs → `flyctl deploy` automatically |
| Admin panel | Push to `main` → Cloudflare Pages builds and deploys automatically |
| Landing | Push to `main` → Cloudflare Pages builds and deploys automatically |
| Demo data reset | `flyctl ssh console --config infrastructure/fly.toml --command "/app/nexusmind-seed /data/nexusmind.db"` |

---

## Architecture

```
GitHub push to main
       │
       ├── CI (GitHub Actions)
       │    ├── backend: build + test + clippy
       │    ├── admin:   npm build
       │    ├── mcp:     npm build
       │    ├── e2e:     smoke test
       │    └── deploy:  flyctl deploy → Fly.io
       │
       └── Cloudflare Pages (automatic)
            ├── admin build: cd apps/admin && npm ci && npm run build
            └── landing build: cd apps/landing && npm ci && npm run build
```

---

## Terraform state

State is stored locally by default (`terraform.tfstate`). For team use, configure a remote backend in `main.tf` (Terraform Cloud, S3, etc.).

**Never commit `terraform.tfstate` or `terraform.tfvars`** — both are gitignored.
