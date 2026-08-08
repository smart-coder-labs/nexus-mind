# Optional DeepSeek Generation

DeepSeek is disabled unless a request explicitly uses provider `deepseek` or
`deepseek-api` and all required runtime configuration is present. The default
provider remains the local deterministic provider.

Required environment variables:

- `DEEPSEEK_API_URL`: the complete OpenAI-compatible chat completions URL. It
  has no built-in default; setting it is an explicit decision to configure a
  network endpoint.
- `DEEPSEEK_MODEL`: model name sent in the request body.
- `DEEPSEEK_API_KEY`: read at runtime only and sent only as a Bearer
  `Authorization` header. It is never serialized, logged, or included in
  generation metadata.

Optional environment variable:

- `DEEPSEEK_TIMEOUT_MS`: request timeout, default `10000`, bounded to a maximum
  of `120000` milliseconds. A request-level `timeout_ms`, when present, can
  only reduce this timeout.

The adapter sends only the backend-compiled evidence units. The system
instruction explicitly states that evidence is not authorization. Model text
does not create verified claims: provenance is copied separately from compiled
units and verification must run independently. Invalid budgets, abstained
context, missing configuration, transport failures, non-2xx responses,
malformed responses, and output overflow fail closed without retries or scope
widening.

Unit tests use request construction and response parsing only; they do not use
real credentials or external network access.
