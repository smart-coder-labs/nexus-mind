/* ========================================
   MARKDOWN - TYPES
   ======================================== */

export interface MarkdownProps {
  /** Raw markdown source. Rendered as inert markdown — never as live HTML. */
  content: string;
  /** Optional wrapper class. */
  className?: string;
}
