import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  listPolicies,
  createPolicy,
  updatePolicy,
  deletePolicy,
  checkPolicy,
  type Policy,
  type PolicyCheckResponse,
} from './client.js'

const BASE = 'http://localhost:8080'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(status === 204 ? null : JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function samplePolicy(overrides: Partial<Policy> = {}): Policy {
  return {
    id: 'pol_1',
    org_id: 'org_1',
    name: 'PII redact',
    rule_type: 'pii_redact',
    config: { patterns: ['\\d{3}-\\d{2}-\\d{4}'] },
    enabled: true,
    created_at: '2026-06-29T00:00:00.000Z',
    updated_at: '2026-06-29T00:00:00.000Z',
    ...overrides,
  }
}

let fetchMock: ReturnType<typeof vi.fn>

beforeEach(() => {
  vi.stubEnv('NEXUSMIND_BASE_URL', BASE)
  vi.stubEnv('NEXUSMIND_API_KEY', 'nm_test_key')
  fetchMock = vi.fn()
  vi.stubGlobal('fetch', fetchMock)
})

afterEach(() => {
  vi.unstubAllEnvs()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('listPolicies', () => {
  it('GETs /v1/policies and unwraps the policies array', async () => {
    const policy = samplePolicy()
    fetchMock.mockResolvedValueOnce(jsonResponse({ policies: [policy] }))

    const result = await listPolicies()

    expect(result).toEqual([policy])
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe(`${BASE}/v1/policies`)
    expect(init?.method).toBeUndefined() // GET is the default
    expect(init?.headers).toMatchObject({ Authorization: 'Bearer nm_test_key' })
  })
})

describe('createPolicy', () => {
  it('POSTs /v1/policies with the rule body and defaults enabled to true', async () => {
    const created = samplePolicy()
    fetchMock.mockResolvedValueOnce(jsonResponse(created, 201))

    const result = await createPolicy({
      name: 'PII redact',
      rule_type: 'pii_redact',
      config: { patterns: ['x'] },
    })

    expect(result).toEqual(created)
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe(`${BASE}/v1/policies`)
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({
      name: 'PII redact',
      rule_type: 'pii_redact',
      config: { patterns: ['x'] },
      enabled: true,
    })
  })
})

describe('updatePolicy', () => {
  it('PATCHes /v1/policies/:id with only the provided fields', async () => {
    const updated = samplePolicy({ enabled: false })
    fetchMock.mockResolvedValueOnce(jsonResponse(updated))

    const result = await updatePolicy('pol_1', { enabled: false })

    expect(result).toEqual(updated)
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe(`${BASE}/v1/policies/pol_1`)
    expect(init?.method).toBe('PATCH')
    expect(JSON.parse(init?.body as string)).toEqual({ enabled: false })
  })
})

describe('deletePolicy', () => {
  it('DELETEs /v1/policies/:id and resolves on 204', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(null, 204))

    await expect(deletePolicy('pol_1')).resolves.toBeUndefined()
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe(`${BASE}/v1/policies/pol_1`)
    expect(init?.method).toBe('DELETE')
  })
})

describe('checkPolicy', () => {
  it('POSTs /v1/policy/check and returns the evaluation result', async () => {
    const response: PolicyCheckResponse = {
      allowed: false,
      violations: [
        {
          policy_id: 'pol_1',
          policy_name: 'Model whitelist',
          rule_type: 'model_whitelist',
          reason: 'model "claude-3" is not allowed',
        },
      ],
    }
    fetchMock.mockResolvedValueOnce(jsonResponse(response))

    const result = await checkPolicy({ model: 'claude-3' })

    expect(result).toEqual(response)
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe(`${BASE}/v1/policy/check`)
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({ model: 'claude-3' })
  })

  it('omits undefined optional fields from the request body', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ allowed: true, violations: [] }))

    await checkPolicy({ model: 'claude-3', prompt_tokens: 1000, project: 'nexus-mind' })

    const body = JSON.parse(fetchMock.mock.calls[0][1]?.body as string)
    expect(body).toEqual({ model: 'claude-3', prompt_tokens: 1000, project: 'nexus-mind' })
    expect('user_id' in body).toBe(false)
  })
})

describe('error handling', () => {
  it('throws with the backend error message on a non-ok response', async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ error: 'rule_type must be one of: ...' }, 400)
    )

    await expect(
      createPolicy({ name: 'x', rule_type: 'bogus', config: {} })
    ).rejects.toThrow('rule_type must be one of: ...')
  })

  it('throws a clear error when the API key is missing', async () => {
    vi.stubEnv('NEXUSMIND_API_KEY', '')

    await expect(listPolicies()).rejects.toThrow('NEXUSMIND_API_KEY is not set')
    expect(fetchMock).not.toHaveBeenCalled()
  })
})
