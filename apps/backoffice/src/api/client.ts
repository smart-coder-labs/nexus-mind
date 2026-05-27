import type { Org, OrgWithStats, User, AuditEntry, GlobalMetrics, CreateOrgResponse } from '../types'

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

// ── Metrics ────────────────────────────────────────────────────────────────────

export function getMetrics(): Promise<GlobalMetrics> {
  return request('/internal/metrics')
}

// ── Orgs ───────────────────────────────────────────────────────────────────────

export function listOrgs(): Promise<OrgWithStats[]> {
  return request('/internal/orgs')
}

export function createOrg(data: {
  org_name: string
  org_slug: string
  admin_email: string
  admin_name: string
}): Promise<CreateOrgResponse> {
  return request('/internal/orgs', { method: 'POST', body: JSON.stringify(data) })
}

export function updateOrg(orgId: string, data: { name: string }): Promise<Org> {
  return request(`/internal/orgs/${orgId}`, { method: 'PATCH', body: JSON.stringify(data) })
}

// ── Users ─────────────────────────────────────────────────────────────────────

export function listOrgUsers(orgId: string): Promise<User[]> {
  return request(`/internal/orgs/${orgId}/users`)
}

export function listAllUsers(): Promise<User[]> {
  return request('/internal/users')
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
  return request(`/internal/audit?${params}`)
}

// ── Single org ─────────────────────────────────────────────────────────────────

export function getOrg(orgId: string): Promise<OrgWithStats> {
  return request(`/internal/orgs/${orgId}`)
}

export function deleteOrg(orgId: string): Promise<void> {
  return request(`/internal/orgs/${orgId}`, { method: 'DELETE' })
}

export function impersonateOrg(orgId: string): Promise<{ token: string }> {
  return request(`/internal/orgs/${orgId}/impersonate`, { method: 'POST' })
}

// ── User actions ───────────────────────────────────────────────────────────────

export function suspendUser(userId: string): Promise<void> {
  return request(`/internal/users/${userId}/suspend`, { method: 'POST' })
}

// ── Auth check (validate key) ─────────────────────────────────────────────────

export async function validateKey(key: string): Promise<boolean> {
  try {
    const res = await fetch(`${BASE_URL}/internal/metrics`, {
      headers: { Authorization: `Bearer ${key}` },
    })
    return res.ok
  } catch {
    return false
  }
}
