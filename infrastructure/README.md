# Infrastructure

NexusMind deployment on GCP — free tier.

| Component | Platform | URL |
|-----------|----------|-----|
| Backend (Rust + SQLite) | Compute Engine e2-micro | `http://<static-ip>:8080` |
| Admin panel (React) | Firebase Hosting | `https://<project>-admin.web.app` |
| Landing (Astro) | Firebase Hosting | `https://<project>-landing.web.app` |
| Docker images | Artifact Registry | `us-central1-docker.pkg.dev/<project>/nexusmind` |
| MCP server | Local (stdio) | runs on each dev's machine |

**Free tier coverage**:
- e2-micro in us-central1: always free
- 30GB HDD persistent disk: always free
- Static external IP (attached): free
- Artifact Registry: 0.5GB free
- Firebase Hosting: 10GB storage, 360MB/day: free

---

## First-time setup

### 1. Prerequisites

```bash
brew install terraform
gcloud components install beta
npm install -g firebase-tools
```

### 2. GCP — authenticate

```bash
gcloud auth login
gcloud config set project YOUR_PROJECT_ID
gcloud auth application-default login   # for Terraform
```

### 3. Generate SSH key for the VM

```bash
ssh-keygen -t ed25519 -f ~/.ssh/nexusmind -C "nexusmind-deploy"
```

### 4. Configure Terraform

```bash
cd infrastructure/terraform
cp terraform.tfvars.example terraform.tfvars
# Fill in: project_id, ssh_public_key (cat ~/.ssh/nexusmind.pub)
```

### 5. Apply infrastructure

```bash
terraform init
terraform plan
terraform apply
```

This creates:
- Artifact Registry repo `nexusmind`
- 10GB persistent HDD disk
- Static external IP
- Firewall rules (port 8080 + SSH)
- e2-micro VM (with startup script that installs Docker + runs the container)
- Firebase Hosting sites for admin and landing

### 6. Copy deploy script to VM

```bash
BACKEND_IP=$(terraform output -raw backend_ip)
ssh -i ~/.ssh/nexusmind nexusmind@$BACKEND_IP "sudo mkdir -p /opt/nexusmind"
scp -i ~/.ssh/nexusmind infrastructure/scripts/deploy-backend.sh \
  nexusmind@$BACKEND_IP:/opt/nexusmind/deploy-backend.sh
ssh -i ~/.ssh/nexusmind nexusmind@$BACKEND_IP "sudo chmod +x /opt/nexusmind/deploy-backend.sh"
```

### 7. First backend deploy

```bash
REGION=us-central1
PROJECT=$(gcloud config get-value project)
IMAGE="$REGION-docker.pkg.dev/$PROJECT/nexusmind/backend"

gcloud auth configure-docker $REGION-docker.pkg.dev
docker build -t $IMAGE:latest apps/backend/
docker push $IMAGE:latest

BACKEND_IP=$(cd infrastructure/terraform && terraform output -raw backend_ip)
ssh -i ~/.ssh/nexusmind nexusmind@$BACKEND_IP \
  "bash /opt/nexusmind/deploy-backend.sh $IMAGE:latest"
```

### 8. Seed demo data on the VM

```bash
BACKEND_IP=$(cd infrastructure/terraform && terraform output -raw backend_ip)

# Copy seed binary + run it
scp -i ~/.ssh/nexusmind \
  apps/backend/target/release/nexusmind-seed \
  nexusmind@$BACKEND_IP:/tmp/

ssh -i ~/.ssh/nexusmind nexusmind@$BACKEND_IP \
  "/tmp/nexusmind-seed /data/nexusmind.db"
```

### 9. Deploy static sites

```bash
# Update firebase.json — replace PROJECT_ID with your actual project ID
sed -i "s/PROJECT_ID/$(gcloud config get-value project)/g" firebase.json

firebase login
firebase use YOUR_PROJECT_ID

# Build + deploy admin
VITE_API_URL="http://$(cd infrastructure/terraform && terraform output -raw backend_ip):8080" \
  npm --prefix apps/admin run build
firebase deploy --only hosting:$(gcloud config get-value project)-admin

# Build + deploy landing
npm --prefix apps/landing run build
firebase deploy --only hosting:$(gcloud config get-value project)-landing
```

### 10. Add GitHub Actions secrets

In GitHub → Settings → Secrets → Actions:

| Secret | How to get it |
|--------|---------------|
| `GCP_PROJECT_ID` | `gcloud config get-value project` |
| `GCP_SA_KEY` | Create a service account key (see below) |
| `GCE_SSH_PRIVATE_KEY` | `cat ~/.ssh/nexusmind` |
| `VITE_API_URL` | `http://<backend-ip>:8080` |
| `PUBLIC_SUPABASE_URL` | Supabase dashboard (optional) |
| `PUBLIC_SUPABASE_ANON_KEY` | Supabase dashboard (optional) |

**Create service account key for CI:**

```bash
PROJECT=$(gcloud config get-value project)

gcloud iam service-accounts create nexusmind-ci \
  --display-name "NexusMind CI"

# Grant required roles
for role in \
  roles/artifactregistry.writer \
  roles/compute.osLogin \
  roles/firebase.admin; do
  gcloud projects add-iam-policy-binding $PROJECT \
    --member "serviceAccount:nexusmind-ci@$PROJECT.iam.gserviceaccount.com" \
    --role "$role"
done

gcloud iam service-accounts keys create /tmp/nexusmind-ci-key.json \
  --iam-account "nexusmind-ci@$PROJECT.iam.gserviceaccount.com"

# Copy the content of /tmp/nexusmind-ci-key.json into the GCP_SA_KEY secret
cat /tmp/nexusmind-ci-key.json
rm /tmp/nexusmind-ci-key.json  # clean up
```

---

## Ongoing deploys

After the first-time setup, everything is automatic:

| What | Trigger |
|------|---------|
| Backend | Push to `main` → CI builds image → pushes to AR → SSHs + restarts container |
| Admin panel | Push to `main` → CI builds → Firebase Hosting deploy |
| Landing | Push to `main` → CI builds → Firebase Hosting deploy |

---

## Architecture

```
GitHub push to main
       │
       └── CI (GitHub Actions)
            ├── backend: build + test + clippy
            ├── admin:   npm build
            ├── mcp:     npm build
            ├── e2e:     smoke test
            └── deploy (only on main):
                 ├── Build Docker image → Artifact Registry
                 ├── SSH → GCE VM → pull image + restart container
                 ├── Build admin → Firebase Hosting
                 └── Build landing → Firebase Hosting

GCE e2-micro (us-central1-a)
  ├── Docker container: nexusmind backend
  ├── Persistent disk /data: nexusmind.db (SQLite)
  └── Static IP: <backend-ip>
```
