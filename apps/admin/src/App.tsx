import { lazy, Suspense } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { AuthProvider, useAuth } from './auth/AuthContext'
import Login from './pages/Login'
import SetPassword from './pages/SetPassword'
import { Layout } from './components/Layout'
import { DISABLED_NAV_HREFS } from './config/disabled-sections'
import Migrations from './pages/Migrations'

const Dashboard = lazy(() => import('./pages/Dashboard'))
const Users     = lazy(() => import('./pages/Users'))
const Memories  = lazy(() => import('./pages/Memories'))
const AuditLog  = lazy(() => import('./pages/AuditLog'))
const Settings  = lazy(() => import('./pages/Settings'))
const Roles     = lazy(() => import('./pages/Roles'))
const Projects  = lazy(() => import('./pages/Projects'))
const Clients   = lazy(() => import('./pages/Clients'))
const Usage     = lazy(() => import('./pages/Usage'))
const Code      = lazy(() => import('./pages/Code'))
const ApiKeys   = lazy(() => import('./pages/ApiKeys'))
const Agents      = lazy(() => import('./pages/Agents'))
const AutonomousAgents = lazy(() => import('./pages/AutonomousAgents'))
const Policies    = lazy(() => import('./pages/Policies'))
const Conventions = lazy(() => import('./pages/Conventions'))
const Webhooks    = lazy(() => import('./pages/Webhooks'))
const Collections = lazy(() => import('./pages/Collections'))
const Tags        = lazy(() => import('./pages/Tags'))
const Search      = lazy(() => import('./pages/Search'))
const Sessions    = lazy(() => import('./pages/Sessions'))
const Backups     = lazy(() => import('./pages/Backups'))
const Graph       = lazy(() => import('./pages/Graph'))
const Harnesses   = lazy(() => import('./pages/Harnesses'))
const Tasks       = lazy(() => import('./pages/Tasks'))
const Sdd         = lazy(() => import('./pages/Sdd'))
const Unauthorized = lazy(() => import('./pages/Unauthorized'))

/** Redirects to / when the given href is in DISABLED_NAV_HREFS; otherwise renders children. */
function MaybeDisabled({ href, children }: { href: string; children: React.ReactNode }) {
  if (DISABLED_NAV_HREFS.has(href)) return <Navigate to="/" replace />
  return <>{children}</>
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { session, loading } = useAuth()
  if (loading) return null
  if (!session) return <Navigate to="/login" replace />
  return <Layout>{children}</Layout>
}

function AdminRoute({ children }: { children: React.ReactNode }) {
  const { session, loading } = useAuth()
  if (loading) return null
  if (!session) return <Navigate to="/login" replace />
  return <>{children}</>
}

function SuperUserRoute({ children }: { children: React.ReactNode }) {
  const { session, loading } = useAuth()
  if (loading) return null
  if (!session) return <Navigate to="/login" replace />
  if (session.user.role !== 'super_user') return <Navigate to="/401" replace />
  return <>{children}</>
}

const PageFallback = () => (
  <div className="flex-1 p-8">
    <div className="animate-pulse h-8 bg-white/[0.04] rounded-[11px] w-48 mb-4" />
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
                <Route path="/"          element={<Dashboard />} />
                <Route path="/dashboard" element={<Navigate to="/" replace />} />
                <Route path="/users"    element={<AdminRoute><Users /></AdminRoute>} />
                <Route path="/roles"    element={<AdminRoute><Roles /></AdminRoute>} />
                <Route path="/projects" element={<AdminRoute><Projects /></AdminRoute>} />
                <Route path="/clients"  element={<AdminRoute><Clients /></AdminRoute>} />
                <Route path="/usage"    element={<MaybeDisabled href="/usage"><AdminRoute><Usage /></AdminRoute></MaybeDisabled>} />
                <Route path="/code"     element={<AdminRoute><Code /></AdminRoute>} />
                <Route path="/api-keys" element={<MaybeDisabled href="/api-keys"><AdminRoute><ApiKeys /></AdminRoute></MaybeDisabled>} />
                <Route path="/agents"  element={<MaybeDisabled href="/agents"><AdminRoute><Agents /></AdminRoute></MaybeDisabled>} />
                <Route path="/autonomous-agents" element={<MaybeDisabled href="/autonomous-agents"><AutonomousAgents /></MaybeDisabled>} />
                <Route path="/policies"     element={<MaybeDisabled href="/policies"><AdminRoute><Policies /></AdminRoute></MaybeDisabled>} />
                <Route path="/conventions" element={<AdminRoute><Conventions /></AdminRoute>} />
                <Route path="/webhooks"    element={<MaybeDisabled href="/webhooks"><AdminRoute><Webhooks /></AdminRoute></MaybeDisabled>} />
                <Route path="/collections" element={<MaybeDisabled href="/collections"><AdminRoute><Collections /></AdminRoute></MaybeDisabled>} />
                <Route path="/search"   element={<Search />} />
                <Route path="/sessions" element={<MaybeDisabled href="/sessions"><Sessions /></MaybeDisabled>} />
                <Route path="/memories" element={<Memories />} />
                <Route path="/tags"     element={<AdminRoute><Tags /></AdminRoute>} />
                <Route path="/audit"    element={<MaybeDisabled href="/audit"><SuperUserRoute><AuditLog /></SuperUserRoute></MaybeDisabled>} />
                <Route path="/settings" element={<Settings />} />
                <Route path="/backups" element={<MaybeDisabled href="/backups"><AdminRoute><Backups /></AdminRoute></MaybeDisabled>} />
                <Route path="/harnesses" element={<MaybeDisabled href="/harnesses"><AdminRoute><Harnesses /></AdminRoute></MaybeDisabled>} />
                <Route path="/tasks"    element={<MaybeDisabled href="/tasks"><AdminRoute><Tasks /></AdminRoute></MaybeDisabled>} />
                <Route path="/sdd"      element={<MaybeDisabled href="/sdd"><AdminRoute><Sdd /></AdminRoute></MaybeDisabled>} />
                <Route path="/migrations" element={<AdminRoute><Migrations /></AdminRoute>} />
                <Route path="/graph"   element={<Graph />} />
                <Route path="/401"    element={<Unauthorized />} />
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
