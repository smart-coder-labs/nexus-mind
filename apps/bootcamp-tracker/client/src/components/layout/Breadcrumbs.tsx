import React from 'react';
import { Link } from 'react-router-dom';

interface Crumb {
  label: string;
  to?: string;
}

interface BreadcrumbsProps {
  crumbs: Crumb[];
}

export function Breadcrumbs({ crumbs }: BreadcrumbsProps) {
  return (
    <nav className="flex items-center gap-2 text-sm mb-6">
      {crumbs.map((crumb, i) => (
        <React.Fragment key={i}>
          {i > 0 && (
            <span style={{ color: 'var(--text-muted)' }}>/</span>
          )}
          {crumb.to ? (
            <Link
              to={crumb.to}
              className="transition-colors hover:underline"
              style={{ color: 'var(--accent)' }}
            >
              {crumb.label}
            </Link>
          ) : (
            <span style={{ color: 'var(--text-secondary)' }}>{crumb.label}</span>
          )}
        </React.Fragment>
      ))}
    </nav>
  );
}
