# Personal ChatGPT plan authentication for Pi

Status: design and rollout boundary only. The factory worker does not invoke Pi,
there is no connected ChatGPT credential, and `execution_ready` must remain
`false`. Do not use this document to provision a credential until the separate
Pi runtime and owner checks described below exist.

## Supported provider

Use the official Pi package `@earendil-works/pi-coding-agent` and its `openai`
provider (Responses API). Do not use Pi's legacy `openai-codex` provider, copy
the Codex/OpenShell login, or implement OAuth endpoints in NexusMind. Pi 1.3.0
implements OpenAI's documented Sign in with ChatGPT (SIWC) flow, including
dynamic client registration, PKCE, consent scopes, and rotating refresh-token
handling. Pin and review the package version when building the executor image.

OpenAI's [SIWC overview](https://developers.openai.com/siwc/token-sharing-open-source),
[sign-in flow](https://developers.openai.com/siwc/token-sharing-open-source/sign-in),
and [self-hosted VM guidance](https://developers.openai.com/siwc/token-sharing-open-source/self-hosted-vms)
are the protocol authority. Pi's [provider documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/providers.md)
states that `auth.json` contains OAuth tokens and must remain private.

## Personal-use boundary

This connection belongs to one named NexusMind user. Every model request must
come from that user's own manual activity, or from an automation which that same
user explicitly enabled and owns. A team member, webhook, schedule, or service
account must not spend the owner's ChatGPT plan merely because it belongs to the
same organization. Until the queue records and verifies the initiating user and
the automation's owner/consent, non-manual triggers must be rejected.

Do not put a personal OAuth credential in `factory-egress-proxy`, `nexusmind-env`,
an org-wide config, SQLite, run inputs, logs, transcripts, or task pods. Do not
share one token across a team. Shared use requires per-user SIWC connections or
an explicitly configured API key. Keep the API-key route separate; never fall
back to Claude or another provider when personal SIWC is unavailable.

## Required execution boundary

Task pods execute untrusted repository code. Therefore Pi's `auth.json` must
never be mounted in a task pod, even read-only: the default Pi toolset includes
`bash`, `read`, `edit`, and `write`, and a prompt or repository script could
exfiltrate a mounted file. The existing worker also has no Secret access, which
must remain true.

The eventual design needs a dedicated, trusted Pi executor/service outside the
task pod. It must hold one user-scoped credential, authenticate the user and
automation owner before every request, and expose only a narrow model-inference
interface to the sandbox. It must not expose shell/file tools to repository
content. Run authorization must be signed and bind the NexusMind `user_id`,
automation ID (when applicable), consent generation, run ID, provider, and model;
the relay must verify those claims. A task pod receives only a short-lived,
run-scoped relay token, never the ChatGPT token.

## Credential provisioning and renewal (operator flow, not yet enabled)

1. The owner completes Pi's official SIWC login on a trusted interactive device
   using the pinned Pi release. The owner must see and accept OpenAI's consent
   screen. NexusMind must not collect the authorization code or run the login.
2. The owner transfers Pi's `auth.json` through the approved Kubernetes secret
   management channel into a dedicated Secret for that user and the dedicated
   Pi executor only. Do not place it in a shared application Secret. Restrict
   Secret read/update RBAC to the executor operator and named Secret; enable the
   cluster's encryption-at-rest mechanism. Set file mode `0600`, disable shell
   dumps, redact request headers, and never log token JSON.
3. Pi rotates refresh tokens. A read-only projected Secret is not a durable
   refresh store. Before enabling runtime, implement and review a rotation path
   that atomically writes the refreshed `auth.json` back to the same
   user-scoped Secret (or to an approved encrypted secret manager), with narrowly
   scoped permissions and no SQLite/log copy. Until this exists, expiry or
   refresh failure blocks the run and requires owner reauthentication.
4. After credential status, owner identity, and consent are verified, the owner
   explicitly enables the feature. Disabling it or revoking consent must block
   new runs immediately and invalidate outstanding relay tokens.

Never set `execution_ready=true` from a role/model preference alone. Readiness
requires all of: feature flag enabled, valid owner-scoped credential, the
requesting user equals its owner (or a validated owner-consented automation),
the Pi executor health check succeeds, and the credential can refresh safely.

## Missing implementation before an OAuth pilot

- A Pi runtime image/service with the package version pinned and model-only
  access to the repository sandbox.
- Owner identity and explicit automation-consent fields carried from trigger
  creation through the claimed run and signed relay token. Current autonomous
  claims do not include `automation_runs.created_by`.
- User-scoped credential status/rotation outside SQLite and the global egress
  Secret; account disconnection, refresh failure, and revocation handling.
- Feature flag default-off and an authorization gate that rejects other users,
  unowned webhook triggers, and schedules without recorded owner consent.
- Tests proving a task pod cannot read the credential, an unrelated user cannot
  trigger a request, and missing/expired credentials fail closed without
  contacting Claude or the OpenAI API-key provider.

Until those pieces land, the current worker guard is the expected behavior:
configured ChatGPT roles remain unavailable and runs stop before launching a
provider.
