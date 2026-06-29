interface ApiError extends Error {
  status: number
}

/** Read the backend base URL lazily so tests and `setup` can run without it. */
function baseUrl(): string {
  const url = process.env.NEXUSMIND_BASE_URL ?? ''
  if (!url) {
    throw new Error(
      'NEXUSMIND_BASE_URL is not set. Run: npx @smart-coder-labs/nexusmind-mcp setup'
    )
  }
  return url
}

/** Read the API key lazily so tests and `setup` can run without it. */
function apiKey(): string {
  const key = process.env.NEXUSMIND_API_KEY ?? ''
  if (!key) {
    throw new Error(
      'NEXUSMIND_API_KEY is not set. Run: npx @smart-coder-labs/nexusmind-mcp setup'
    )
  }
  return key
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const url = baseUrl()
  const key = apiKey()

  let res: Response
  try {
    res = await fetch(`${url}${path}`, {
      ...init,
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${key}`,
        ...init?.headers,
      },
    })
  } catch {
    const err = new Error(
      `NexusMind backend not reachable at ${url}. Is it running?`
    ) as ApiError
    err.status = 0
    throw err
  }

  if (res.status === 401) {
    const err = new Error(
      'Invalid API key. Set NEXUSMIND_API_KEY to your NexusMind key.'
    ) as ApiError
    err.status = 401
    throw err
  }

  if (!res.ok) {
    const body = (await res.json().catch(() => ({ error: res.statusText }))) as {
      error?: string
    }
    const err = new Error(body.error ?? res.statusText) as ApiError
    err.status = res.status
    throw err
  }

  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

// ── Memory types ──────────────────────────────────────────────────────────────

export interface Memory {
  id: string
  user_id: string
  project: string
  tool: string
  content: string
  tags: string[]
  created_at: string
}

export interface StoreMemoryInput {
  content: string
  project?: string
  tool?: string
  tags?: string[]
}

export interface StoreMemoryResponse {
  id: string
}

// ── Memory API calls ──────────────────────────────────────────────────────────

export function storeMemory(input: StoreMemoryInput): Promise<StoreMemoryResponse> {
  return request('/v1/memory/store', {
    method: 'POST',
    body: JSON.stringify({
      content: input.content,
      project: input.project ?? '',
      tool: input.tool ?? 'claude-code',
      tags: input.tags ?? [],
    }),
  })
}

export function searchMemories(query: string, limit = 10): Promise<Memory[]> {
  return request('/v1/memory/search', {
    method: 'POST',
    body: JSON.stringify({ query, limit }),
  })
}

export function listMemories(
  params: {
    project?: string
    tool?: string
    limit?: number
  } = {}
): Promise<Memory[]> {
  const qs = new URLSearchParams()
  if (params.project) qs.set('project', params.project)
  if (params.tool) qs.set('tool', params.tool)
  if (params.limit) qs.set('limit', String(params.limit))
  return request(`/v1/memory?${qs}`)
}

// ── Policy types ──────────────────────────────────────────────────────────────

export interface Policy {
  id: string
  org_id: string
  name: string
  rule_type: string
  config: Record<string, unknown>
  enabled: boolean
  created_at: string
  updated_at: string
}

export interface CreatePolicyInput {
  name: string
  rule_type: string
  config: Record<string, unknown>
  enabled?: boolean
}

export interface UpdatePolicyInput {
  name?: string
  config?: Record<string, unknown>
  enabled?: boolean
}

export interface PolicyCheckInput {
  model: string
  prompt_tokens?: number
  prompt_preview?: string
  user_id?: string
  project?: string
}

export interface PolicyViolation {
  policy_id: string
  policy_name: string
  rule_type: string
  reason: string
}

export interface PolicyCheckResponse {
  allowed: boolean
  violations: PolicyViolation[]
}

interface PoliciesResponse {
  policies: Policy[]
}

// ── Policy API calls ──────────────────────────────────────────────────────────

/** `GET /v1/policies` — list all policies for the caller's org. */
export async function listPolicies(): Promise<Policy[]> {
  const res = await request<PoliciesResponse>('/v1/policies')
  return res.policies
}

/** `POST /v1/policies` — create a new policy. */
export function createPolicy(input: CreatePolicyInput): Promise<Policy> {
  return request('/v1/policies', {
    method: 'POST',
    body: JSON.stringify({
      name: input.name,
      rule_type: input.rule_type,
      config: input.config,
      enabled: input.enabled ?? true,
    }),
  })
}

/** `PATCH /v1/policies/:id` — update an existing policy. `rule_type` is immutable. */
export function updatePolicy(id: string, input: UpdatePolicyInput): Promise<Policy> {
  const body: Record<string, unknown> = {}
  if (input.name !== undefined) body.name = input.name
  if (input.config !== undefined) body.config = input.config
  if (input.enabled !== undefined) body.enabled = input.enabled
  return request(`/v1/policies/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    body: JSON.stringify(body),
  })
}

/** `DELETE /v1/policies/:id` — delete a policy. */
export function deletePolicy(id: string): Promise<void> {
  return request(`/v1/policies/${encodeURIComponent(id)}`, { method: 'DELETE' })
}

/** `POST /v1/policy/check` — evaluate policies against an incoming request. */
export function checkPolicy(input: PolicyCheckInput): Promise<PolicyCheckResponse> {
  const body: Record<string, unknown> = { model: input.model }
  if (input.prompt_tokens !== undefined) body.prompt_tokens = input.prompt_tokens
  if (input.prompt_preview !== undefined) body.prompt_preview = input.prompt_preview
  if (input.user_id !== undefined) body.user_id = input.user_id
  if (input.project !== undefined) body.project = input.project
  return request('/v1/policy/check', {
    method: 'POST',
    body: JSON.stringify(body),
  })
}
