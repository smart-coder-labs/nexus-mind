# ── Cloudflare Pages — Landing (Astro) ───────────────────────────────────────

resource "cloudflare_pages_project" "landing" {
  account_id        = var.cloudflare_account_id
  name              = "nexusmind-landing"
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
    build_command       = "cd apps/landing && npm ci && npm run build"
    destination_dir     = "apps/landing/dist"
    root_dir            = ""
    build_caching       = true
  }

  deployment_configs {
    production {
      environment_variables = {
        PUBLIC_SUPABASE_URL      = var.supabase_url
        PUBLIC_SUPABASE_ANON_KEY = var.supabase_anon_key
        NODE_VERSION             = "20"
      }
    }
    preview {
      environment_variables = {
        PUBLIC_SUPABASE_URL      = var.supabase_url
        PUBLIC_SUPABASE_ANON_KEY = var.supabase_anon_key
        NODE_VERSION             = "20"
      }
    }
  }
}
