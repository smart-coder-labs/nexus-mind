import { Routes, Route, Navigate } from 'react-router-dom'
import { AuthProvider, useAuth } from './auth/AuthContext'
import Login from './pages/Login'
import SetPassword from './pages/SetPassword'
import Dashboard from './pages/Dashboard'
import Users from './pages/Users'
import Memories from './pages/Memories'
import AuditLog from './pages/AuditLog'
import Settings from './pages/Settings'
import { Layout } from './components/Layout'

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { session } = useAuth()
  if (!session) return <Navigate to="/login" replace />
  return <Layout>{children}</Layout>
}

function AppRoutes() {
  const { session } = useAuth()
  return (
    <Routes>
      <Route path="/set-password" element={<SetPassword />} />
      <Route
        path="/login"
        element={session ? <Navigate to="/" replace /> : <Login />}
      />
      <Route
        path="/*"
        element={
          <ProtectedRoute>
            <Routes>
              <Route path="/"         element={<Dashboard />} />
              <Route path="/users"    element={<Users />} />
              <Route path="/memories" element={<Memories />} />
              <Route path="/audit"    element={<AuditLog />} />
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
