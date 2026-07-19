import type { ReactNode } from 'react'

function escapeRegExp(term: string): string {
  return term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * Splits `text` into plain and highlighted fragments wherever any non-empty
 * whitespace-separated token of `query` occurs (case-insensitive, substring match).
 *
 * Safe by construction: this returns React children (strings + `<mark>` elements),
 * never `dangerouslySetInnerHTML`, so there is no HTML-injection surface even
 * though `text` and `query` are both untrusted/user-supplied.
 */
export function highlightMatches(text: string, query: string): ReactNode {
  const terms = query
    .split(/\s+/)
    .map(t => t.trim())
    .filter(Boolean)
    .map(escapeRegExp)

  if (terms.length === 0 || !text) return text

  const re = new RegExp(`(${terms.join('|')})`, 'gi')
  const parts = text.split(re)
  if (parts.length <= 1) return text

  // String.prototype.split with a single capturing group interleaves the
  // captured matches at odd indices — no regex-state re-testing needed.
  //
  // Non-matched segments are returned as bare strings (not wrapped in a <span>)
  // on purpose: Testing Library's getByText only reads an element's *direct*
  // text-node children, not full recursive textContent, so wrapping every
  // segment would make the parent's own text invisible to `getByText` even
  // though the highlighted word is a true substring of it.
  return parts.map((part, i) =>
    i % 2 === 1 ? (
      <mark
        key={i}
        className="bg-status-warning/25 text-status-warning rounded-[3px] px-0.5 not-italic font-medium"
      >
        {part}
      </mark>
    ) : (
      part
    ),
  )
}
