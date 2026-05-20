# ── Artifact Registry — Docker image store ────────────────────────────────────

resource "google_artifact_registry_repository" "backend" {
  location      = var.region
  repository_id = "nexusmind"
  format        = "DOCKER"
  description   = "NexusMind backend Docker images"
  depends_on    = [google_project_service.apis]
}

# ── Service account for the VM ────────────────────────────────────────────────

resource "google_service_account" "backend_vm" {
  account_id   = "nexusmind-backend-vm"
  display_name = "NexusMind Backend VM"
}

# Allow VM to pull images from Artifact Registry
resource "google_artifact_registry_repository_iam_member" "vm_reader" {
  location   = google_artifact_registry_repository.backend.location
  repository = google_artifact_registry_repository.backend.name
  role       = "roles/artifactregistry.reader"
  member     = "serviceAccount:${google_service_account.backend_vm.email}"
}

# ── Persistent disk for SQLite ────────────────────────────────────────────────

resource "google_compute_disk" "data" {
  name  = "nexusmind-data"
  type  = "pd-standard" # HDD — covered by free tier
  zone  = var.zone
  size  = var.data_disk_size_gb

  depends_on = [google_project_service.apis]
}

# ── Static external IP ────────────────────────────────────────────────────────

resource "google_compute_address" "backend" {
  name   = "nexusmind-backend-ip"
  region = var.region

  depends_on = [google_project_service.apis]
}

# ── Firewall rules ────────────────────────────────────────────────────────────

resource "google_compute_firewall" "allow_http" {
  name    = "nexusmind-allow-http"
  network = "default"

  allow {
    protocol = "tcp"
    ports    = ["8080"]
  }

  source_ranges = ["0.0.0.0/0"]
  target_tags   = ["nexusmind-backend"]
}

resource "google_compute_firewall" "allow_ssh" {
  name    = "nexusmind-allow-ssh"
  network = "default"

  allow {
    protocol = "tcp"
    ports    = ["22"]
  }

  source_ranges = ["0.0.0.0/0"]
  target_tags   = ["nexusmind-backend"]
}

# ── Compute Engine VM (e2-micro — always free in us-central1) ─────────────────

locals {
  image_url = "${var.region}-docker.pkg.dev/${var.project_id}/nexusmind/backend:latest"
}

resource "google_compute_instance" "backend" {
  name         = var.instance_name
  machine_type = "e2-micro" # always free tier
  zone         = var.zone

  tags = ["nexusmind-backend"]

  boot_disk {
    initialize_params {
      image = "debian-cloud/debian-12"
      size  = 10 # GB — boot disk, not data
      type  = "pd-standard"
    }
  }

  attached_disk {
    source      = google_compute_disk.data.id
    device_name = "nexusmind-data"
  }

  network_interface {
    network = "default"
    access_config {
      nat_ip = google_compute_address.backend.address
    }
  }

  service_account {
    email  = google_service_account.backend_vm.email
    scopes = ["cloud-platform"]
  }

  metadata = {
    ssh-keys = "${var.ssh_user}:${var.ssh_public_key}"

    startup-script = <<-EOT
      #!/bin/bash
      set -e

      # Mount persistent data disk (only format on first boot)
      DISK=/dev/disk/by-id/google-nexusmind-data
      if ! mountpoint -q /data; then
        if ! blkid "$DISK" | grep -q ext4; then
          mkfs.ext4 "$DISK"
        fi
        mkdir -p /data
        mount "$DISK" /data
        echo "$DISK /data ext4 defaults,nofail 0 2" >> /etc/fstab
      fi

      # Install Docker (idempotent)
      if ! command -v docker &>/dev/null; then
        apt-get update -q
        apt-get install -y docker.io
        systemctl enable --now docker
        usermod -aG docker ${var.ssh_user}
      fi

      # Configure Docker credential helper for Artifact Registry
      gcloud auth configure-docker ${var.region}-docker.pkg.dev --quiet

      # Pull and start the backend (noop if image hasn't changed)
      docker pull ${local.image_url} 2>/dev/null || true

      if ! docker ps -q --filter name=nexusmind-backend | grep -q .; then
        docker run -d \
          --name nexusmind-backend \
          --restart always \
          -p 8080:8080 \
          -v /data:/data \
          -e DB_PATH=/data/nexusmind.db \
          -e RUST_LOG=info \
          ${local.image_url}
      fi
    EOT
  }

  depends_on = [
    google_compute_disk.data,
    google_compute_address.backend,
    google_artifact_registry_repository.backend,
  ]
}
