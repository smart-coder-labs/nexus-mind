import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import type { AutonomousAgentEvent } from '../../../types'
import { RunLogs } from './RunLogs'

const turns = (payloads: Record<string, unknown>[]): AutonomousAgentEvent[] => payloads.map((payload, i) => ({ sequence: i + 1, kind: String(payload.type), payload, created_at: '2026-10-31T04:00:00Z' }))
const describeEvent = (event: AutonomousAgentEvent) => event.kind
const start = { type: 'tool_execution_start', toolCallId: 't1', toolName: 'bash', args: { command: 'cargo test' } }

async function openLogs() { await userEvent.click(screen.getByText(/Event log \(/)) }

describe('RunLogs', () => {
  it('shows lifecycle payloads so the exact failure code is inspectable', async () => {
    const events = turns([{ type: 'run.finished', code: 'pi_oauth_required', executor: 'pi' }])
    render(<RunLogs events={events} transcript={[]} live={false} describeEvent={describeEvent} />)
    await openLogs()
    await userEvent.click(screen.getByText('run.finished'))
    expect(await screen.findByText(/"code": "pi_oauth_required"/)).toBeInTheDocument()
  })

  it('updates Pi tool output in-place while streaming, then shows the final result', async () => {
    const events = [start, { type: 'tool_execution_update', toolCallId: 't1', toolName: 'bash', partialResult: { content: [{ type: 'text', text: 'Compiling tests' }] } }]
    const view = render(<RunLogs events={[]} transcript={turns(events)} live describeEvent={describeEvent} />)
    await openLogs()
    await userEvent.click(screen.getByText('bash'))
    expect(await screen.findByText(/"command": "cargo test"/)).toBeInTheDocument()
    expect(screen.getByText('Compiling tests')).toBeInTheDocument()
    expect(screen.getByText('Running')).toBeInTheDocument()
    view.rerender(<RunLogs events={[]} transcript={turns([...events, { type: 'tool_execution_end', toolCallId: 't1', toolName: 'bash', isError: false, result: { content: [{ type: 'text', text: 'All tests passed' }] } }])} live={false} describeEvent={describeEvent} />)
    expect(await screen.findByText('All tests passed')).toBeInTheDocument()
    expect(screen.queryByText('Compiling tests')).not.toBeInTheDocument()
    expect(screen.getAllByText('bash')).toHaveLength(1)
    expect(screen.getByText('Completed')).toBeInTheDocument()
  })

  it('supports Claude command input, output and failures too', async () => {
    render(<RunLogs events={[]} live={false} describeEvent={describeEvent} transcript={turns([
      { type: 'assistant', message: { content: [{ type: 'tool_use', id: 'c1', name: 'Bash', input: { command: 'npm test' } }] } },
      { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'c1', is_error: true, content: 'Tests failed' }] } },
    ])} />)
    await openLogs()
    await userEvent.click(screen.getByText('Bash'))
    expect(screen.getByText('Failed')).toBeInTheDocument()
    expect(await screen.findByText('Tests failed')).toBeInTheDocument()
    expect(screen.getByText(/"command": "npm test"/)).toBeInTheDocument()
  })

  it('does not label unfinished tools completed after the run stops', async () => {
    render(<RunLogs events={[]} transcript={turns([start])} live={false} describeEvent={describeEvent} />)
    await openLogs()
    expect(screen.getByText('No result recorded')).toBeInTheDocument()
    expect(screen.queryByText('Completed')).not.toBeInTheDocument()
  })

  it('bounds long output, omits images and excludes prompts/reasoning', async () => {
    render(<RunLogs events={[]} live={false} describeEvent={describeEvent} transcript={turns([
      { type: 'message_end', message: { role: 'user', content: [{ type: 'text', text: 'private prompt' }] } },
      { type: 'message_update', assistantMessageEvent: { type: 'thinking_delta', delta: 'private reasoning' } },
      start,
      { type: 'tool_execution_end', toolCallId: 't1', toolName: 'bash', result: { content: [{ type: 'text', text: 'x'.repeat(20_000) + '\nLATEST LINE' }, { type: 'image', data: 'binary secret' }] } },
    ])} />)
    await openLogs()
    await userEvent.click(screen.getByText('bash'))
    expect(await screen.findByText(/earlier output omitted/)).toBeInTheDocument()
    expect(screen.getByText(/LATEST LINE/)).toBeInTheDocument()
    for (const text of ['binary secret', 'private prompt', 'private reasoning']) expect(screen.queryByText(text)).not.toBeInTheDocument()
  })

  it('bounds visible tool rows and lets the user load older logs', async () => {
    const transcript = turns(Array.from({ length: 120 }, (_, i) => ({ type: 'tool_execution_start', toolCallId: String(i), toolName: `tool-${i}` })))
    render(<RunLogs events={[]} transcript={transcript} live describeEvent={describeEvent} />)
    await openLogs()
    expect(screen.queryByText('tool-0')).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: /Show older tool logs/ }))
    expect(screen.getByText('tool-0')).toBeInTheDocument()
    expect(screen.getByText('tool-119')).toBeInTheDocument()
    expect(screen.getByRole('checkbox', { name: 'Follow live logs' })).not.toBeChecked()
  })
})
