# ── Fly.io ────────────────────────────────────────────────────────────────────

variable "fly_api_token" {
  description = "Fly.io API token (fly tokens create deploy)"
  type        = string
  sensitive   = true
}

variable "fly_app_name" {
  description = "Fly app name — must be globally unique"
  type        = string
  default     = "nexusmind-api"
}

variable "fly_region" {
  description = "Primary Fly.io region"
  type        = string
  default     = "mad" # Madrid
}

variable "fly_volume_size_gb" {
  description = "SQLite volume size in GB"
  type        = number
  default     = 1
}

# ── Cloudflare ────────────────────────────────────────────────────────────────

variable "cloudflare_api_token" {
  description = "Cloudflare API token with Pages:Edit permission"
  type        = string
  sensitive   = true
}

variable "cloudflare_account_id" {
  description = "Cloudflare account ID (dash.cloudflare.com → right sidebar)"
  type        = string
}

variable "github_owner" {
  description = "GitHub org or user that owns the repo"
  type        = string
  default     = "smart-coder-labs"
}

variable "github_repo" {
  description = "GitHub repository name"
  type        = string
  default     = "nexus-mind"
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
