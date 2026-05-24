import { Routes, Route, Navigate } from 'react-router-dom'
import { AuthProvider, useAuth } from './auth/AuthContext'
import Login from './pages/Login'
import SetPassword from './pages/SetPassword'
import Dashboard from './pages/Dashboard'
import Users from './pages/Users'
import Memories from './pages/Memories'
import AuditLog from './pages/AuditLog'
import Settings from './pages/Settings'
import Orgs from './pages/Orgs'
import Roles from './pages/Roles'
import { Layout } from './components/Layout'

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

function AppRoutes() {
  const { session, loading } = useAuth()
  return (
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
              <Route path="/orgs"     element={<Orgs />} />
              <Route path="/users"    element={<AdminRoute><Users /></AdminRoute>} />
              <Route path="/roles"    element={<AdminRoute><Roles /></AdminRoute>} />
              <Route path="/memories" element={<Memories />} />
              <Route path="/audit"    element={<AdminRoute><AuditLog /></AdminRoute>} />
              <Route path="/settings" element={<Settings />} />
            </Routes>
          </ProtectedRoute>
        }
      />
    </Routes>
  )
}

export default function App() {
  return (
    <AuthProvider>
      <AppRoutes />
    </AuthProvider>
  )
}
