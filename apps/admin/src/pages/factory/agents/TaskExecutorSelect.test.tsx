import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { TaskExecutorSelect } from './TaskExecutorSelect'

describe('TaskExecutorSelect', () => {
  it.each(['qa', 'judge', 'security_scan'])('shows Pi but explains why it is disabled for %s', async template => {
    const onChange = vi.fn()
    render(<TaskExecutorSelect value="claude" template={template} onChange={onChange} />)
    await userEvent.click(screen.getByRole('combobox', { name: 'Task executor' }))
    const pi = screen.getByRole('option', { name: /Pi \(ChatGPT account/ })
    expect(pi).toHaveAttribute('aria-disabled', 'true')
    expect(pi).toHaveTextContent('Available for issue resolvers and PR reviewers only')
    expect(screen.getByRole('option', { name: /Codex \(OpenAI API key/ })).toHaveAttribute('aria-disabled', 'true')
    await userEvent.click(pi)
    expect(onChange).not.toHaveBeenCalled()
  })

  it.each(['github_issue_resolver', 'github_pr_reviewer'])('allows choosing Pi for %s', async template => {
    const onChange = vi.fn()
    render(<TaskExecutorSelect value="claude" template={template} onChange={onChange} />)
    await userEvent.click(screen.getByRole('combobox', { name: 'Task executor' }))
    const pi = screen.getByRole('option', { name: /Pi \(ChatGPT account/ })
    expect(pi).not.toHaveAttribute('aria-disabled')
    await userEvent.click(pi)
    expect(onChange).toHaveBeenCalledWith('pi')
  })
})
