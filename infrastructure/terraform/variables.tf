# ── GCP ───────────────────────────────────────────────────────────────────────

variable "project_id" {
  description = "GCP project ID"
  type        = string
}

variable "region" {
  description = "GCP region — must be us-central1, us-east1, or us-west1 for free tier"
  type        = string
  default     = "us-central1"
}

variable "zone" {
  description = "GCP zone for the Compute Engine instance"
  type        = string
  default     = "us-central1-a"
}

# ── Compute Engine ────────────────────────────────────────────────────────────

variable "instance_name" {
  description = "Name of the backend VM"
  type        = string
  default     = "nexusmind-backend"
}

variable "data_disk_size_gb" {
  description = "Persistent disk size for SQLite data (free tier includes 30GB HDD)"
  type        = number
  default     = 10
}

variable "ssh_public_key" {
  description = "SSH public key to add to the VM (cat ~/.ssh/id_rsa.pub)"
  type        = string
}

variable "ssh_user" {
  description = "SSH username on the VM"
  type        = string
  default     = "nexusmind"
}

# ── App config ────────────────────────────────────────────────────────────────

variable "supabase_url" {
  description = "Supabase project URL for the landing waitlist (optional)"
  type        = string
  default     = ""
}

variable "supabase_anon_key" {
  description = "Supabase anon key for the landing waitlist (optional)"
  type        = string
  sensitive   = true
  default     = ""
}
