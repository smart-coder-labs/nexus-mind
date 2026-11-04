import { describe, it, expect } from 'vitest'
import { runtimeGuidance, runStages } from './RunDetail'
import { agentStatus } from '../agents/AgentCard'
import { triggerName } from '../shared/format'
import type { AutonomousAgentRun, AutonomousAgentDefinition } from '../../../types'
describe('truthful reviewer readiness and runtime guidance', () => {
  it('labels only recorded provenance and never infers historical GitHub webhooks', () => {
    expect(triggerName('schedule')).toBe('Temporizador')
    expect(
      triggerName('github_webhook', { kind: 'github_webhook', delivery_id: 'received-123' }),
    ).toBe('Webhook recibido de GitHub')
    expect(triggerName('reconcile', { kind: 'github_reconciliation', scan_id: 'scan-123' })).toBe(
      'Reconciliación de GitHub (sondeo)',
    )
    expect(triggerName('reconcile', { kind: 'agent_chain', source_run_id: 'run-123' })).toBe(
      'Encadenado de agente',
    )
    expect(triggerName('github_webhook')).toContain('histórico no registrado')
    expect(triggerName('github_webhook', { kind: 'github_webhook' })).toContain(
      'histórico no registrado',
    )
    expect(triggerName('reconcile')).toContain('histórico')
  })
  it('does not turn missing PR input into a sandbox/authentication error', () => {
    const hint = runtimeGuidance('pull_request_number_missing', 'blocked_runtime')!
    expect(hint).toContain('Falta el número de PR')
    expect(hint).toContain('temporizador')
    expect(hint).not.toContain('Fix the runtime')
    expect(hint).not.toContain('No budget was spent')
    const stages = runStages(
      { status: 'blocked_runtime' } as AutonomousAgentRun,
      [],
      [],
      'pull_request_number_missing',
    )
    expect(stages[1].state).toBe('skipped')
    expect(stages[2].note).toBe('Falta el PR')
  })
  it('does not promise automatic retries for an arbitrary runtime error', () => {
    expect(runtimeGuidance('pi_failed_or_auth_required', 'blocked_runtime')).toContain(
      'no garantiza',
    )
    expect(runtimeGuidance('claude_auth_required', 'blocked_runtime')).toContain(
      'se reintentan automáticamente',
    )
    expect(runtimeGuidance(undefined, 'succeeded')).toBeUndefined()
  })
  it('pending validation takes priority over the legacy enabled flag', () => {
    expect(
      agentStatus({ status: 'enabled', validation_status: 'pending' } as AutonomousAgentDefinition)
        .variant,
    ).toBe('warning')
    expect(
      agentStatus({ status: 'enabled', validation_status: 'valid' } as AutonomousAgentDefinition)
        .variant,
    ).toBe('success')
  })
})
