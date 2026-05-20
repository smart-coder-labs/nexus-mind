terraform {
  required_version = ">= 1.6"

  required_providers {
    fly = {
      source  = "fly-apps/fly"
      version = "~> 0.0.23"
    }
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.0"
    }
  }

  # Uncomment to use Terraform Cloud for shared state
  # backend "remote" {
  #   organization = "your-org"
  #   workspaces { name = "nexusmind" }
  # }
}

provider "fly" {
  fly_api_token = var.fly_api_token
}

provider "cloudflare" {
  api_token = var.cloudflare_api_token
}
