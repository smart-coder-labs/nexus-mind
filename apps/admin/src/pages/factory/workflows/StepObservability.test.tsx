import { describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { StepObservability, sanitize, transcriptTurns } from './StepObservability'
import type { WorkflowEvent, WorkflowEventPage } from './types'

const event = (seq: number, kind: string, payload: Record<string, unknown>): WorkflowEvent => ({
  seq, step_id: 'implement', kind, payload, created_at: '2026-11-07 10:00:00',
})
const page = (events: WorkflowEvent[], over: Partial<WorkflowEventPage> = {}): WorkflowEventPage => ({
  events,
  has_older: false,
  has_newer: false,
  coverage: { log_events: 1, log_limit: 6000, control_events: 1, control_limit: 3000, truncated: [], pruned_events: 0, dropped_events: 0, gap_bytes: 0, complete: true },
  context: { run_created_at: '2026-11-07 09:00:00', backend_started_at: '2026-11-07 09:30:00', runner: { name: 'oracle', last_seen: null, revoked: false } },
  ...over,
})
const props = {
  runId: 'run1',
  stepId: 'implement',
  agent: true,
  live: false,
  artifacts: [{ id: 'a1', step_id: 'implement', attempt: 2, name: 'runtime.log-0', mime: 'text/plain', digest: 'd', size: 2048 }],
  openArtifact: vi.fn(),
}

describe('StepObservability', () => {
  it('shows the cause, the console and the conversation of the selected attempt', async () => {
    const assistant = JSON.stringify({ type: 'assistant', message: { content: [{ type: 'text', text: 'Corrijo el lockfile' }] } })
    const load = vi.fn().mockResolvedValue(page([
      event(1, 'log.chunk', { message: `\u001b[31mnpm ERR!\u001b[0m lockfile\n${assistant}\n`, source: 'agent', attempt: 2 }),
      event(2, 'command.finished', { message: 'exit 137', data: { exit_code: 137, duration_ms: 4200, hint: 'killed_or_out_of_memory' } }),
      event(3, 'backend.rejected', { route: '/v1/factory/runner/runs/:run/steps/:step/lease', status: 409, code: 'lease_conflict', request_id: 'rn-1' }),
    ], { coverage: { ...page([]).coverage, complete: false, dropped_events: 3 } }))
    render(<StepObservability {...props} state={{ id: 'implement', status: 'blocked', attempt: 2, result: { output: { error: 'adapter_http_502', stage: 'container', request_id: 'rn-abc', retryable: true } } }} load={load} />)
    await waitFor(() => expect(load).toHaveBeenCalledWith({ step: 'implement', attempt: 2, limit: 300 }))
    expect(await screen.findByText('container')).toBeInTheDocument()
    expect(screen.getByText('rn-abc')).toBeInTheDocument()
    expect(screen.getAllByText(/posible falta de memoria/).length).toBeGreaterThan(0)
    expect(screen.getByText(/lease_conflict/)).toBeInTheDocument()
    expect(screen.getByRole('note')).toHaveTextContent('3 eventos no registrados')
    expect(screen.getByText(/se reinició/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('tab', { name: 'Consola' }))
    const log = screen.getByRole('log')
    expect(log).toHaveTextContent('npm ERR! lockfile')
    expect(log.textContent).not.toContain('\u001b')
    await userEvent.click(screen.getByRole('tab', { name: 'Conversación' }))
    expect(screen.getByText('Corrijo el lockfile')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('tab', { name: 'Artefactos (1)' }))
    await userEvent.click(screen.getByText('runtime.log-0'))
    expect(props.openArtifact).toHaveBeenCalledWith('a1')
  })

  it('distinguishes no permission, not executed and no recorded conversation', async () => {
    const forbidden = vi.fn().mockRejectedValue(Object.assign(new Error('Forbidden'), { status: 403 }))
    const { unmount } = render(<StepObservability {...props} state={{ id: 'implement', status: 'blocked', attempt: 1, result: null }} load={forbidden} />)
    expect(await screen.findByText(/No tienes permiso/)).toBeInTheDocument()
    unmount()
    render(<StepObservability {...props} state={{ id: 'implement', status: 'pending', attempt: 0, result: null }} load={forbidden} />)
    expect(screen.getByText('Fase no ejecutada todavía.')).toBeInTheDocument()
  })

  it('reads conversation only from complete JSON lines and strips terminal control sequences', () => {
    expect(sanitize('\u001b]0;title\u0007ok\u001b[1m!\u0000')).toBe('ok!')
    const turns = transcriptTurns([event(1, 'log.chunk', { message: '{"type":"assistant"}\n{"type":"user"\nplain\n' })])
    expect(turns.map((t) => t.kind)).toEqual(['assistant'])
  })
})
