import type { Plugin, ProxyOptions } from 'vite'

// This allowlist grants no backend permissions: every request still needs the
// production session. Only authentication and known read-only queries may POST.
const READ_POSTS = new Set([
  '/v1/admin/auth/login',
  '/v1/admin/auth/logout',
  '/v1/memory/search',
  '/v1/code/search',
  '/v1/code/locate',
])

export function allowsPreviewRequest(method: string, url: string): boolean {
  const path = new URL(url, 'http://localhost').pathname
  return ['GET', 'HEAD', 'OPTIONS'].includes(method)
    || (method === 'POST' && READ_POSTS.has(path))
}

export function localSessionCookie(cookie: string): string {
  // Only the loopback copy is adapted. The upstream cookie/security config is
  // untouched. HttpOnly and SameSite remain present; no token enters JS/env.
  if (!cookie.startsWith('nexusmind_session=')) return cookie
  return cookie.replace(/;\s*Secure\b/gi, '').replace(/;\s*Domain=[^;]*/gi, '')
}

export function liveApiProxy(target: string): { plugin: Plugin; proxy: ProxyOptions } {
  if (new URL(target).protocol !== 'https:') {
    throw new Error('The live-data preview requires an HTTPS API target.')
  }
  return {
    plugin: {
      name: 'live-api-read-only',
      apply: 'serve',
      configureServer(server) {
        server.middlewares.use((req, res, next) => {
          if (!req.url?.startsWith('/v1')) return next()
          const port = req.socket.localPort
          const localHosts = new Set([`localhost:${port}`, `127.0.0.1:${port}`])
          let sameOrigin = !req.headers.origin
          try {
            if (req.headers.origin) sameOrigin = new URL(req.headers.origin).host === req.headers.host
          } catch { sameOrigin = false }
          const localPeer = ['127.0.0.1', '::1', '::ffff:127.0.0.1'].includes(req.socket.remoteAddress ?? '')
          if (!localPeer || !localHosts.has(req.headers.host ?? '') || !sameOrigin) {
            res.writeHead(403, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
            res.end(JSON.stringify({ error: 'This preview is available only from its local origin.', code: 'preview_origin_blocked' }))
            return
          }
          if (!allowsPreviewRequest(req.method ?? 'GET', req.url)) {
            res.writeHead(405, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
            res.end(JSON.stringify({ error: 'Live-data preview is read-only. Changes are not sent to production.', code: 'preview_read_only' }))
            return
          }
          next()
        })
      },
    },
    proxy: {
      target,
      changeOrigin: true,
      secure: true,
      configure(proxy) {
        proxy.on('proxyRes', (response) => {
          const cookies = response.headers['set-cookie']
          if (cookies) response.headers['set-cookie'] = cookies.map(localSessionCookie)
          response.headers['cache-control'] = 'no-store'
        })
      },
    },
  }
}
