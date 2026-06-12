import { getAuthToken } from '../auth/AuthContext'

export interface DownloadDeps {
  fetcher?: typeof fetch
  createObjectURL?: (b: Blob) => string
  revokeObjectURL?: (u: string) => void
}

export async function downloadExport(
  url: string,
  filename: string,
  deps: DownloadDeps = {},
): Promise<void> {
  const fetcher = deps.fetcher ?? fetch
  const createURL = deps.createObjectURL ?? URL.createObjectURL
  const revokeURL = deps.revokeObjectURL ?? URL.revokeObjectURL

  const token = getAuthToken()
  const headers: Record<string, string> = {}
  if (token) headers.Authorization = `Bearer ${token}`

  const res = await fetcher(url, { method: 'GET', headers, credentials: 'include' })
  if (!res.ok) {
    let message = `HTTP ${res.status}`
    try {
      const body = await res.json()
      if (body?.error) message = body.error
    } catch {
      // body wasn't JSON, keep generic message
    }
    throw new Error(message)
  }

  const blob = await res.blob()
  const objectUrl = createURL(blob)
  const a = document.createElement('a')
  a.href = objectUrl
  a.download = filename
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  revokeURL(objectUrl)
}

export function todayStamp(): string {
  return new Date().toISOString().slice(0, 10)
}
