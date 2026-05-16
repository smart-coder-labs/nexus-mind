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
        backgroundColor: 'var(--bg-secondary)',
        borderColor: 'var(--border)',
      }}
    >
      {/* Logo */}
      <div className="px-2 mb-6">
        <div className="flex items-center gap-2">
          <span className="text-xl">🧠</span>
          <div>
            <div className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
              NexusMind
            </div>
            <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>
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
              backgroundColor: isActive ? 'rgba(88, 166, 255, 0.1)' : undefined,
              color: isActive ? 'var(--accent)' : 'var(--text-secondary)',
            })}
          >
            <span className="text-base">{item.icon}</span>
            {item.label}
          </NavLink>
        ))}
      </nav>

      {/* Footer */}
      <div className="px-2 pt-4 border-t" style={{ borderColor: 'var(--border)' }}>
        <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
          120h · 4 weeks · 12 topics
        </div>
      </div>
    </aside>
  );
}
