# Live-data design preview

To review the admin against the real API, put these non-secret settings in the ignored `.env.development.local`:

```dotenv
VITE_API_URL=
ADMIN_PROXY_TARGET=https://api.nexusmind.smartcoderlabs.com
VITE_PREVIEW_READ_ONLY=true
```

Start with `npm run dev -- --port 3005 --strictPort` and open http://127.0.0.1:3005. Sign in through the normal form with an existing account or API key. Do not store credentials in the environment file. Production login may invalidate an earlier web session for the same user.

The development-only proxy requires an HTTPS upstream and binds to loopback. It rejects nonlocal hosts, peers, and foreign browser origins. It forwards GET, HEAD, OPTIONS and the explicit POST allowlist in `live-api-proxy.ts`: login, logout, memory search, code search and code locate. Other methods and POST paths return a local 405 before reaching production. Normal backend authorization still applies to every request. Authentication creates/revokes sessions; this preview is intended to prevent business-data edits, not authentication state changes.

The proxy adapts only the local copy of the session cookie for loopback HTTP, retaining HttpOnly and SameSite. Upstream TLS verification stays enabled; production cookies and server settings are unchanged. Responses are marked `no-store`. Use the same local hostname throughout the session.

Production builds do not enable this proxy. To return to a local backend, remove these preview settings and restart Vite. The default development API target is http://localhost:8080.
