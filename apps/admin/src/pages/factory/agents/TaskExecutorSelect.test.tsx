import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { TaskExecutorSelect } from './TaskExecutorSelect'
import { CODEX_TEMPLATES } from '../isolation'

describe('TaskExecutorSelect', () => {
  it.each(CODEX_TEMPLATES)('allows choosing Pi and Codex for %s', async template => {
    const onChange = vi.fn()
    render(<TaskExecutorSelect value="claude" template={template} onChange={onChange} />)
    await userEvent.click(screen.getByRole('combobox', { name: 'Task executor' }))
    const pi = screen.getByRole('option', { name: /Pi \(ChatGPT account/ })
    expect(pi).not.toHaveAttribute('aria-disabled')
    expect(screen.getByRole('option', { name: /Codex \(OpenAI API key/ })).not.toHaveAttribute('aria-disabled')
    await userEvent.click(pi)
    expect(onChange).toHaveBeenCalledWith('pi')
    await userEvent.click(screen.getByRole('combobox', { name: 'Task executor' }))
    await userEvent.click(screen.getByRole('option', { name: /Codex \(OpenAI API key/ }))
    expect(onChange).toHaveBeenCalledWith('codex')
  })

  it('keeps unknown templates disabled', async () => {
    render(<TaskExecutorSelect value="claude" template="unknown" onChange={vi.fn()} />)
    await userEvent.click(screen.getByRole('combobox', { name: 'Task executor' }))
    expect(screen.getByRole('option', { name: /Pi \(ChatGPT account/ })).toHaveAttribute('aria-disabled', 'true')
    expect(screen.getByRole('option', { name: /Codex \(OpenAI API key/ })).toHaveAttribute('aria-disabled', 'true')
  })
})
