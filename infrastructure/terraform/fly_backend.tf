# ── Fly.io — Backend (Rust + SQLite) ─────────────────────────────────────────

resource "fly_app" "backend" {
  name = var.fly_app_name
  org  = "personal"
}

# Persistent volume for SQLite — survives deploys and restarts
resource "fly_volume" "data" {
  name       = "nexusmind_data"
  app        = fly_app.backend.name
  size       = var.fly_volume_size_gb
  region     = var.fly_region
  depends_on = [fly_app.backend]
}

# Shared IPv4 (costs $2/mo) — needed for public HTTPS
resource "fly_ip" "backend_v4" {
  app    = fly_app.backend.name
  type   = "v4"
  region = var.fly_region
}

# IPv6 is free
resource "fly_ip" "backend_v6" {
  app    = fly_app.backend.name
  type   = "v6"
  region = var.fly_region
}
