import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import * as Sentry from '@sentry/react'
import './index.css'
import App from './App.tsx'

Sentry.init({
  dsn: import.meta.env.VITE_SENTRY_DSN,
  environment: import.meta.env.VITE_SENTRY_ENVIRONMENT || import.meta.env.MODE,
  enabled: Boolean(import.meta.env.VITE_SENTRY_DSN),
  sendDefaultPii: false,
})

createRoot(document.getElementById('root')!, {
  onUncaughtError: Sentry.reactErrorHandler(),
  onRecoverableError: Sentry.reactErrorHandler(),
}).render(
  <StrictMode>
    <BrowserRouter>
      <Sentry.ErrorBoundary fallback={<p role="alert">Ocurrió un error inesperado.</p>}>
        <App />
      </Sentry.ErrorBoundary>
    </BrowserRouter>
  </StrictMode>,
)
