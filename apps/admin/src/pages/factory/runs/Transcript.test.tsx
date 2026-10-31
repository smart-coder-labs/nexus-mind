import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { AutonomousAgentEvent } from '../../../types'
import { TranscriptView } from './Transcript'

vi.mock('../../../components/ui/Markdown', () => ({ Markdown: ({ content }: { content: string }) => <span>{content}</span> }))

function turns(payloads: Record<string, unknown>[]): AutonomousAgentEvent[] {
  return payloads.map((payload, index) => ({ sequence: index + 1, kind: String(payload.type), payload, created_at: '2026-10-31' }))
}
const assistant = (text: string) => ({ role: 'assistant', content: [{ type: 'text', text }] })
const delta = (text: string, index = 0) => ({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', contentIndex: index, delta: text } })

describe('factory conversation transcript', () => {
  it('preserves Claude prose, tool calls, errors and structured final output', () => {
    render(<TranscriptView live={false} turns={turns([
      { type: 'system', message: { content: [{ type: 'text', text: 'system hidden' }] } },
      { type: 'assistant', message: { content: [{ type: 'text', text: 'Inspecting files' }, { type: 'tool_use', name: 'Read' }] } },
      { type: 'user', message: { content: [{ type: 'tool_result', is_error: true, content: 'File missing' }] } },
      { type: 'assistant', message: assistant('{"summary":"Review complete","findings":[]}') },
      { type: 'result', result: 'Review complete' },
    ])} />)
    expect(screen.getByText('Inspecting files')).toBeInTheDocument()
    expect(screen.getByText('Read')).toBeInTheDocument()
    expect(screen.getByText('File missing')).toBeInTheDocument()
    expect(screen.getAllByText('Review complete')).toHaveLength(1)
    expect(screen.queryByText('system hidden')).not.toBeInTheDocument()
  })

  it('streams Pi text before message_end and replaces drafts without duplicates', () => {
    const events = [{ type: 'message_start', message: { role: 'assistant', content: [] } }, delta('Checking '), delta('the page')]
    const view = render(<TranscriptView live turns={turns(events)} />)
    expect(screen.getByText('Checking the page')).toBeInTheDocument()
    expect(screen.getByRole('status')).toHaveTextContent('Streaming')
    view.rerender(<TranscriptView live={false} turns={turns([
      ...events,
      { type: 'message_update', assistantMessageEvent: { type: 'text_end', contentIndex: 0, content: 'Page checked' } },
      { type: 'message_end', message: assistant('Page checked') },
      { type: 'turn_end', message: assistant('Page checked') },
      { type: 'agent_end', messages: [assistant('Page checked')] },
    ])} />)
    expect(screen.getAllByText('Page checked')).toHaveLength(1)
    expect(screen.queryByText('Checking the page')).not.toBeInTheDocument()
    expect(screen.queryByText('No readable messages yet.')).not.toBeInTheDocument()
  })

  it('renders Pi tools once and surfaces tool errors without dumping successful outputs', () => {
    render(<TranscriptView live turns={turns([
      { type: 'message_end', message: { role: 'assistant', content: [{ type: 'toolCall', id: 't1', name: 'mcp__playwright__browser_navigate' }] } },
      { type: 'tool_execution_start', toolCallId: 't1', toolName: 'mcp__playwright__browser_navigate' },
      { type: 'tool_execution_end', toolCallId: 't1', toolName: 'mcp__playwright__browser_navigate', isError: false, result: { content: [{ type: 'text', text: 'large snapshot hidden' }] } },
      { type: 'tool_execution_start', toolCallId: 't2', toolName: 'read' },
      { type: 'tool_execution_end', toolCallId: 't2', toolName: 'read', isError: true, result: { content: [{ type: 'text', text: 'Permission denied' }] } },
    ])} />)
    expect(screen.getAllByText('mcp__playwright__browser_navigate')).toHaveLength(1)
    expect(screen.getByText('Permission denied')).toBeInTheDocument()
    expect(screen.queryByText('large snapshot hidden')).not.toBeInTheDocument()
  })

  it('renders historical Pi messages without start events and hides system, user and thinking content', () => {
    render(<TranscriptView live={false} turns={turns([
      { type: 'message_end', message: { role: 'system', content: [{ type: 'text', text: 'system hidden' }] } },
      { type: 'message_end', message: { role: 'user', content: [{ type: 'text', text: 'prompt hidden' }] } },
      { type: 'message_update', assistantMessageEvent: { type: 'thinking_delta', contentIndex: 0, delta: 'thinking hidden' } },
      { type: 'message_end', message: { role: 'assistant', content: [{ type: 'thinking', thinking: 'thinking hidden' }, { type: 'text', text: '{"summary":"QA complete","findings":[]}' }] } },
    ])} />)
    expect(screen.getByText('QA complete')).toBeInTheDocument()
    expect(screen.getByText('0 findings')).toBeInTheDocument()
    for (const text of ['system hidden', 'prompt hidden', 'thinking hidden']) expect(screen.queryByText(text)).not.toBeInTheDocument()
  })

  it('keeps multiple Pi assistant responses separate and surfaces provider errors', () => {
    render(<TranscriptView live={false} turns={turns([
      { type: 'message_start', message: { role: 'assistant' } }, delta('First response'),
      { type: 'message_end', message: assistant('First response') },
      { type: 'message_start', message: { role: 'assistant' } }, delta('Second response'),
      { type: 'message_end', message: { ...assistant('Second response'), errorMessage: 'Rate limited' } },
    ])} />)
    expect(screen.getAllByText('First response')).toHaveLength(1)
    expect(screen.getAllByText('Second response')).toHaveLength(1)
    expect(screen.getByText('Rate limited')).toBeInTheDocument()
  })

  it('coalesces thousands of Pi streaming events into one message', () => {
    const text = 'x'.repeat(1300)
    render(<TranscriptView live turns={turns([
      { type: 'message_start', message: { role: 'assistant' } },
      ...Array.from({ length: 1300 }, () => delta('x')),
      { type: 'message_end', message: assistant(text) },
    ])} />)
    expect(screen.getAllByText(text)).toHaveLength(1)
  })
})
