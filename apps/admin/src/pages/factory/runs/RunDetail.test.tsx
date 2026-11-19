import { describe, expect, it } from 'vitest'
import type { AutonomousAgentEvent } from '../../../types'
import { runOrigins, slackPosts } from './RunDetail'

const ev = (sequence: number, kind: string, payload: Record<string, unknown>): AutonomousAgentEvent => ({ sequence, kind, payload, created_at: '2026-11-19 11:00:00' })

describe('runOrigins', () => {
  it('reads the run.origin event and attaches the judge verdict comment', () => {
    const events = [ev(1, 'run.origin', { targets: [{ type: 'pr', repository: 'acme/web', number: 284, url: 'https://github.com/acme/web/pull/284' }] })]
    const pub = { results: [{ number: 284, url: 'https://github.com/acme/web/pull/284#issuecomment-1' }] }
    expect(runOrigins(events, pub)).toEqual([{ type: 'pr', label: 'acme/web#284', url: 'https://github.com/acme/web/pull/284', commentUrl: 'https://github.com/acme/web/pull/284#issuecomment-1' }])
  })

  it('falls back to what older runs published', () => {
    expect(runOrigins([], { results: [{ url: 'https://github.com/acme/web/issues/9#issuecomment-2' }] })[0]).toMatchObject({ type: 'issue', label: 'acme/web#9' })
    expect(runOrigins([], { github_review: { _links: { html: { href: 'https://github.com/acme/api/pull/78#pullrequestreview-5' } } } })[0]).toMatchObject({ type: 'pr', url: 'https://github.com/acme/api/pull/78' })
    expect(runOrigins([], {})).toEqual([])
  })
})

describe('slackPosts', () => {
  it('pairs Slack MCP sends with their message link', () => {
    const transcript = [
      ev(1, 'assistant', { message: { content: [{ type: 'tool_use', id: 't1', name: 'mcp__slack__slack_send_message', input: { channel_id: 'D04', message: 'Judged PR #284' } }, { type: 'tool_use', id: 't2', name: 'mcp__slack__slack_search_users', input: {} }] } }),
      ev(2, 'user', { message: { content: [{ type: 'tool_result', tool_use_id: 't1', content: [{ type: 'text', text: '{"message_link":"https://x.slack.com/archives/D04/p179"}' }] }] } }),
    ]
    expect(slackPosts(transcript)).toEqual([{ channel: 'D04', text: 'Judged PR #284', failed: false, link: 'https://x.slack.com/archives/D04/p179' }])
  })
})
