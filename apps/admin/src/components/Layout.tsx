import { useState, useEffect, useRef } from 'react'
import { useLocation, useNavigate, Link } from 'react-router-dom'
import {
  LayoutDashboard,
  Users,
  Brain,
  ScrollText,
  Settings,
  LogOut,
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
  Building2,
  BarChart3,
  Inbox,
  Radio,
  ShieldCheck,
  PlayCircle,
  Flag,
  Scale,
  SlidersHorizontal,
  ChevronDown,
} from 'lucide-react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { cn } from '@/lib/utils'
import { CommandPalette } from './CommandPalette'
import { SidebarProvider, Sidebar, SidebarInset, SidebarMenuButton, useSidebar } from './ui/shadcn/sidebar'
import { AdminThemeButton } from './AdminThemeButton'
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
        className="border border-border-primary bg-surface-elevated rounded-xl p-6 max-w-md w-full mx-4"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-[15px] font-semibold text-text-primary">Keyboard Shortcuts</h2>
          <button
            onClick={onClose}
            className={cn('rounded-sm text-text-tertiary hover:text-text-primary transition-colors', FOCUS_RING)}
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
                    className="rounded-sm bg-foreground/[0.06] px-1.5 py-0.5 font-mono text-[11px] text-text-tertiary border border-border-primary"
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
      { label: 'Usage',       href: '/usage',       icon: BarChart3,     adminOnly: true },
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
      { label: 'Migration',   href: '/migrations',  icon: BookMarked,    adminOnly: true, requiredPermission: 'migration:read' },
      { label: 'Sessions',    href: '/sessions',    icon: MessageSquare },
      { label: 'Tasks',       href: '/tasks',       icon: ListTodo,      adminOnly: true, requiredPermission: 'task:read' },
      { label: 'SDD',         href: '/sdd',         icon: FileStack,     adminOnly: true, requiredPermission: 'sdd:read' },
    ],
  },
  {
    label: 'Code',
    items: [
      { label: 'Clients',     href: '/clients',     icon: Building2,     adminOnly: true, requiredPermission: 'client:read' },
      { label: 'Projects',    href: '/projects',    icon: FolderGit,     adminOnly: true, requiredPermission: 'project:read' },
      { label: 'Code',        href: '/code',        icon: Code2,         adminOnly: true, requiredPermission: 'code:read' },
      { label: 'Harnesses',   href: '/harnesses',   icon: Boxes,         adminOnly: true, requiredPermission: 'harness:read' },
    ],
  },
  {
    // The software factory: what autonomous agents do (operate) and what they
    // may do (govern). One group so "agent" means one thing in the nav.
    label: 'Factory',
    items: [
      { label: 'Agents',            href: '/factory',                icon: Bot,               adminOnly: true, requiredPermission: 'autonomous_agent:read' },
      { label: 'Runs',              href: '/factory/runs',           icon: PlayCircle,        adminOnly: true, requiredPermission: 'autonomous_agent:read' },
      { label: 'Findings',          href: '/factory/findings',       icon: Flag,              adminOnly: true, requiredPermission: 'autonomous_agent:read' },
      { label: 'Needs a human',     href: '/factory/inbox',          icon: Inbox,             adminOnly: true, requiredPermission: 'factory_policy:read' },
      { label: 'Intake',            href: '/factory/intake',         icon: Radio,             adminOnly: true, requiredPermission: 'factory_policy:read' },
      { label: 'Autonomy policies', href: '/factory/policies',       icon: ShieldCheck,       adminOnly: true, requiredPermission: 'factory_policy:read' },
      { label: 'Decision model',    href: '/factory/decision-model', icon: Scale,             adminOnly: true, requiredPermission: 'factory_policy:read' },
      { label: 'Factory settings',  href: '/factory/settings',       icon: SlidersHorizontal, adminOnly: true, requiredPermission: 'autonomous_agent:read' },
    ],
  },
  {
    label: 'Access',
    items: [
      { label: 'Users',       href: '/users',       icon: Users,         adminOnly: true },
      { label: 'Roles',       href: '/roles',       icon: Shield,        adminOnly: true },
      { label: 'API Keys',    href: '/api-keys',    icon: Key,           adminOnly: true },
      { label: 'Agent identities', href: '/agents', icon: Bot,           adminOnly: true },
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
  const { state: sidebarState } = useSidebar()
  const { session } = useAuth()
  const isAdmin = session?.user.role === 'admin' || session?.user.role === 'super_user'
  const activeGroup = NAV_GROUPS.find(group => group.items.some(item => {
    if (item.href === '/' || item.href === '/factory') return location.pathname === item.href
    return location.pathname === item.href || location.pathname.startsWith(`${item.href}/`)
  }))?.label
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => new Set(NAV_GROUPS.map(group => group.label)))

  useEffect(() => {
    if (activeGroup) {
      setExpandedGroups(current => current.has(activeGroup) ? current : new Set(current).add(activeGroup))
    }
  }, [activeGroup])

  const toggleGroup = (label: string) => {
    setExpandedGroups(current => {
      const next = new Set(current)
      if (next.has(label)) next.delete(label)
      else next.add(label)
      return next
    })
  }

  return (
    <nav className="flex flex-col gap-2 px-2">
      {NAV_GROUPS.map(group => {
        const permissions = session?.user.permissions ?? []
        const items = group.items.filter(item => {
          if (DISABLED_NAV_HREFS.has(item.href)) return false
          if (item.requiredPermission) return permissions.includes(item.requiredPermission)
          if (!item.adminOnly) return true
          return isAdmin
        })
        if (items.length === 0) return null

        const isExpanded = expandedGroups.has(group.label)
        const contentsVisible = isExpanded || sidebarState === 'collapsed'
        const hasActiveItem = items.some(({ href }) =>
          href === '/' || href === '/factory'
            ? location.pathname === href
            : location.pathname === href || location.pathname.startsWith(`${href}/`)
        )
        const groupId = `sidebar-group-${group.label.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`

        return (
          <div key={group.label} className="flex flex-col">
            <button
              type="button"
              className={cn(
                'group-data-[collapsible=icon]:hidden flex min-h-10 w-full items-center justify-between gap-2 rounded-md border border-transparent px-3 py-2 text-left text-sm font-medium transition-colors duration-150 hover:bg-foreground/[0.04] hover:text-text-primary',
                FOCUS_RING,
                hasActiveItem ? 'text-text-primary' : 'text-text-tertiary',
              )}
              aria-expanded={isExpanded}
              aria-controls={groupId}
              onClick={() => toggleGroup(group.label)}
            >
              <span>{group.label}</span>
              <ChevronDown
                aria-hidden="true"
                className={cn('size-4 shrink-0 transition-transform duration-200 ease-out motion-reduce:transition-none', isExpanded && 'rotate-180')}
              />
            </button>
            <div
              id={groupId}
                aria-hidden={!contentsVisible}
              className={cn(
                'grid transition-[grid-template-rows,opacity] duration-300 ease-in-out motion-reduce:transition-none',
                contentsVisible ? 'grid-rows-[1fr] opacity-100' : 'pointer-events-none grid-rows-[0fr] opacity-0',
              )}
            >
              <div className="min-h-0 overflow-hidden">
                <div className="flex flex-col gap-0.5 pb-1">
            {items.map(({ href, label, icon: Icon }) => {
              // Section roots that have sibling routes under them match exactly,
              // so /factory/runs does not also light up /factory.
              const isActive =
                href === '/' || href === '/factory'
                  ? location.pathname === href
                  : location.pathname === href || location.pathname.startsWith(`${href}/`)

              return (
                <SidebarMenuButton key={href} asChild tooltip={label} isActive={isActive} className="group-data-[collapsible=icon]:justify-center">
                <Link
                  to={href}
                  onClick={onNavigate}
                  aria-label={label}
                  aria-current={isActive ? 'page' : undefined}
                  tabIndex={contentsVisible ? undefined : -1}
                  className={cn(
                    'group flex items-center gap-2 w-full h-8 border border-transparent px-3 py-2 rounded-md text-sm transition-colors duration-150',
                    FOCUS_RING,
                    isActive
                      ? 'bg-sidebar-accent text-sidebar-accent-foreground font-medium'
                      : 'text-text-secondary hover:text-text-primary hover:bg-foreground/[0.05] font-normal',
                  )}
                >
                  <Icon
                    className={cn(
                      'w-4 h-4 flex-shrink-0 opacity-90',
                      isActive ? 'text-text-primary' : 'text-text-secondary group-hover:text-text-primary',
                    )}
                  />
                  <span className="group-data-[collapsible=icon]:hidden">{label}</span>
                </Link>
                </SidebarMenuButton>
              )
            })}
                </div>
              </div>
            </div>
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
      <div className="px-4 pt-[18px] pb-3.5 group-data-[collapsible=icon]:px-1 group-data-[collapsible=icon]:pt-3">
        <div className="flex items-center gap-3">
          {orgSettings?.logo_url ? (
            <img
              src={orgSettings.logo_url}
              className="w-9 h-9 rounded-full object-cover flex-shrink-0"
              alt="org logo"
            />
          ) : (
            <div className="w-9 h-9 rounded-full bg-action-primary/10 flex items-center justify-center flex-shrink-0" aria-hidden="true">
              <div className="grid grid-cols-2 gap-[3px]">
                <div className="w-[7px] h-[7px] rounded-[2px] bg-action-primary" />
                <div className="w-[7px] h-[7px] rounded-[2px] bg-action-primary" />
                <div className="w-[7px] h-[7px] rounded-[2px] bg-action-primary" />
                <div className="w-[7px] h-[7px] rounded-[2px] bg-action-primary" />
              </div>
            </div>
          )}
          <div className="flex flex-col gap-0.5 min-w-0 group-data-[collapsible=icon]:hidden">
            <p className="text-[15px] font-bold tracking-[-0.01em] text-text-primary truncate leading-tight">
              {session?.org.name ?? 'NexusMind'}
            </p>
            <p className="text-[12px] text-text-tertiary leading-tight">nexusmind</p>
          </div>
        </div>
      </div>

      {/* Nav */}
      <div className="audit-sidebar-scroll min-h-0 flex-1 overflow-y-auto py-2">
        <NavLinks onNavigate={onNavigate} />
      </div>

      {/* Bottom: notifications + sign out */}
      <div className="group-data-[collapsible=icon]:hidden px-2.5 py-2.5 border-t border-border-primary flex flex-col gap-0.5">
        {/* Notification bell — hidden while NOTIFICATIONS_DISABLED */}
        {!NOTIFICATIONS_DISABLED && <div className="relative" ref={notifRef}>
          <button
            onClick={handleNotifOpen}
            className={cn('flex items-center gap-3 w-full px-3 py-2 rounded-md text-sm text-text-secondary hover:text-text-primary hover:bg-foreground/[0.04] transition-colors duration-150', FOCUS_RING)}
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
            <div className="absolute bottom-full left-0 mb-2 w-72 border border-border-primary bg-surface-elevated shadow-md rounded-xl z-50 overflow-hidden">
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
                    className="px-4 py-3 border-b border-border-secondary/30 last:border-b-0 hover:bg-foreground/[0.05]"
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
                  <label key={type} className="flex items-center gap-[10px] px-[11px] py-[9px] cursor-pointer hover:bg-foreground/[0.06] rounded-md">
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
            className={cn('flex flex-1 items-center gap-3 px-2 py-2 rounded-md text-sm text-text-secondary hover:text-text-primary hover:bg-foreground/[0.05] transition-colors duration-150', FOCUS_RING)}
          >
            <LogOut className="w-4 h-4 flex-shrink-0" />
            Sign out
          </button>
          <div className="flex items-center gap-1">
            <button
              onClick={onOpenShortcuts}
              className={cn('p-2 rounded-md text-text-secondary hover:text-text-primary transition-colors', FOCUS_RING)}
              title="Keyboard shortcuts (?)"
              aria-label="Keyboard shortcuts"
            >
              <Keyboard className="w-4 h-4" />
            </button>
            <AdminThemeButton compact />
          </div>
        </div>
      </div>
    </div>
  )
}

export function Layout({ children }: { children: React.ReactNode }) {
 return <SidebarProvider style={{'--sidebar-width':'288px',maxHeight:'100dvh',maxWidth:'100vw'} as React.CSSProperties} className="h-dvh overflow-hidden"><LayoutBody>{children}</LayoutBody></SidebarProvider>
}
function LayoutBody({ children }: { children: React.ReactNode }) {
 const { setOpenMobile } = useSidebar()
  const workspaceRef = useRef<HTMLDivElement>(null)
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [showShortcuts, setShowShortcuts] = useState(false)
  const [scrollFades, setScrollFades] = useState({ top: false, bottom: false, bottomOffset: 0 })
  const { session } = useAuth()
  const navigate = useNavigate()

  const { data: orgSettings } = useQuery({
    queryKey: ['org-settings'],
    queryFn: () => client.getOrgSettings(),
    enabled: !!session,
    staleTime: 5 * 60_000,
  })

  const announcement = orgSettings?.announcement ?? ''
  const announcementType = orgSettings?.announcement_type
  const announcementTone = announcementType === 'error'
    ? { border: 'border-status-error', icon: 'text-status-error', surface: 'bg-status-error/5' }
    : announcementType === 'warning'
      ? { border: 'border-status-warning', icon: 'text-status-warning', surface: 'bg-status-warning/5' }
      : { border: 'border-action-primary', icon: 'text-action-primary', surface: 'bg-action-primary/5' }
  const dismissKey = `nexusmind_announcement_dismissed_${announcement.slice(0, 20)}`
  const [dismissed, setDismissed] = useState(() => !!sessionStorage.getItem(dismissKey))

  useEffect(() => {
    const workspace = workspaceRef.current
    if (!workspace) return
    const updateFades = () => {
      const top = workspace.scrollTop > 8
      const bottom = workspace.scrollTop + workspace.clientHeight < workspace.scrollHeight - 8
      const bottomOffset = Math.max(0, workspace.clientHeight - 104)
      setScrollFades((current) => current.top === top && current.bottom === bottom && current.bottomOffset === bottomOffset
        ? current
        : { top, bottom, bottomOffset })
    }
    updateFades()
    workspace.addEventListener('scroll', updateFades, { passive: true })
    const resizeObserver = new ResizeObserver(updateFades)
    resizeObserver.observe(workspace)
    const page = workspace.querySelector('.admin-page')
    if (page) resizeObserver.observe(page)
    return () => {
      workspace.removeEventListener('scroll', updateFades)
      resizeObserver.disconnect()
    }
  }, [])

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

  return (
    <>
      {/* Skip to content — first tab stop, visible only when focused */}
      <a
        href="#main-content"
        className={cn(
          'sr-only focus:not-sr-only focus:absolute focus:top-3 focus:left-3 focus:z-[100] focus:px-4 focus:py-2 focus:rounded-md focus:bg-action-primary focus:text-action-foreground focus:text-sm focus:font-medium',
          FOCUS_RING,
        )}
      >
        Skip to content
      </a>

      <Sidebar collapsible="icon" variant="inset" className="audit-sidebar">
        <SidebarContent onNavigate={() => setOpenMobile(false)} onOpenShortcuts={() => setShowShortcuts(true)} orgSettings={orgSettings} />
      </Sidebar>
      <SidebarInset className="min-w-0 overflow-hidden mb-2.5">
        <div ref={workspaceRef} role="region" aria-label="Page content" id="main-content" tabIndex={-1} className="audit-workspace flex-1 overflow-y-auto focus:outline-none">
          <div aria-hidden="true" className="audit-scroll-fade-layer">
            {scrollFades.top && <div className="audit-scroll-fade-top" />}
            {scrollFades.bottom && <div className="audit-scroll-fade-bottom" style={{ top: `${scrollFades.bottomOffset}px` }} />}
          </div>
          {announcement && !dismissed && (
            <div className="px-5 pt-4 max-sm:px-4" data-testid="announcement-placement">
              <div
                role="status"
                aria-label="Organization announcement"
                className={cn('flex items-start gap-3 border-l-[3px] px-3 py-2.5 text-sm text-text-secondary animate-in fade-in-0 slide-in-from-top-1 duration-200 motion-reduce:animate-none', announcementTone.border, announcementTone.surface)}
              >
                <Megaphone className={cn('mt-0.5 size-4 shrink-0', announcementTone.icon)} aria-hidden="true" />
                <p className="min-w-0 flex-1 leading-5">{announcement}</p>
                <button
                  type="button"
                  onClick={() => {
                    setDismissed(true)
                    sessionStorage.setItem(dismissKey, '1')
                  }}
                  className={cn('inline-flex size-7 shrink-0 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-surface-primary hover:text-text-primary', FOCUS_RING)}
                  aria-label="Dismiss announcement"
                >
                  <X className="size-4" />
                </button>
              </div>
            </div>
          )}
          <div className="admin-page">{children}</div>
        </div>
      </SidebarInset>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      {showShortcuts && <ShortcutsPanel onClose={() => setShowShortcuts(false)} />}
    </>
  )
}
