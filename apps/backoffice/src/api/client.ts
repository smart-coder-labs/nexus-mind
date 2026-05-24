import type { Org, User, AuditEntry, CreateOrgResponse } from '../types'

const BASE_URL = import.meta.env.VITE_API_URL ?? ''

function getSuperuserKey(): string {
  return sessionStorage.getItem('bo_key') ?? ''
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const key = getSuperuserKey()
  const res = await fetch(`${BASE_URL}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(key ? { Authorization: `Bearer ${key}` } : {}),
      ...init?.headers,
    },
  })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText, code: 'unknown' }))
    throw Object.assign(new Error(body.error ?? res.statusText), {
      code: body.code,
      status: res.status,
    })
  }
  if (res.status === 204) return undefined as T
  return res.json()
}

// ── Orgs ──────────────────────────────────────────────────────────────────────

export function listOrgs(): Promise<Org[]> {
  return request('/v1/orgs')
}

export function createOrg(data: {
  org_name: string
  org_slug: string
  admin_email: string
  admin_name: string
}): Promise<CreateOrgResponse> {
  return request('/v1/orgs', { method: 'POST', body: JSON.stringify(data) })
}

// ── Users (cross-org via superuser) ──────────────────────────────────────────

export function listOrgUsers(orgId: string): Promise<User[]> {
  return request(`/v1/orgs/${orgId}/users`)
}

// ── Audit ─────────────────────────────────────────────────────────────────────

export interface AuditFilters {
  limit?: number
  offset?: number
  action?: string
  resource_type?: string
  from?: string
  to?: string
}

export function listAudit(filters: AuditFilters = {}): Promise<AuditEntry[]> {
  const params = new URLSearchParams()
  Object.entries(filters).forEach(([k, v]) => v != null && params.set(k, String(v)))
  return request(`/v1/audit?${params}`)
}

// ── Auth check (validate key) ─────────────────────────────────────────────────

export async function validateKey(key: string): Promise<boolean> {
  try {
    const res = await fetch(`${BASE_URL}/v1/orgs`, {
      headers: { Authorization: `Bearer ${key}` },
    })
    return res.ok
  } catch {
    return false
  }
}
