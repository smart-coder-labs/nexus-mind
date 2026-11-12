import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { TaskExecutorSelect } from './TaskExecutorSelect'

describe('TaskExecutorSelect', () => {
  it.each(['lead_generation', 'judge', 'security_scan'])('shows Pi but explains why it is disabled for %s', async template => {
    const onChange = vi.fn()
    render(<TaskExecutorSelect value="claude" template={template} onChange={onChange} />)
    await userEvent.click(screen.getByRole('combobox', { name: 'Task executor' }))
    const pi = screen.getByRole('option', { name: /Pi \(ChatGPT account/ })
    expect(pi).toHaveAttribute('aria-disabled', 'true')
    expect(pi).toHaveTextContent('Available for QA, issue resolvers and PR reviewers only')
    expect(screen.getByRole('option', { name: /Codex \(OpenAI API key/ })).toHaveAttribute('aria-disabled', 'true')
    await userEvent.click(pi)
    expect(onChange).not.toHaveBeenCalled()
  })

  it.each(['github_issue_resolver', 'github_pr_reviewer', 'qa'])('allows choosing Pi for %s', async template => {
    const onChange = vi.fn()
    render(<TaskExecutorSelect value="claude" template={template} onChange={onChange} />)
    await userEvent.click(screen.getByRole('combobox', { name: 'Task executor' }))
    const pi = screen.getByRole('option', { name: /Pi \(ChatGPT account/ })
    expect(pi).not.toHaveAttribute('aria-disabled')
    if (template === 'qa') {
      expect(screen.getByRole('option', { name: /Codex \(OpenAI API key/ })).toHaveAttribute('aria-disabled', 'true')
      expect(pi).toHaveTextContent('Playwright QA')
    }
    await userEvent.click(pi)
    expect(onChange).toHaveBeenCalledWith('pi')
  })
})
