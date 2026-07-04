import { type ClassValue, clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

// For interpolating untrusted strings into raw-HTML contexts (e.g. force-graph nodeLabel).
export function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, c => (
    c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : '&quot;'
  ))
}
