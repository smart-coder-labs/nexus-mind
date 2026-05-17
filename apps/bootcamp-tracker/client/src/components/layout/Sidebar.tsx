import React from 'react';
import { NavLink } from 'react-router-dom';

const navItems = [
  { to: '/', label: 'Dashboard', icon: '⊞' },
  { to: '/roadmap', label: 'Roadmap', icon: '🗺️' },
  { to: '/reminders', label: 'Reminders', icon: '🔔' },
  { to: '/sessions', label: 'Sessions', icon: '⏱️' },
];

export function Sidebar() {
  return (
    <aside
      className="w-56 shrink-0 h-screen sticky top-0 flex flex-col py-4 px-3 border-r"
      style={{
        backgroundColor: 'var(--color-bg-secondary)',
        borderColor: 'var(--color-border-primary)',
      }}
    >
      {/* Logo */}
      <div className="px-2 mb-6">
        <div className="flex items-center gap-2">
          <span className="text-xl">🧠</span>
          <div>
            <div className="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              NexusMind
            </div>
            <div className="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              Bootcamp Tracker
            </div>
          </div>
        </div>
      </div>

      {/* Navigation */}
      <nav className="flex flex-col gap-1 flex-1">
        {navItems.map(item => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === '/'}
            className={({ isActive }) =>
              `flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm transition-colors ${
                isActive
                  ? 'font-medium'
                  : 'hover:bg-white/5'
              }`
            }
            style={({ isActive }) => ({
              backgroundColor: isActive ? 'var(--color-accent-blue-tint)' : undefined,
              color: isActive ? 'var(--color-accent-blue)' : 'var(--color-text-secondary)',
            })}
          >
            <span className="text-base">{item.icon}</span>
            {item.label}
          </NavLink>
        ))}
      </nav>

      {/* Footer */}
      <div className="px-2 pt-4 border-t" style={{ borderColor: 'var(--color-border-primary)' }}>
        <div className="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          120h · 4 weeks · 12 topics
        </div>
      </div>
    </aside>
  );
}
