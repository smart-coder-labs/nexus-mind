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
  BookMarked,
  Zap,
  FolderOpen,
  Hash,
  Search,
  Megaphone,
  MessageSquare,
  Database,
  Network,
  Boxes,
  ListTodo,
  FileStack,
} from 'lucide-react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { cn } from '@/lib/utils'
import { CommandPalette } from './CommandPalette'
import { createClient } from '../api/client'
import type { OrgSettings } from '../types'
import { DISABLED_NAV_HREFS, NOTIFICATIONS_DISABLED } from '../config/disabled-sections'

const client = createClient()

const NOTIF_LAST_SEEN_KEY = 'nexusmind-notif-last-seen'

type NotifEventType = 'memory.created' | 'memory.updated' | 'memory.deleted' |
  'code.indexed' | 'user.disabled' | 'announcement'
const ALL_NOTIF_TYPES: NotifEventType[] = [
  'memory.created', 'memory.updated', 'memory.deleted',
  'code.indexed', 'user.disabled', 'announcement'
]
const NOTIF_PREFS_KEY = 'nexusmind-notif-prefs'

// Visible keyboard-focus indicator (accessibility floor, DESIGN_DIRECTION §6):
// 2px focus-ring outline with 2px offset. Applied to every interactive element.
const FOCUS_RING =
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

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
  { keys: ['⌘', 'N'], description: 'New memory' },
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
        className="bg-background-secondary rounded-[18px] border border-border-primary p-6 max-w-md w-full mx-4"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-[15px] font-semibold text-text-primary">Keyboard Shortcuts</h2>
          <button
            onClick={onClose}
            className={cn('rounded-[6px] text-text-tertiary hover:text-text-primary transition-colors', FOCUS_RING)}
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
                    className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5 font-mono text-[11px] text-text-tertiary border border-border-primary"
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
  adminOnly?: boolean
  requiredPermission?: string
}

interface NavGroup {
  label: string
  items: NavItem[]
}

// Sidebar nav grouped by domain (DESIGN_DIRECTION §4). adminOnly items are
// visible to admins only — this preserves the prior per-href visibility filter.
const NAV_GROUPS: NavGroup[] = [
  {
    label: 'Overview',
    items: [
      { label: 'Search',      href: '/search',      icon: Search },
      { label: 'Dashboard',   href: '/',            icon: LayoutDashboard },
    ],
  },
  {
    label: 'Knowledge',
    items: [
      { label: 'Memories',    href: '/memories',    icon: Brain },
      { label: 'Graph',       href: '/graph',       icon: Network },
      { label: 'Collections', href: '/collections', icon: FolderOpen,    adminOnly: true, requiredPermission: 'collection:read' },
      { label: 'Tags',        href: '/tags',        icon: Hash,          adminOnly: true, requiredPermission: 'tag:read' },
      { label: 'Conventions', href: '/conventions', icon: BookMarked,    adminOnly: true, requiredPermission: 'convention:read' },
      { label: 'Sessions',    href: '/sessions',    icon: MessageSquare },
      { label: 'Tasks',       href: '/tasks',       icon: ListTodo,      adminOnly: true, requiredPermission: 'task:read' },
      { label: 'SDD',         href: '/sdd',         icon: FileStack,     adminOnly: true, requiredPermission: 'sdd:read' },
    ],
  },
  {
    label: 'Code',
    items: [
      { label: 'Projects',    href: '/projects',    icon: FolderGit,     adminOnly: true, requiredPermission: 'project:read' },
      { label: 'Code',        href: '/code',        icon: Code2,         adminOnly: true, requiredPermission: 'code:read' },
      { label: 'Harnesses',   href: '/harnesses',   icon: Boxes,         adminOnly: true, requiredPermission: 'harness:read' },
    ],
  },
  {
    label: 'Access',
    items: [
      { label: 'Users',       href: '/users',       icon: Users,         adminOnly: true },
      { label: 'Roles',       href: '/roles',       icon: Shield,        adminOnly: true },
      { label: 'API Keys',    href: '/api-keys',    icon: Key,           adminOnly: true },
      { label: 'Agents',      href: '/agents',      icon: Bot,           adminOnly: true },
      { label: 'Policies',    href: '/policies',    icon: ShieldAlert,   adminOnly: true, requiredPermission: 'policy:read' },
    ],
  },
  {
    label: 'System',
    items: [
      { label: 'Webhooks',    href: '/webhooks',    icon: Zap,           adminOnly: true, requiredPermission: 'settings:write' },
      { label: 'Audit Log',   href: '/audit',       icon: ScrollText,    adminOnly: true, requiredPermission: 'audit:read' },
      { label: 'Backups',     href: '/backups',     icon: Database,      adminOnly: true },
      { label: 'Settings',    href: '/settings',    icon: Settings },
    ],
  },
]

