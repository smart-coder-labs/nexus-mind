# Sentry

Sentry is initialized for the Rust backend processes, the admin and backoffice
React applications, the Astro landing page, and both parts of Bootcamp Tracker.
Reporting stays disabled when an application has no DSN.

## GitHub Actions configuration

Add these **repository variables** under Settings → Secrets and variables →
Actions → Variables:

| Variable | Used by |
| --- | --- |
| `VITE_SENTRY_DSN_ADMIN` | Admin panel builds, including the U2S container |
| `VITE_SENTRY_DSN_BACKOFFICE` | Backoffice Cloudflare Pages build |
| `PUBLIC_SENTRY_DSN_LANDING` | Landing Cloudflare Pages build |

Each value is the DSN for the corresponding Sentry project. Browser DSNs are
included in the client bundle, so treat them as public project identifiers.
The workflows set their Sentry environment to `production`.

Add these **repository secrets** under Actions → Secrets:

| Secret | Used by |
| --- | --- |
| `SENTRY_DSN` | Oracle backend deploy; patches `nexusmind-env` when provided |

For the manual U2S deploy workflow, add `U2S_SENTRY_DSN` to the
`u2s-production` GitHub Environment secrets. It is written to the server's
Compose `.env` file and passed to the backend container. Add these secrets only
when using that deployment: Sentry is optional.

The existing deploy workflows do not deploy Bootcamp Tracker. When that app
gets a deployment workflow, set `VITE_SENTRY_DSN` for its client build and
`SENTRY_DSN` for its Node server runtime. Its optional tracing sample rates are
`VITE_SENTRY_TRACES_SAMPLE_RATE` and `SENTRY_TRACES_SAMPLE_RATE` (both default
to `0.1`).

## Local and self-hosted configuration

Set `SENTRY_DSN` on the Rust or Node server. Rust also reads `SENTRY_ENVIRONMENT`
and `SENTRY_RELEASE`; the Node server additionally reads
`SENTRY_TRACES_SAMPLE_RATE` (default `0.1`). For local Vite apps, set
`VITE_SENTRY_DSN`; Astro reads `PUBLIC_SENTRY_DSN`. Their environment defaults
to the build mode and does not need a separate variable.

Source maps are not uploaded by these workflows, so no `SENTRY_AUTH_TOKEN`,
organization slug, or project slug is needed.
