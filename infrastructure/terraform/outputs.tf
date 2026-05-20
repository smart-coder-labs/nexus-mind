output "backend_ip" {
  description = "Backend static external IP"
  value       = google_compute_address.backend.address
}

output "backend_url" {
  description = "Backend API URL"
  value       = "http://${google_compute_address.backend.address}:8080"
}

output "admin_url" {
  description = "Admin panel URL (Firebase Hosting)"
  value       = "https://${google_firebase_hosting_site.admin.site_id}.web.app"
}

output "landing_url" {
  description = "Landing URL (Firebase Hosting)"
  value       = "https://${google_firebase_hosting_site.landing.site_id}.web.app"
}

output "artifact_registry_url" {
  description = "Docker image registry URL"
  value       = "${var.region}-docker.pkg.dev/${var.project_id}/nexusmind"
}

output "ssh_command" {
  description = "SSH into the backend VM"
  value       = "ssh ${var.ssh_user}@${google_compute_address.backend.address}"
}