function NavLinks({ onNavigate }: { onNavigate?: () => void }) {
  const location = useLocation()
  const { session } = useAuth()
  const isAdmin = session?.user.role === 'admin' || session?.user.role === 'super_user'

  return (
    <nav className="flex flex-col gap-4 px-2">
      {NAV_GROUPS.map(group => {
        const permissions = session?.user.permissions ?? []
        const items = group.items.filter(item => {
          if (DISABLED_NAV_HREFS.has(item.href)) return false
          if (!item.adminOnly) return true
          if (isAdmin) return true
          if (item.requiredPermission && permissions.includes(item.requiredPermission)) return true
          return false
        })
        if (items.length === 0) return null

        return (
          <div key={group.label} className="flex flex-col gap-0.5">
            <p className="px-2.5 pt-2 pb-1.5 text-[10.5px] font-bold uppercase tracking-[0.12em] text-[#5b6373]">
              {group.label}
            </p>
            {items.map(({ href, label, icon: Icon }) => {
              const isActive =
                href === '/'
                  ? location.pathname === '/'
                  : location.pathname.startsWith(href)

              return (
                <Link
                  key={href}
                  to={href}
                  onClick={onNavigate}
                  aria-current={isActive ? 'page' : undefined}
                  className={cn(
                    'group flex items-center gap-3 w-full px-2.5 py-[9px] rounded-[10px] text-[14px] transition-colors duration-150',
                    FOCUS_RING,
                    isActive
                      ? 'bg-white/[0.08] text-[#f2f4f8] font-semibold'
                      : 'text-[#9aa2b2] hover:text-[#e7eaf0] hover:bg-white/[0.05] font-normal',
                  )}
                >
                  <Icon
                    className={cn(
                      'w-[18px] h-[18px] flex-shrink-0 opacity-90',
                      isActive ? 'text-[#f2f4f8]' : 'text-[#9aa2b2] group-hover:text-[#e7eaf0]',
                    )}
                  />
                  {label}
                </Link>
              )
            })}
          </div>
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
    // TEMPORARY: disabled while NOTIFICATIONS_DISABLED is true
    enabled: !NOTIFICATIONS_DISABLED,
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
      {/* Org header — 36px round avatar + name, per the design shell */}
      <div className="px-4 pt-[18px] pb-3.5">
        <div className="flex items-center gap-3">
          {orgSettings?.logo_url ? (
            <img
              src={orgSettings.logo_url}
              className="w-9 h-9 rounded-full object-cover flex-shrink-0"
              alt="org logo"
            />
          ) : (
            <div className="w-9 h-9 rounded-full bg-[#e9edf3] flex items-center justify-center flex-shrink-0" aria-hidden="true">
              <div className="grid grid-cols-2 gap-[3px]">
                <div className="w-[7px] h-[7px] rounded-[2px] bg-[#22c55e]" />
                <div className="w-[7px] h-[7px] rounded-[2px] bg-[#16a34a]" />
                <div className="w-[7px] h-[7px] rounded-[2px] bg-[#16a34a]" />
                <div className="w-[7px] h-[7px] rounded-[2px] bg-[#22c55e]" />
              </div>
            </div>
          )}
          <div className="flex flex-col gap-0.5 min-w-0">
            <p className="text-[15px] font-bold tracking-[-0.01em] text-[#f2f4f8] truncate leading-tight">
              {session?.org.name ?? 'NexusMind'}
            </p>
            <p className="text-[12px] text-[#7c8496] leading-tight">nexusmind</p>
          </div>
        </div>
      </div>

      {/* Nav */}
      <div className="flex-1 overflow-y-auto py-2">
        <NavLinks onNavigate={onNavigate} />
      </div>

      {/* Bottom: notifications + sign out */}
      <div className="px-2.5 py-2.5 border-t border-white/[0.06] flex flex-col gap-0.5">
        {/* Notification bell — hidden while NOTIFICATIONS_DISABLED */}
        {!NOTIFICATIONS_DISABLED && <div className="relative" ref={notifRef}>
          <button
            onClick={handleNotifOpen}
            className={cn('flex items-center gap-3 w-full px-3 py-2 rounded-[8px] text-[13px] text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors duration-150', FOCUS_RING)}
            aria-label="Notifications"
          >
            <Bell className="w-4 h-4 shrink-0" />
            <span>Notifications</span>
            {unreadCount > 0 && (
              <span className="ml-auto w-5 h-5 rounded-full bg-status-error text-white text-[11px] font-semibold flex items-center justify-center shrink-0">
                {unreadCount > 9 ? '9+' : unreadCount}
              </span>
            )}
          </button>

          {notifOpen && (
            <div className="absolute bottom-full left-0 mb-2 w-72 bg-background-secondary border border-border-primary rounded-[18px] z-50 overflow-hidden">
              <div className="px-4 py-3 border-b border-border-secondary/50">
                <p className="text-xs font-semibold text-text-primary">Notifications</p>
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
                      <p className="text-[11px] text-text-tertiary mt-0.5">by {n.actor}</p>
                    )}
                    <p className="text-[11px] text-text-tertiary mt-0.5">
                      {relativeTime(n.created_at)}
                    </p>
                  </div>
                ))}
                <hr className="border-border-secondary/30 my-1" />
                <p className="text-[11px] text-text-tertiary uppercase tracking-wide font-semibold px-3 pb-1">Preferences</p>
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
        </div>}

        {/* Sign out + shortcuts */}
        <div className="flex items-center gap-1">
          <button
            onClick={handleLogout}
            className={cn('flex flex-1 items-center gap-3 px-2.5 py-[9px] rounded-[10px] text-[14px] text-[#9aa2b2] hover:text-[#e7eaf0] hover:bg-white/[0.05] transition-colors duration-150', FOCUS_RING)}
          >
            <LogOut className="w-[18px] h-[18px] flex-shrink-0" />
            Sign out
          </button>
          <button
            onClick={onOpenShortcuts}
            className={cn('p-2 rounded-[8px] text-text-secondary hover:text-text-primary transition-colors', FOCUS_RING)}
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
  const menuButtonRef = useRef<HTMLButtonElement>(null)

  const { data: orgSettings } = useQuery({
    queryKey: ['org-settings'],
    queryFn: () => client.getOrgSettings(),
    enabled: !!session,
    staleTime: 5 * 60_000,
  })

  const announcement = orgSettings?.announcement ?? ''
  const dismissKey = `nexusmind_announcement_dismissed_${announcement.slice(0, 20)}`
  const [dismissed, setDismissed] = useState(() => !!sessionStorage.getItem(dismissKey))

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        navigate('/search')
      }
      if (
        (e.metaKey || e.ctrlKey) &&
        e.key === 'n' &&
        !['INPUT', 'TEXTAREA'].includes((e.target as HTMLElement).tagName) &&
        !(e.target as HTMLElement).isContentEditable
      ) {
        e.preventDefault()
        navigate('/memories?new=1')
      }
      if (e.key === '?' && !['INPUT', 'TEXTAREA'].includes((e.target as HTMLElement).tagName)) {
        setShowShortcuts((prev) => !prev)
      }
      if (e.key === 'Escape') setShowShortcuts(false)
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [navigate])

  // Escape closes the mobile drawer and returns focus to the trigger.
  useEffect(() => {
    if (!drawerOpen) return
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setDrawerOpen(false)
        menuButtonRef.current?.focus()
      }
    }
    window.addEventListener('keydown', handleEscape)
    return () => window.removeEventListener('keydown', handleEscape)
  }, [drawerOpen])

  return (
    <div className="h-screen overflow-hidden bg-[#07080c] flex">
      {/* Skip to content — first tab stop, visible only when focused */}
      <a
        href="#main-content"
        className={cn(
          'sr-only focus:not-sr-only focus:absolute focus:top-3 focus:left-3 focus:z-[100] focus:px-4 focus:py-2 focus:rounded-[8px] focus:bg-accent-blue focus:text-white focus:text-[13px] focus:font-medium',
          FOCUS_RING,
        )}
      >
        Skip to content
      </a>

      {/* Desktop sidebar — floating glass panel (design shell: fixed inset
          16px, 244px wide, radius 16, blur 18) */}
      <aside className="hidden lg:flex flex-col fixed left-4 top-4 bottom-4 w-[244px] rounded-[16px] border border-white/[0.07] bg-[#0d0f14]/[0.72] backdrop-blur-[18px] shadow-[0_12px_40px_rgba(0,0,0,0.45)] z-30 overflow-hidden">
        <SidebarContent onOpenShortcuts={() => setShowShortcuts(true)} orgSettings={orgSettings} />
      </aside>
      <div className="hidden lg:block w-[276px] flex-shrink-0" />

      {/* Mobile overlay */}
      {drawerOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/50 backdrop-blur-sm lg:hidden"
          onClick={() => setDrawerOpen(false)}
          aria-hidden="true"
        />
      )}

      {/* Mobile drawer — inert + hidden from a11y tree while closed so its
          links are not reachable by keyboard or screen readers offscreen */}
      <aside
        inert={!drawerOpen}
        aria-hidden={!drawerOpen || undefined}
        className={cn(
          'fixed inset-y-0 left-0 z-50 w-[244px] flex flex-col bg-[#0d0f14]/95 backdrop-blur-[18px] border-r border-white/[0.07] lg:hidden transition-transform duration-200',
          drawerOpen ? 'translate-x-0' : '-translate-x-full',
        )}
      >
        <div className="flex items-center justify-end px-4 py-4">
          <button
            onClick={() => setDrawerOpen(false)}
            className={cn('p-1 rounded-[5px] text-text-tertiary hover:text-text-primary transition-colors', FOCUS_RING)}
            aria-label="Close menu"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <SidebarContent onNavigate={() => setDrawerOpen(false)} onOpenShortcuts={() => setShowShortcuts(true)} orgSettings={orgSettings} />
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Mobile top bar */}
        <header className="lg:hidden flex items-center gap-3 px-4 py-3 border-b border-white/[0.06] bg-black">
          <button
            ref={menuButtonRef}
            onClick={() => setDrawerOpen(true)}
            className={cn('p-1.5 rounded-[5px] text-text-secondary hover:text-text-primary transition-colors', FOCUS_RING)}
            aria-label="Open menu"
          >
            <Menu className="w-4 h-4" />
          </button>
          <div className="flex flex-col min-w-0">
            <p className="text-[13px] font-semibold text-text-primary truncate leading-tight">
              {session?.org.name ?? 'NexusMind'}
            </p>
            <p className="text-[10px] text-text-tertiary leading-tight flex items-center gap-1">
              <Brain className="w-3 h-3 text-accent-blue flex-shrink-0" />
              nexusmind
            </p>
          </div>
        </header>

        <main id="main-content" tabIndex={-1} className="flex-1 overflow-y-auto focus:outline-none">
          {announcement && !dismissed && (
            <div className="mx-6 mt-4 mb-0 flex items-start gap-3 rounded-[11px] border border-accent-blue/30 bg-accent-blue/[0.08] px-4 py-3">
              <Megaphone className="w-4 h-4 text-accent-blue mt-0.5 shrink-0" />
              <p className="flex-1 text-xs text-text-secondary leading-relaxed">{announcement}</p>
              <button
                onClick={() => {
                  setDismissed(true)
                  sessionStorage.setItem(`nexusmind_announcement_dismissed_${announcement.slice(0, 20)}`, '1')
                }}
                className={cn('rounded-[5px] text-text-tertiary hover:text-text-primary transition-colors shrink-0', FOCUS_RING)}
                aria-label="Dismiss announcement"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
          )}
          {children}
        </main>
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      {showShortcuts && <ShortcutsPanel onClose={() => setShowShortcuts(false)} />}
    </div>
  )
}
