import http from 'node:http'
import net from 'node:net'
import { fixture } from './workflows-preview-data.mjs'
const banner =
  '<div style="position:fixed;bottom:0;left:0;right:0;z-index:9999;background:#1d2409;color:white;padding:8px;font:12px sans-serif;text-align:center">Vista previa local · datos ilustrativos · sin conexión a producción · guardado desactivado</div>'
const server = http.createServer((req, res) => {
  const path = new URL(req.url, 'http://127.0.0.1').pathname
  if (path === '/v1' || path.startsWith('/v1/')) {
    res.setHeader('Content-Type', 'application/json')
    res.setHeader('Cache-Control', 'no-store')
    if (req.method !== 'GET') {
      res.statusCode = 405
      res.end(
        JSON.stringify({
          error:
            'Esta vista previa no guarda cambios ni ejecuta tareas. Revisa los formularios; no tiene conexión a producción.',
          code: 'preview_read_only',
        }),
      )
      return
    }
    res.end(JSON.stringify(fixture(path)))
    return
  }
  if (req.method !== 'GET' && req.method !== 'HEAD') {
    res.writeHead(405)
    res.end()
    return
  }
  const headers = { ...req.headers, host: '127.0.0.1:3015' }
  delete headers.cookie
  delete headers.authorization
  const upstream = http.request(
    { hostname: '127.0.0.1', port: 3015, path: req.url, method: req.method, headers },
    (response) => {
      const type = response.headers['content-type'] ?? ''
      if (type.includes('text/html')) {
        let body = ''
        response.setEncoding('utf8')
        response.on('data', (part) => (body += part))
        response.on('end', () => {
          const h = { ...response.headers }
          h['content-security-policy'] =
            "connect-src 'self' ws://127.0.0.1:3016 ws://127.0.0.1:3015 http://127.0.0.1:3015"
          delete h['content-length']
          delete h['content-encoding']
          res.writeHead(response.statusCode ?? 200, h)
          res.end(body.replace('</body>', banner + '</body>'))
        })
      } else {
        res.writeHead(response.statusCode ?? 200, response.headers)
        response.pipe(res)
      }
    },
  )
  upstream.on('error', () => {
    res.writeHead(502)
    res.end('El servidor Vite de la vista previa no está disponible.')
  })
  req.pipe(upstream)
})
server.on('upgrade', (req, socket, head) => {
  const upstream = net.connect(3015, '127.0.0.1', () => {
    upstream.write(
      `${req.method} ${req.url} HTTP/${req.httpVersion}\r\n${Object.entries(req.headers)
        .map(([k, v]) => `${k}: ${v}`)
        .join('\r\n')}\r\n\r\n`,
    )
    if (head.length) upstream.write(head)
    socket.pipe(upstream)
    upstream.pipe(socket)
  })
  upstream.on('error', () => socket.destroy())
  socket.on('error', () => upstream.destroy())
})
server.listen(3016, '127.0.0.1', () =>
  console.log('Read-only illustrated preview: http://127.0.0.1:3016/factory/workflows'),
)
