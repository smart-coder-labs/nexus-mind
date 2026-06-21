import { useState, useEffect, useRef } from 'react'
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
  Shield,
  ShieldAlert,
  FolderGit,
  Code2,
  Key,
  Bot,
  Bell,
  Keyboard,
  AlertCircle,
  BookMarked,
  Zap,
  FolderOpen,
  Hash,
  Search,
} from 'lucide-react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { cn } from '@/lib/utils'
import { CommandPalette } from './CommandPalette'
import { createClient } from '../api/client'
import type { OrgSettings } from '../types'

const client = createClient()

const NOTIF_LAST_SEEN_KEY = 'nexusmind-notif-last-seen'

type NotifEventType = 'memory.created' | 'memory.updated' | 'memory.deleted' |
  'code.indexed' | 'user.disabled' | 'announcement'
const ALL_NOTIF_TYPES: NotifEventType[] = [
  'memory.created', 'memory.updated', 'memory.deleted',
  'code.indexed', 'user.disabled', 'announcement'
]
const NOTIF_PREFS_KEY = 'nexusmind-notif-prefs'

function relativeTime(isoString: string): string {
  const now = Date.now()
  const then = new Date(isoString).getTime()
  const diff = Math.floor((now - then) / 1000)
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

const SHORTCUTS = [
  { keys: ['⌘', 'K'], description: 'Open command palette' },
  { keys: ['?'], description: 'Open keyboard shortcuts' },
  { keys: ['Esc'], description: 'Close panels / cancel' },
  { keys: ['↑', '↓'], description: 'Navigate suggestions' },
  { keys: ['Enter'], description: 'Confirm / select' },
  { keys: ['⌘', 'Enter'], description: 'Submit forms' },
]

function ShortcutsPanel({ onClose }: { onClose: () => void }) {
  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label="Keyboard shortcuts"
    >
      <div
        className="bg-[#272729] rounded-[18px] border border-border-primary p-6 max-w-md w-full shadow-2xl mx-4"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-sm font-semibold text-text-primary">Keyboard Shortcuts</h2>
          <button
            onClick={onClose}
            className="text-text-quaternary hover:text-text-secondary transition-colors"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div>
          {SHORTCUTS.map(({ keys, description }) => (
            <div
              key={description}
              className="flex items-center justify-between py-2 border-b border-border-secondary/30 last:border-b-0"
            >
              <span className="text-xs text-text-secondary">{description}</span>
              <div className="flex items-center gap-1">
                {keys.map((k, i) => (
                  <kbd
                    key={i}
                    className="border border-border-primary rounded-[5px] px-1.5 py-0.5 text-[10px] text-text-quaternary font-mono bg-white/[0.04]"
                  >
                    {k}
                  </kbd>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

interface NavItem {
  label: string
  href: string
  icon: React.ComponentType<{ className?: string }>
}

const NAV_ITEMS: NavItem[] = [
  { label: 'Search',       href: '/search',       icon: Search },
  { label: 'Dashboard',    href: '/',             icon: LayoutDashboard },
  { label: 'Users',        href: '/users',        icon: Users },
  { label: 'Roles',        href: '/roles',        icon: Shield },
  { label: 'Projects',     href: '/projects',     icon: FolderGit },
  { label: 'Code',         href: '/code',         icon: Code2 },
  { label: 'API Keys',     href: '/api-keys',     icon: Key },
  { label: 'Agents',       href: '/agents',       icon: Bot },
  { label: 'Conventions',  href: '/conventions',  icon: BookMarked },
  { label: 'Policies',     href: '/policies',     icon: ShieldAlert },
  { label: 'Webhooks',     href: '/webhooks',     icon: Zap },
  { label: 'Memories',     href: '/memories',     icon: Brain },
  { label: 'Tags',         href: '/tags',         icon: Hash },
  { label: 'Collections',  href: '/collections',  icon: FolderOpen },
  { label: 'Audit Log',    href: '/audit',        icon: ScrollText },
  { label: 'Settings',     href: '/settings',     icon: Settings },
]

function NavLinks({ onNavigate }: { onNavigate?: () => void }) {
  const location = useLocation()
  const { session } = useAuth()

  const visibleItems = NAV_ITEMS.filter(item => {
    if (
      item.href === '/users' ||
      item.href === '/audit' ||
      item.href === '/roles' ||
      item.href === '/projects' ||
      item.href === '/code' ||
      item.href === '/api-keys' ||
      item.href === '/agents' ||
      item.href === '/conventions' ||
      item.href === '/policies' ||
      item.href === '/webhooks' ||
      item.href === '/collections' ||
      item.href === '/tags'
    ) {
      return session?.user.role === 'admin'
    }
    return true
  })

  return (
    <nav className="flex flex-col gap-0.5 px-2">
      {visibleItems.map(({ href, label, icon: Icon }) => {
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
              'relative group flex items-center gap-3 w-full px-3 py-2 rounded-[8px] text-sm transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue/40',
              isActive
                ? 'bg-accent-blue/10 text-accent-blue font-semibold before:absolute before:left-0 before:top-1/2 before:-translate-y-1/2 before:h-4 before:w-0.5 before:bg-accent-blue before:rounded-full'
                : 'text-text-secondary hover:text-text-primary hover:bg-white/[0.04] font-normal',
            )}
          >
            <Icon
              className={cn(
                'w-[15px] h-[15px] flex-shrink-0',
                isActive ? 'text-accent-blue' : 'text-text-quaternary group-hover:text-text-tertiary',
              )}
            />
            {label}
          </Link>
        )
      })}
    </nav>
  )
}

function SidebarContent({ onNavigate, onOpenShortcuts, orgSettings }: { onNavigate?: () => void; onOpenShortcuts?: () => void; orgSettings?: OrgSettings }) {
  const { session, logout } = useAuth()
  const navigate = useNavigate()
  const [notifOpen, setNotifOpen] = useState(false)
  const [lastSeenAt, setLastSeenAt] = useState<string>(
    () => localStorage.getItem(NOTIF_LAST_SEEN_KEY) ?? new Date(0).toISOString()
  )
  const [enabledTypes, setEnabledTypes] = useState<Set<NotifEventType>>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(NOTIF_PREFS_KEY) ?? 'null')
      if (Array.isArray(saved)) return new Set(saved as NotifEventType[])
    } catch {}
    return new Set(ALL_NOTIF_TYPES)
  })
  const notifRef = useRef<HTMLDivElement>(null)

  const { data: notifications } = useQuery({
    queryKey: ['notifications'],
    queryFn: () => client.getNotifications(),
    refetchInterval: 60000,
  })

  const toggleType = (type: NotifEventType) => {
    setEnabledTypes(prev => {
      const next = new Set(prev)
      if (next.has(type)) next.delete(type)
      else next.add(type)
      localStorage.setItem(NOTIF_PREFS_KEY, JSON.stringify(Array.from(next)))
      return next
    })
  }

  const visibleItems = (notifications ?? []).filter(
    item => !(item as { event_type?: string }).event_type ||
      enabledTypes.has((item as { event_type?: string }).event_type as NotifEventType)
  )

  const unreadCount = visibleItems.filter(
    (n) => new Date(n.created_at) > new Date(lastSeenAt)
  ).length

  // Close dropdown when clicking outside.
  useEffect(() => {
    if (!notifOpen) return
    const handler = (e: MouseEvent) => {
      if (notifRef.current && !notifRef.current.contains(e.target as Node)) {
        setNotifOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [notifOpen])

  const handleLogout = () => {
    logout()
    navigate('/login')
    onNavigate?.()
  }

  const handleNotifOpen = () => {
    const now = new Date().toISOString()
    setNotifOpen((o) => {
      if (!o) {
        setLastSeenAt(now)
        localStorage.setItem(NOTIF_LAST_SEEN_KEY, now)
      }
      return !o
    })
  }

  return (
    <div className="flex flex-col h-full">
      {/* Logo */}
      <div className="px-5 pt-6 pb-5">
        <div className="flex items-center gap-2">
          <Brain className="w-4 h-4 text-accent-blue flex-shrink-0" />
          <p className="text-sm font-semibold text-text-primary">NexusMind</p>
        </div>
        {session?.org.name && (
          <div className="mt-2 flex items-center gap-1.5">
            {orgSettings?.logo_url && (
              <img
                src={orgSettings.logo_url}
                className="w-6 h-6 rounded-full object-cover flex-shrink-0"
                alt="org logo"
              />
            )}
            <span className="inline-block bg-[#272729] rounded-full px-2 py-0.5 text-[11px] text-text-tertiary truncate max-w-full">
              {session.org.name}
            </span>
          </div>
        )}
      </div>

      <div className="border-b border-border-secondary mx-3" />

      {/* Nav */}
      <div className="flex-1 overflow-y-auto py-2">
        <NavLinks onNavigate={onNavigate} />
      </div>

      {/* Bottom: notifications + sign out */}
      <div className="px-2 pb-3 flex flex-col gap-0.5">
        {/* Notification bell */}
        <div className="relative" ref={notifRef}>
          <button
            onClick={handleNotifOpen}
            className="flex items-center gap-3 w-full px-3 py-2 rounded-[8px] text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue/40"
            aria-label="Notifications"
          >
            <Bell className="w-4 h-4 shrink-0" />
            <span className="text-sm">Notifications</span>
            {unreadCount > 0 && (
              <span className="ml-auto w-5 h-5 rounded-full bg-status-error text-white text-[9px] font-semibold flex items-center justify-center shrink-0">
                {unreadCount > 9 ? '9+' : unreadCount}
              </span>
            )}
          </button>

          {notifOpen && (
            <div className="absolute bottom-full left-0 mb-2 w-72 bg-[#272729] border border-border-primary rounded-[18px] shadow-xl z-50 overflow-hidden">
              <div className="px-4 py-3 border-b border-border-secondary/50">
                <p className="text-sm font-semibold text-text-primary">Notifications</p>
              </div>
              <div className="max-h-80 overflow-y-auto">
                {visibleItems.length === 0 && (
                  <p className="text-xs text-text-quaternary text-center py-8">No recent activity</p>
                )}
                {visibleItems.map((n) => (
                  <div
                    key={n.id}
                    className="px-4 py-3 border-b border-border-secondary/30 last:border-b-0 hover:bg-white/[0.02]"
                  >
                    <p className="text-xs text-text-secondary">{n.message}</p>
                    {n.actor && (
                      <p className="text-[10px] text-text-quaternary mt-0.5">by {n.actor}</p>
                    )}
                    <p className="text-[10px] text-text-quaternary mt-0.5">
                      {relativeTime(n.created_at)}
                    </p>
                  </div>
                ))}
                <hr className="border-border-secondary/30 my-1" />
                <p className="text-[10px] text-text-quaternary uppercase tracking-wide font-semibold px-3 pb-1">Preferences</p>
                {ALL_NOTIF_TYPES.map(type => (
                  <label key={type} className="flex items-center gap-2 px-3 py-1.5 cursor-pointer hover:bg-white/[0.04] rounded-[8px]">
                    <input
                      type="checkbox"
                      checked={enabledTypes.has(type)}
                      onChange={() => toggleType(type)}
                      className="accent-accent-blue w-3 h-3"
                    />
                    <span className="text-xs text-text-secondary capitalize">{type.replace('.', ' ')}</span>
                  </label>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Sign out + shortcuts */}
        <div className="flex items-center gap-1">
          <button
            onClick={handleLogout}
            className="flex flex-1 items-center gap-3 px-3 py-2 rounded-[8px] text-sm text-text-secondary hover:text-status-error hover:bg-[#272729]/60 transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue/40"
          >
            <LogOut className="w-[15px] h-[15px] flex-shrink-0 text-text-tertiary" />
            Sign out
          </button>
          <button
            onClick={onOpenShortcuts}
            className="p-2 rounded-[8px] text-text-quaternary hover:text-text-secondary transition-colors"
            title="Keyboard shortcuts (?)"
            aria-label="Keyboard shortcuts"
          >
            <Keyboard className="w-4 h-4" />
          </button>
        </div>
      </div>
    </div>
  )
}

export function Layout({ children }: { children: React.ReactNode }) {
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [showShortcuts, setShowShortcuts] = useState(false)
  const { session } = useAuth()
  const navigate = useNavigate()

  const { data: orgSettings, refetch: refetchSettings } = useQuery({
    queryKey: ['org-settings'],
    queryFn: () => client.getOrgSettings(),
    enabled: !!session,
    staleTime: 60000,
  })

  const announcement = orgSettings?.announcement ?? null
  const announcementType = orgSettings?.announcement_type ?? 'info'

  const handleClearAnnouncement = async () => {
    await client.updateAnnouncement('', 'info')
    refetchSettings()
  }

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        navigate('/search')
      }
      if (e.key === '?' && !['INPUT', 'TEXTAREA'].includes((e.target as HTMLElement).tagName)) {
        setShowShortcuts((prev) => !prev)
      }
      if (e.key === 'Escape') setShowShortcuts(false)
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [navigate])

  return (
    <div className="h-screen overflow-hidden bg-[#1d1d1f] flex">
      {/* Desktop sidebar */}
      <aside className="hidden lg:flex flex-col fixed inset-y-0 left-0 w-52 border-r border-white/[0.06] bg-black z-30">
        <SidebarContent onOpenShortcuts={() => setShowShortcuts(true)} orgSettings={orgSettings} />
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
          'fixed inset-y-0 left-0 z-50 w-52 flex flex-col bg-black border-r border-white/[0.06] lg:hidden transition-transform duration-200',
          drawerOpen ? 'translate-x-0' : '-translate-x-full',
        )}
      >
        <div className="flex items-center justify-end px-4 py-4">
          <button
            onClick={() => setDrawerOpen(false)}
            className="p-1 rounded-[5px] text-text-tertiary hover:text-text-secondary transition-colors"
            aria-label="Close menu"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <SidebarContent onNavigate={() => setDrawerOpen(false)} onOpenShortcuts={() => setShowShortcuts(true)} orgSettings={orgSettings} />
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Announcement Banner */}
        {announcement && (
          <div className={cn(
            'w-full px-5 py-2.5 text-xs flex items-center gap-2',
            announcementType === 'error'
              ? 'bg-status-error/10 text-status-error border-b border-status-error/20'
              : announcementType === 'warning'
              ? 'bg-status-warning/10 text-status-warning border-b border-status-warning/20'
              : 'bg-accent-blue/10 text-accent-blue border-b border-accent-blue/20',
          )}>
            <AlertCircle className="w-3 h-3 shrink-0" />
            <span className="flex-1">{announcement}</span>
            {session?.user.role === 'admin' && (
              <button
                onClick={handleClearAnnouncement}
                className="ml-auto shrink-0 text-current opacity-60 hover:opacity-100 transition-opacity"
                aria-label="Dismiss announcement"
              >
                <X className="w-3 h-3" />
              </button>
            )}
          </div>
        )}

        {/* Mobile top bar */}
        <header className="lg:hidden flex items-center gap-3 px-4 py-3 border-b border-white/[0.06] bg-black">
          <button
            onClick={() => setDrawerOpen(true)}
            className="p-1.5 text-text-secondary hover:text-text-primary transition-colors"
            aria-label="Open menu"
          >
            <Menu className="w-4 h-4" />
          </button>
          <div className="flex items-center gap-2">
            <Brain className="w-4 h-4 text-accent-blue flex-shrink-0" />
            <p className="text-sm font-semibold text-text-primary">NexusMind</p>
          </div>
          {session?.org.name && (
            <span className="ml-1 text-[11px] text-text-tertiary truncate">{session.org.name}</span>
          )}
        </header>

        <main className="flex-1 overflow-y-auto">
          {children}
        </main>
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      {showShortcuts && <ShortcutsPanel onClose={() => setShowShortcuts(false)} />}
    </div>
  )
}
