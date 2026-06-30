import { Routes, Route, Navigate } from 'react-router-dom'
import { AuthProvider, useAuth } from './auth/AuthContext'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Orgs from './pages/Orgs'
import OrgDetail from './pages/OrgDetail'
import Users from './pages/Users'
import AuditLog from './pages/AuditLog'
import Search from './pages/Search'
import { Layout } from './components/Layout'

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { authenticated, loading } = useAuth()
  if (loading) return null
  if (!authenticated) return <Navigate to="/login" replace />
  return <Layout>{children}</Layout>
}

function AppRoutes() {
  const { authenticated, loading } = useAuth()
  return (
    <Routes>
      <Route
        path="/login"
        element={loading ? null : authenticated ? <Navigate to="/" replace /> : <Login />}
      />
      <Route
        path="/*"
        element={
          <ProtectedRoute>
            <Routes>
              <Route path="/"              element={<Dashboard />} />
              <Route path="/orgs"          element={<Orgs />} />
              <Route path="/orgs/:id"      element={<OrgDetail />} />
              <Route path="/users"         element={<Users />} />
              <Route path="/audit"         element={<AuditLog />} />
              <Route path="/search"        element={<Search />} />
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
