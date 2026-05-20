# ── Firebase Hosting — Admin Panel + Landing ──────────────────────────────────
# Firebase resources require the google-beta provider.
# Files are deployed via CI (firebase deploy), not Terraform.
# Terraform only provisions the hosting sites.

resource "google_firebase_project" "default" {
  provider   = google-beta
  project    = var.project_id
  depends_on = [google_project_service.apis]
}

resource "google_firebase_hosting_site" "admin" {
  provider = google-beta
  project  = var.project_id
  site_id  = "${var.project_id}-admin"

  depends_on = [google_firebase_project.default]
}

resource "google_firebase_hosting_site" "landing" {
  provider = google-beta
  project  = var.project_id
  site_id  = "${var.project_id}-landing"

  depends_on = [google_firebase_project.default]
}
