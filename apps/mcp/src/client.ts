const BASE_URL = process.env.NEXUSMIND_BASE_URL ?? ''
const API_KEY  = process.env.NEXUSMIND_API_KEY  ?? ''

interface ApiError extends Error {
  status: number
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  if (!BASE_URL) throw new Error('NEXUSMIND_BASE_URL is not set. Run: npx @smart-coder-labs/nexusmind-mcp setup')
  if (!API_KEY)  throw new Error('NEXUSMIND_API_KEY is not set. Run: npx @smart-coder-labs/nexusmind-mcp setup')

  let res: Response
  try {
    res = await fetch(`${BASE_URL}${path}`, {
      ...init,
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${API_KEY}`,
        ...init?.headers,
      },
    })
  } catch {
    const err = new Error(
      `NexusMind backend not reachable at ${BASE_URL}. Is it running?`
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
    const body = await res.json().catch(() => ({ error: res.statusText })) as { error?: string }
    const err = new Error(body.error ?? res.statusText) as ApiError
    err.status = res.status
    throw err
  }

  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

// ── Types ────────────────────────────────────────────────────────────────────

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

// ── API calls ────────────────────────────────────────────────────────────────

export function storeMemory(input: StoreMemoryInput): Promise<StoreMemoryResponse> {
  return request('/v1/memory/store', {
    method: 'POST',
    body: JSON.stringify({
      content: input.content,
      project: input.project ?? '',
      tool:    input.tool    ?? 'claude-code',
      tags:    input.tags    ?? [],
    }),
  })
}

export function searchMemories(query: string, limit = 10): Promise<Memory[]> {
  return request('/v1/memory/search', {
    method: 'POST',
    body: JSON.stringify({ query, limit }),
  })
}

export function listMemories(params: {
  project?: string
  tool?: string
  limit?: number
} = {}): Promise<Memory[]> {
  const qs = new URLSearchParams()
  if (params.project) qs.set('project', params.project)
  if (params.tool)    qs.set('tool',    params.tool)
  if (params.limit)   qs.set('limit',   String(params.limit))
  return request(`/v1/memory?${qs}`)
}
