# ── Cloudflare Pages — Admin Panel ───────────────────────────────────────────

resource "cloudflare_pages_project" "admin" {
  account_id        = var.cloudflare_account_id
  name              = "nexusmind-admin"
  production_branch = "main"

  source {
    type = "github"
    config {
      owner                         = var.github_owner
      repo_name                     = var.github_repo
      production_branch             = "main"
      pr_comments_enabled           = true
      deployments_enabled           = true
      production_deployment_enabled = true
      preview_deployment_setting    = "custom"
      preview_branch_includes       = ["feat/*", "fix/*"]
      preview_branch_excludes       = ["main"]
    }
  }

  build_config {
    build_command       = "cd apps/admin && npm ci && npm run build"
    destination_dir     = "apps/admin/dist"
    root_dir            = ""
    build_caching       = true
  }

  deployment_configs {
    production {
      environment_variables = {
        VITE_API_URL = "https://${var.fly_app_name}.fly.dev"
        NODE_VERSION = "20"
      }
    }
    preview {
      environment_variables = {
        VITE_API_URL = "https://${var.fly_app_name}.fly.dev"
        NODE_VERSION = "20"
      }
    }
  }
}
