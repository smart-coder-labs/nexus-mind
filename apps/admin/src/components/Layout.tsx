import { useState } from 'react'
import { useLocation, useNavigate, Link } from 'react-router-dom'
import {
  LayoutDashboard,
  Users,
  Brain,
  ScrollText,
  Settings,
  LogOut,
  Menu,
  X,
} from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { cn } from '@/lib/utils'

interface NavItem {
  label: string
  href: string
  icon: React.ReactNode
}

const NAV_ITEMS: NavItem[] = [
  { label: 'Dashboard', href: '/', icon: <LayoutDashboard className="w-4 h-4" /> },
  { label: 'Users', href: '/users', icon: <Users className="w-4 h-4" /> },
  { label: 'Memories', href: '/memories', icon: <Brain className="w-4 h-4" /> },
  { label: 'Audit Log', href: '/audit', icon: <ScrollText className="w-4 h-4" /> },
  { label: 'Settings', href: '/settings', icon: <Settings className="w-4 h-4" /> },
]

function NavLinks({ onNavigate }: { onNavigate?: () => void }) {
  const location = useLocation()

  return (
    <nav className="flex flex-col space-y-1 px-3">
      {NAV_ITEMS.map((item) => {
        const isActive =
          item.href === '/'
            ? location.pathname === '/'
            : location.pathname.startsWith(item.href)
        return (
          <Link
            key={item.href}
            to={item.href}
            onClick={onNavigate}
            className={cn(
              'flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-apple focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue',
              isActive
                ? 'bg-surface-primary text-text-primary'
                : 'text-text-secondary hover:bg-surface-primary/20',
            )}
          >
            {item.icon}
            {item.label}
          </Link>
        )
      })}
    </nav>
  )
}

function SidebarContent({ onNavigate }: { onNavigate?: () => void }) {
  const { session, logout } = useAuth()
  const navigate = useNavigate()

  const handleLogout = () => {
    logout()
    navigate('/login')
    onNavigate?.()
  }

  return (
    <div className="flex flex-col h-full bg-surface-secondary">
      {/* Logo */}
      <div className="px-6 py-5 border-b border-border-secondary">
        <p className="text-lg font-bold text-text-primary">NexusMind</p>
        <p className="text-xs text-text-tertiary mt-0.5 truncate">
          {session?.org.name}
        </p>
      </div>

      {/* Nav */}
      <div className="flex-1 py-4 overflow-y-auto">
        <NavLinks onNavigate={onNavigate} />
      </div>

      {/* Logout */}
      <div className="px-3 py-4 border-t border-border-secondary">
        <button
          onClick={handleLogout}
          className="flex w-full items-center gap-3 px-3 py-2 rounded-md text-sm font-medium text-text-secondary hover:bg-surface-primary/20 transition-apple focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue"
        >
          <LogOut className="w-4 h-4" />
          Sign out
        </button>
      </div>
    </div>
  )
}

export function Layout({ children }: { children: React.ReactNode }) {
  const [drawerOpen, setDrawerOpen] = useState(false)

  return (
    <div className="min-h-screen bg-gray-950 flex">
      {/* Desktop sidebar */}
      <aside className="hidden lg:flex flex-col w-56 border-r border-border-secondary flex-shrink-0">
        <SidebarContent />
      </aside>

      {/* Mobile drawer overlay */}
      {drawerOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/60 lg:hidden"
          onClick={() => setDrawerOpen(false)}
          aria-hidden="true"
        />
      )}

      {/* Mobile drawer */}
      <aside
        className={cn(
          'fixed inset-y-0 left-0 z-50 w-56 flex flex-col lg:hidden transition-transform duration-300',
          drawerOpen ? 'translate-x-0' : '-translate-x-full',
        )}
      >
        <div className="flex items-center justify-end px-4 py-3 bg-surface-secondary border-b border-border-secondary">
          <button
            onClick={() => setDrawerOpen(false)}
            className="p-1 rounded-md text-text-secondary hover:text-text-primary transition-apple"
            aria-label="Close menu"
          >
            <X className="w-5 h-5" />
          </button>
        </div>
        <SidebarContent onNavigate={() => setDrawerOpen(false)} />
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Mobile top bar */}
        <header className="lg:hidden flex items-center gap-3 px-4 py-3 border-b border-border-secondary bg-surface-secondary">
          <button
            onClick={() => setDrawerOpen(true)}
            className="p-1.5 rounded-md text-text-secondary hover:text-text-primary transition-apple"
            aria-label="Open menu"
          >
            <Menu className="w-5 h-5" />
          </button>
          <p className="text-sm font-semibold text-text-primary">NexusMind</p>
        </header>

        <main className="flex-1 overflow-auto">
          {children}
        </main>
      </div>
    </div>
  )
}
