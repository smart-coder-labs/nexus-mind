import React from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { MarkdownProps } from './Markdown.types'

/**
 * The single shared markdown primitive for the admin.
 *
 * SECURITY — DO NOT ADD `rehype-raw` (design.md §10, decision A6).
 * The content rendered here is agent-authored (memories, conventions, SDD
 * artifacts). `react-markdown` does NOT render embedded HTML unless a raw-HTML
 * rehype plugin is added, so a `<script>` tag or an `onerror=` attribute in the
 * source arrives as inert text. That inertness is a REQUIREMENT, not a happy
 * accident of a library default: adding `rehype-raw` (or `skipHtml={false}`
 * plus a raw-HTML plugin) would turn every stored artifact into a stored-XSS
 * vector. Covered by `markdown_does_not_execute_embedded_html_or_script`.
 *
 * `remark-gfm` is load-bearing too: SDD `tasks.md` artifacts are nothing but
 * GFM task lists and tables.
 */
export const Markdown: React.FC<MarkdownProps> = ({ content, className }) => {
  const body = (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        h1: ({ children }) => (
          <h1 className="text-base font-semibold text-text-primary mt-6 mb-2 first:mt-0">{children}</h1>
        ),
        h2: ({ children }) => (
          <h2 className="text-xs font-semibold text-text-primary mt-5 mb-1.5 pb-1.5 border-b border-border-secondary first:mt-0">{children}</h2>
        ),
        h3: ({ children }) => (
          <h3 className="text-[13px] font-semibold text-accent-blue mt-4 mb-1 first:mt-0">{children}</h3>
        ),
        p: ({ children }) => (
          <p className="text-xs text-text-secondary leading-relaxed mb-3 last:mb-0">{children}</p>
        ),
        ul: ({ children }) => (
          <ul className="mb-3 ml-4 space-y-1 list-none last:mb-0">{children}</ul>
        ),
        ol: ({ children }) => (
          <ol className="mb-3 ml-4 space-y-1 list-decimal last:mb-0">{children}</ol>
        ),
        li: ({ children, className: liClassName }) => {
          // remark-gfm marks a task-list item with `task-list-item`; its bullet is
          // the checkbox itself, so the decorative dot must not be drawn.
          if (liClassName?.includes('task-list-item')) {
            return (
              <li className="task-list-item text-xs text-text-secondary leading-relaxed flex gap-2 items-start">
                {children}
              </li>
            )
          }
          return (
            <li className="text-xs text-text-secondary leading-relaxed flex gap-2">
              <span className="text-accent-blue/50 mt-1.5 shrink-0 w-1 h-1 rounded-full bg-accent-blue/40 inline-block" />
              <span>{children}</span>
            </li>
          )
        },
        // The only <input> markdown can produce is a GFM task-list checkbox. It is
        // always read-only: the admin never authors artifact content (A7).
        input: ({ type, checked }) =>
          type === 'checkbox' ? (
            <input
              type="checkbox"
              checked={Boolean(checked)}
              disabled
              readOnly
              className="mt-1 shrink-0 h-3 w-3 rounded-[3px] border border-border-primary accent-accent-blue cursor-default"
            />
          ) : null,
        strong: ({ children }) => (
          <strong className="font-semibold text-text-primary">{children}</strong>
        ),
        em: ({ children }) => (
          <em className="italic text-text-secondary">{children}</em>
        ),
        a: ({ href, children }) => (
          <a href={href} target="_blank" rel="noopener noreferrer"
             className="text-accent-blue hover:text-accent-blue-hover underline decoration-accent-blue/30 transition-colors">
            {children}
          </a>
        ),
        blockquote: ({ children }) => (
          <blockquote className="border-l-2 border-accent-blue/30 pl-4 my-3 text-text-tertiary italic">
            {children}
          </blockquote>
        ),
        code: ({ children, className: codeClassName }) => {
          const isBlock = codeClassName?.startsWith('language-')
          if (isBlock) {
            return (
              <code className="block text-xs font-mono text-text-secondary leading-relaxed">
                {children}
              </code>
            )
          }
          return (
            <code className="text-[12px] font-mono text-accent-blue bg-accent-blue/8 rounded px-1.5 py-0.5">
              {children}
            </code>
          )
        },
        pre: ({ children }) => (
          <pre className="bg-[#1d1d1f] border border-border-primary rounded-[11px] px-4 py-3 overflow-x-auto mb-3 last:mb-0">
            {children}
          </pre>
        ),
        hr: () => <hr className="border-border-primary my-4" />,
        // A tasks.md table is wide. Scroll it inside its own container so it
        // never blows out the width of the drawer that hosts it.
        table: ({ children }) => (
          <div className="overflow-x-auto mb-3 last:mb-0 border border-border-primary rounded-[8px]">
            <table className="w-full text-xs text-text-secondary border-collapse">{children}</table>
          </div>
        ),
        thead: ({ children }) => (
          <thead className="bg-white/[0.04]">{children}</thead>
        ),
        tr: ({ children }) => (
          <tr className="border-b border-border-secondary last:border-b-0">{children}</tr>
        ),
        th: ({ children }) => (
          <th className="text-left font-semibold text-text-primary px-3 py-2 whitespace-nowrap">{children}</th>
        ),
        td: ({ children }) => (
          <td className="px-3 py-2 align-top">{children}</td>
        ),
      }}
    >
      {content}
    </ReactMarkdown>
  )

  return className ? <div className={className}>{body}</div> : body
}

Markdown.displayName = 'Markdown'
