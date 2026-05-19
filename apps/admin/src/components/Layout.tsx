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
  icon: React.ComponentType<{ className?: string }>
}

const NAV_ITEMS: NavItem[] = [
  { label: 'Dashboard', href: '/',         icon: LayoutDashboard },
  { label: 'Users',     href: '/users',    icon: Users },
  { label: 'Memories',  href: '/memories', icon: Brain },
  { label: 'Audit Log', href: '/audit',    icon: ScrollText },
  { label: 'Settings',  href: '/settings', icon: Settings },
]

function NavLinks({ onNavigate }: { onNavigate?: () => void }) {
  const location = useLocation()

  return (
    <nav className="flex flex-col gap-0.5 px-2">
      {NAV_ITEMS.map(({ href, label, icon: Icon }) => {
        const isActive =
          href === '/'
            ? location.pathname === '/'
            : location.pathname.startsWith(href)

        return (
          <Link
            key={href}
            to={href}
            onClick={onNavigate}
            className={cn(
              'group flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue/40',
              isActive
                ? 'bg-surface-secondary text-text-primary font-medium'
                : 'text-text-secondary hover:text-text-primary hover:bg-surface-secondary/60 font-normal',
            )}
          >
            <Icon
              className={cn(
                'w-[15px] h-[15px] flex-shrink-0',
                isActive ? 'text-accent-blue' : 'text-text-tertiary group-hover:text-text-secondary',
              )}
            />
            {label}
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
    <div className="flex flex-col h-full">
      {/* Logo */}
      <div className="px-5 pt-6 pb-5">
        <p className="text-sm font-semibold text-text-primary">NexusMind</p>
        <p className="text-xs text-text-tertiary mt-0.5 truncate">{session?.org.name}</p>
      </div>

      {/* Nav */}
      <div className="flex-1 overflow-y-auto py-2">
        <NavLinks onNavigate={onNavigate} />
      </div>

      {/* Bottom: sign out */}
      <div className="px-2 pb-3">
        <button
          onClick={handleLogout}
          className="flex w-full items-center gap-3 px-3 py-2 rounded-lg text-sm text-text-secondary hover:text-text-primary hover:bg-surface-secondary/60 transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue/40"
        >
          <LogOut className="w-[15px] h-[15px] flex-shrink-0 text-text-tertiary" />
          Sign out
        </button>
      </div>
    </div>
  )
}

export function Layout({ children }: { children: React.ReactNode }) {
  const [drawerOpen, setDrawerOpen] = useState(false)

  return (
    <div className="min-h-screen bg-bg-secondary flex">
      {/* Desktop sidebar */}
      <aside className="hidden lg:flex flex-col fixed inset-y-0 left-0 w-52 border-r border-border-primary bg-bg-primary z-30">
        <SidebarContent />
      </aside>
      <div className="hidden lg:block w-52 flex-shrink-0" />

      {/* Mobile overlay */}
      {drawerOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/50 backdrop-blur-sm lg:hidden"
          onClick={() => setDrawerOpen(false)}
          aria-hidden="true"
        />
      )}

      {/* Mobile drawer */}
      <aside
        className={cn(
          'fixed inset-y-0 left-0 z-50 w-52 flex flex-col bg-bg-primary border-r border-border-primary lg:hidden transition-transform duration-200',
          drawerOpen ? 'translate-x-0' : '-translate-x-full',
        )}
      >
        <div className="flex items-center justify-end px-4 py-4">
          <button
            onClick={() => setDrawerOpen(false)}
            className="p-1 rounded-md text-text-tertiary hover:text-text-secondary transition-colors"
            aria-label="Close menu"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <SidebarContent onNavigate={() => setDrawerOpen(false)} />
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Mobile top bar */}
        <header className="lg:hidden flex items-center gap-3 px-4 py-3 border-b border-border-primary bg-bg-primary">
          <button
            onClick={() => setDrawerOpen(true)}
            className="p-1.5 text-text-secondary hover:text-text-primary transition-colors"
            aria-label="Open menu"
          >
            <Menu className="w-4 h-4" />
          </button>
          <p className="text-sm font-medium text-text-primary">NexusMind</p>
        </header>

        <main className="flex-1 overflow-auto">
          {children}
        </main>
      </div>
    </div>
  )
}
