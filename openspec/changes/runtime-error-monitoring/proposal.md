# Proposal — Runtime Error Monitoring

**Change:** `runtime-error-monitoring`
**Project:** `nexus-mind`
**Status:** implemented (backend + admin), awaiting CI and a pilot
**Date:** 2026-11-20

## Problem

Teams run many services on GCP (Cloud Run, GKE, Cloud Functions) plus Next.js apps. When one throws an
error or behaves unexpectedly, nobody learns about it until a user complains. Today people read logs by
hand and file GitHub issues manually. NexusMind already has Slack delivery, GitHub issue creation and a
factory intake (`factory/intake.rs`) that turns external signals (Sentry, Slack, GitHub) into tasks, but
it has no way to receive runtime errors from GCP or from an app directly.

This must not be built for one client (Kasymir). Any org must be able to connect any GCP project
(including Next.js apps on Cloud Run) in minutes, from the admin panel, with no NexusMind code changes.

## Goals

1. **One intake path: GCP.** A Cloud Logging sink sends `severity>=ERROR` logs to Pub/Sub, and a push
   subscription delivers them to NexusMind. It works for any service in any language (Next.js on Cloud
   Run included) with no code changes, and it catches crashes an app can't report itself.
2. **Group errors, don't flood.** Each error gets a fingerprint. A new fingerprint produces one Slack
   alert and one GitHub issue. Repeats bump a counter, and Slack hears at most once per throttle window.
   When an error comes back after its issue was closed, it is flagged as a regression.
3. **Connect from the admin panel.** A "Monitoring" page creates a source, picks the Slack connector
   and GitHub repo, and shows the `gcloud` setup commands to copy.
4. **Optional metric alerts.** A Cloud Monitoring alert policy (crash loops, restarts, error-rate spikes)
   can publish to the same Pub/Sub topic through a Pub/Sub notification channel.
5. **Safe by default.** Every payload is untrusted input. Pub/Sub requests are verified with Google's
   OIDC token. Logs are redacted before they leave NexusMind.

## Non-goals (MVP)

- Storing or searching full logs. NexusMind keeps only error groups and samples, not a log platform.
- Dashboards or charts beyond a list of error groups.
- An npm package or SDK. Dropped on 2026-11-20: the GCP path already covers Next.js on Cloud Run. If
  browser errors or non-GCP hosting are needed later, start with a pasted snippet, not a package.
- Channels other than Slack and GitHub (no WhatsApp or email).
- Automatic fixes. Created issues can feed the existing Issue Resolver agent, but that is opt-in and
  separate.

## Cost

At about 30k errors/month (about 5 GiB), Pub/Sub stays inside its free 10 GiB/month, and Logging
routing is free. Optional alert policies cost $0.10 per condition per month. The only real cost is the
Logging ingestion teams already pay.

## Success criteria

- A new org can connect a GCP project in under 10 minutes using only the admin page.
- An error thrown in a connected service reaches Slack within 60 seconds.
- 1,000 copies of the same error produce 1 GitHub issue and at most 1 Slack message per throttle window.
- Requests with a forged or wrong token are rejected with 401 and never stored.
