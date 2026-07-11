import { describe, it, expect, afterEach } from 'vitest'
import { screen, within } from '@testing-library/react'
import { renderWithProviders } from '../../../test/render'
import { Markdown } from './Markdown'
import packageJson from '../../../../package.json?raw'
import markdownSource from './Markdown.tsx?raw'

// The <Markdown> primitive is pure presentation — it takes no API client, so the
// vi.hoisted() + vi.mock('../api/client') harness the page suites need does not apply.

describe('Markdown', () => {
  it('markdown_renders_gfm_table', () => {
    const { container } = renderWithProviders(
      <Markdown content={'| a | b |\n|---|---|\n| 1 | 2 |'} />,
    )

    const table = container.querySelector('table')
    expect(table).not.toBeNull()

    const header = within(table as HTMLElement).getByRole('columnheader', { name: 'a' })
    expect(header.tagName).toBe('TH')
    expect(within(table as HTMLElement).getByRole('cell', { name: '1' }).tagName).toBe('TD')

    // Without remark-gfm the row would survive as a literal paragraph of pipes.
    expect(container.textContent).not.toContain('|')
    expect(screen.queryByText(/\|---\|/)).toBeNull()
  })

  it('markdown_renders_gfm_task_list_checkboxes', () => {
    // This is the tasks.md requirement: SDD task lists are checklists.
    const { container } = renderWithProviders(
      <Markdown content={'- [ ] Write the migration\n- [x] Write the spec'} />,
    )

    const boxes = container.querySelectorAll('input[type="checkbox"]')
    expect(boxes).toHaveLength(2)
    expect((boxes[0] as HTMLInputElement).checked).toBe(false)
    expect((boxes[1] as HTMLInputElement).checked).toBe(true)
    // A7 — the admin never writes artifact content, so the checkbox is read-only.
    expect((boxes[0] as HTMLInputElement).disabled).toBe(true)
    expect((boxes[1] as HTMLInputElement).disabled).toBe(true)

    // The literal task-list syntax must not leak through as text.
    expect(container.textContent).not.toContain('- [ ]')
    expect(container.textContent).not.toContain('[x]')
    expect(screen.getByText('Write the migration')).toBeInTheDocument()

    // A task-list item is marked by its checkbox, never by the decorative bullet
    // dot the plain-list override draws.
    const taskItems = container.querySelectorAll('li.task-list-item')
    expect(taskItems).toHaveLength(2)
    taskItems.forEach(li => {
      expect(li.querySelector('span.rounded-full')).toBeNull()
    })
  })

  // ── A6: agent-authored markdown must never execute ────────────────────────

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__pwned
  })

  it('markdown_does_not_execute_embedded_html_or_script', () => {
    const hostile = [
      '# Heading',
      '',
      '<script>window.__pwned = true</script>',
      '',
      '<img src=x onerror="window.__pwned = true">',
      '',
      'tail',
    ].join('\n')

    const { container } = renderWithProviders(<Markdown content={hostile} />)

    // Nothing ran.
    expect((window as unknown as Record<string, unknown>).__pwned).toBeUndefined()

    // No live HTML made it into the DOM.
    expect(container.querySelector('script')).toBeNull()
    expect(container.querySelector('img')).toBeNull()

    // No inline event-handler attribute survived anywhere in the output.
    container.querySelectorAll('*').forEach(el => {
      for (const attr of Array.from(el.attributes)) {
        expect(attr.name.toLowerCase().startsWith('on')).toBe(false)
      }
    })
    // No live tag was ever constructed: the raw HTML arrives escaped, as text.
    expect(container.innerHTML).not.toContain('<script')
    expect(container.innerHTML).not.toContain('<img')
    expect(container.innerHTML).toContain('&lt;script&gt;')

    // The surrounding markdown still renders — the HTML is simply inert.
    expect(screen.getByRole('heading', { name: 'Heading' })).toBeInTheDocument()
    expect(screen.getByText('tail')).toBeInTheDocument()
  })

  it('markdown_never_depends_on_rehype_raw', () => {
    // A6 is a MUST, not an accident of a library default: rehype-raw would make
    // every stored artifact a stored-XSS vector.
    expect(packageJson).not.toContain('rehype-raw')
    expect(packageJson).toContain('remark-gfm')

    // The primitive imports no raw-HTML plugin and passes no rehype plugins at
    // all. (The word itself appears in the file — in the comment forbidding it.)
    expect(markdownSource).not.toMatch(/from\s+['"]rehype-/)
    expect(markdownSource).not.toContain('rehypePlugins')
  })

  it('markdown_renders_strikethrough_and_autolinks', () => {
    const { container } = renderWithProviders(
      <Markdown content={'~~dropped~~ see https://nexusmind.dev/docs for more'} />,
    )

    const del = container.querySelector('del')
    expect(del).not.toBeNull()
    expect(del?.textContent).toBe('dropped')
    expect(container.textContent).not.toContain('~~')

    const link = screen.getByRole('link', { name: 'https://nexusmind.dev/docs' })
    expect(link).toHaveAttribute('href', 'https://nexusmind.dev/docs')
    expect(link).toHaveAttribute('rel', 'noopener noreferrer')
  })

  it('markdown_wide_table_scrolls_horizontally_without_breaking_layout', () => {
    const wide = [
      '| PR | Branch | Scope | Est. lines | Depends on |',
      '|----|--------|-------|-----------:|------------|',
      '| PR-7 | sdd-artifacts/pr7-markdown-primitive | remark-gfm + primitive | ~280 | — |',
    ].join('\n')

    const { container } = renderWithProviders(<Markdown content={wide} />)

    const table = container.querySelector('table')
    expect(table).not.toBeNull()

    // The table must be wrapped, not left to blow out the drawer width.
    const wrapper = table?.parentElement
    expect(wrapper?.tagName).toBe('DIV')
    expect(wrapper?.className).toContain('overflow-x-auto')
  })
})
