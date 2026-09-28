# Nexus executor for autonomous agents

The `nexus` executor runs the existing Claude Code CLI inside a persistent
OpenShell sandbox, then returns its JSONL transcript and safely synchronizes
workspace changes. The backend is only a client of a separately operated
OpenShell gateway; it does not require or receive a Docker socket.

## Production prerequisites

1. Operate an OpenShell gateway with a sandbox compute driver on a separate
   host or cluster workload. Restrict access to the worker, authenticate it,
   and configure a private endpoint with trusted TLS. Do not publish the
   unauthenticated gateway on the public Internet.
2. Register that gateway for the worker's persisted `HOME` using
   `openshell gateway add https://<private-gateway> --name production`, then
   set `OPENSHELL_GATEWAY=production` in the worker environment. Provision
   the gateway's client credentials according to its authentication mode.
3. The Oracle workflow builds and publishes `apps/nexus-cli/sandbox/Dockerfile`
   to `ghcr.io/<owner>/nexusmind-openshell:main-arm64`. Confirm that the
   gateway's compute driver can pull it (including GHCR credentials for a
   private package). Resolve the published digest and set
   `NEXUS_OPENSHELL_IMAGE` in the `nexusmind-env` Kubernetes secret to the
   immutable `ghcr.io/<owner>/nexusmind-openshell@sha256:<digest>` reference.
4. The worker image contains `/app/nexus` and the OpenShell CLI. Set
   `NEXUS_WORKER_BIN=/app/nexus` (the default). The worker uses the `essential`
   NexusMind MCP profile.
   The Oracle worker inherits `OPENSHELL_GATEWAY`, `NEXUS_OPENSHELL_IMAGE`, and
   credentials from `nexusmind-env`; they must be provisioned before selecting
   Nexus in an agent definition. The existing Claude executor remains usable
   while these values are absent.
5. Start a run with a single agent after provisioning. The sandbox name is
   stable per agent definition (`nx-a-…`), so it can retain the Claude Code
   executable login. Perform `claude auth login` in that sandbox once using an
   operator-controlled interactive session. No Anthropic model API key is
   requested by the worker.
6. If an agent uses the NexusMind MCP, attach an OpenShell provider that
   supplies the NexusMind credential; configure its name via
   `NEXUS_OPENSHELL_EXTRA_PROVIDERS`. Do not put secret values in MCP config
   files or command arguments.

The gateway is a separate service, not a container embedded in the backend.
For a Kubernetes deployment, use the upstream OpenShell gateway/operator
installation and a private service; the backend deployment only needs network
access, its gateway registration/certificates, and the environment above.

The worker reports `nexus_gateway_unavailable`, `nexus_auth_required`,
`nexus_cli_unavailable`, or `nexus_runtime_failed` when a prerequisite fails.
Do not enable Nexus agents for production traffic until a real gateway,
interactive login, image pull, transcript, file sync, and cancellation drill
have passed in the target environment. None of those steps is a deployment
performed by this change.
