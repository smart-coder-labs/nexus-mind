import { lazy, Suspense } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { AuthProvider, useAuth } from './auth/AuthContext'
import Login from './pages/Login'
import SetPassword from './pages/SetPassword'
import { Layout } from './components/Layout'

const Dashboard = lazy(() => import('./pages/Dashboard'))
const Users     = lazy(() => import('./pages/Users'))
const Memories  = lazy(() => import('./pages/Memories'))
const AuditLog  = lazy(() => import('./pages/AuditLog'))
const Settings  = lazy(() => import('./pages/Settings'))
const Roles     = lazy(() => import('./pages/Roles'))
const Projects  = lazy(() => import('./pages/Projects'))
const Code      = lazy(() => import('./pages/Code'))
const ApiKeys   = lazy(() => import('./pages/ApiKeys'))
const Agents      = lazy(() => import('./pages/Agents'))
const Policies    = lazy(() => import('./pages/Policies'))
const Conventions = lazy(() => import('./pages/Conventions'))
const Webhooks    = lazy(() => import('./pages/Webhooks'))
const Collections = lazy(() => import('./pages/Collections'))
const Tags        = lazy(() => import('./pages/Tags'))
const Search      = lazy(() => import('./pages/Search'))
const Sessions    = lazy(() => import('./pages/Sessions'))

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { session, loading } = useAuth()
  if (loading) return null
  if (!session) return <Navigate to="/login" replace />
  return <Layout>{children}</Layout>
}

/** Redirects non-admin users to `/` without re-wrapping in Layout. */
function AdminRoute({ children }: { children: React.ReactNode }) {
  const { session, loading } = useAuth()
  if (loading) return null
  if (!session) return <Navigate to="/login" replace />
  if (session.user.role !== 'admin') return <Navigate to="/" replace />
  return <>{children}</>
}

const PageFallback = () => (
  <div className="flex-1 p-8">
    <div className="animate-pulse h-8 bg-[#272729] rounded-[11px] w-48 mb-4" />
  </div>
)

function AppRoutes() {
  const { session, loading } = useAuth()
  return (
    <Suspense fallback={<PageFallback />}>
      <Routes>
        <Route path="/set-password" element={<SetPassword />} />
        <Route
          path="/login"
          element={loading ? null : session ? <Navigate to="/" replace /> : <Login />}
        />
        <Route
          path="/*"
          element={
            <ProtectedRoute>
              <Routes>
                <Route path="/"         element={<Dashboard />} />
                <Route path="/users"    element={<AdminRoute><Users /></AdminRoute>} />
                <Route path="/roles"    element={<AdminRoute><Roles /></AdminRoute>} />
                <Route path="/projects" element={<AdminRoute><Projects /></AdminRoute>} />
                <Route path="/code"     element={<AdminRoute><Code /></AdminRoute>} />
                <Route path="/api-keys" element={<AdminRoute><ApiKeys /></AdminRoute>} />
                <Route path="/agents"  element={<AdminRoute><Agents /></AdminRoute>} />
                <Route path="/policies"     element={<AdminRoute><Policies /></AdminRoute>} />
                <Route path="/conventions" element={<AdminRoute><Conventions /></AdminRoute>} />
                <Route path="/webhooks"    element={<AdminRoute><Webhooks /></AdminRoute>} />
                <Route path="/collections" element={<AdminRoute><Collections /></AdminRoute>} />
                <Route path="/search"   element={<Search />} />
                <Route path="/sessions" element={<Sessions />} />
                <Route path="/memories" element={<Memories />} />
                <Route path="/tags"     element={<AdminRoute><Tags /></AdminRoute>} />
                <Route path="/audit"    element={<AdminRoute><AuditLog /></AdminRoute>} />
                <Route path="/settings" element={<Settings />} />
              </Routes>
            </ProtectedRoute>
          }
        />
      </Routes>
    </Suspense>
  )
}

export default function App() {
  return (
    <AuthProvider>
      <AppRoutes />
    </AuthProvider>
  )
}
