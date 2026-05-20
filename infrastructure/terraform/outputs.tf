output "backend_url" {
  description = "Backend API URL"
  value       = "https://${fly_app.backend.name}.fly.dev"
}

output "admin_url" {
  description = "Admin panel URL (Cloudflare Pages)"
  value       = "https://nexusmind-admin.pages.dev"
}

output "landing_url" {
  description = "Landing page URL (Cloudflare Pages)"
  value       = "https://nexusmind-landing.pages.dev"
}

output "fly_app_name" {
  description = "Fly app name — use with flyctl"
  value       = fly_app.backend.name
}

output "fly_volume_id" {
  description = "SQLite volume ID"
  value       = fly_volume.data.id
}
