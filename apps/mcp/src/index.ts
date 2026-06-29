#!/usr/bin/env node
if (process.argv[2] === 'setup') {
  await import('./setup.js')
  process.exit(0)
}

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { z } from 'zod'
import {
  storeMemory,
  searchMemories,
  listMemories,
  listPolicies,
  createPolicy,
  updatePolicy,
  deletePolicy,
  checkPolicy,
} from './client.js'
import type { Memory, Policy, PolicyCheckResponse } from './client.js'

// ── Helpers ──────────────────────────────────────────────────────────────────

function formatMemory(m: Memory): string {
  const date = new Date(m.created_at).toLocaleDateString()
  const tags = m.tags.length > 0 ? ` [${m.tags.join(', ')}]` : ''
  return `• [${m.tool}] ${m.project || '(no project)'} — ${m.content}${tags} (${date})`
}

function formatList(memories: Memory[]): string {
  if (memories.length === 0) return 'No memories found.'
  return memories.map(formatMemory).join('\n')
}

function formatPolicy(p: Policy): string {
  const state = p.enabled ? 'enabled' : 'disabled'
  return `• ${p.name} (${p.rule_type}, ${state}) — id: ${p.id}\n    config: ${JSON.stringify(p.config)}`
}

function formatPolicies(policies: Policy[]): string {
  if (policies.length === 0) return 'No policies found.'
  return policies.map(formatPolicy).join('\n')
}

function formatCheck(res: PolicyCheckResponse): string {
  if (res.allowed) return 'ALLOWED — no policy violations.'
  const lines = res.violations.map(
    (v) => `  ✗ ${v.policy_name} (${v.rule_type}): ${v.reason}`
  )
  return `DENIED — ${res.violations.length} violation(s):\n${lines.join('\n')}`
}

// ── Server ───────────────────────────────────────────────────────────────────

const server = new McpServer({
  name: 'nexusmind',
  version: '0.4.5',
})

// store_memory
server.tool(
  'store_memory',
  'Store a memory, decision, or piece of context for later retrieval by the team.',
  {
    content: z.string().describe('The memory content to store (decision, convention, finding, etc.)'),
    project: z.string().optional().describe('Project or repo name (e.g. "nexusmind", "payments-api")'),
    tool: z.string().optional().describe('Tool name — defaults to "claude-code"'),
    tags: z.array(z.string()).optional().describe('Tags for categorization (e.g. ["auth", "convention"])'),
  },
  async ({ content, project, tool, tags }) => {
    try {
      const res = await storeMemory({ content, project, tool, tags })
      return {
        content: [{ type: 'text', text: `Memory stored (id: ${res.id})` }],
      }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)

// search_memory
server.tool(
  'search_memory',
  'Search past memories and decisions stored by the team using full-text search.',
  {
    query: z.string().describe('What to search for (e.g. "authentication", "database connection pool")'),
    limit: z.number().int().min(1).max(50).optional().describe('Max results to return (default: 10)'),
  },
  async ({ query, limit }) => {
    try {
      const memories = await searchMemories(query, limit ?? 10)
      const text = memories.length === 0
        ? `No memories found for query: "${query}"`
        : `Found ${memories.length} result(s) for "${query}":\n\n${formatList(memories)}`
      return { content: [{ type: 'text', text }] }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)

// list_memories
server.tool(
  'list_memories',
  'List recent memories stored by the team, optionally filtered by project or tool.',
  {
    project: z.string().optional().describe('Filter by project name'),
    tool: z.string().optional().describe('Filter by tool (e.g. "claude-code", "cursor")'),
    limit: z.number().int().min(1).max(100).optional().describe('Max results (default: 20)'),
  },
  async ({ project, tool, limit }) => {
    try {
      const memories = await listMemories({ project, tool, limit: limit ?? 20 })
      const text = memories.length === 0
        ? 'No memories found.'
        : `${memories.length} recent memory(ies):\n\n${formatList(memories)}`
      return { content: [{ type: 'text', text }] }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)

// list_policies
server.tool(
  'list_policies',
  'List all governance policies for the caller\'s organization (model whitelist, budget limits, PII redaction).',
  {},
  async () => {
    try {
      const policies = await listPolicies()
      const text = policies.length === 0
        ? 'No policies found.'
        : `${policies.length} policy(ies):\n\n${formatPolicies(policies)}`
      return { content: [{ type: 'text', text }] }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)

// create_policy
server.tool(
  'create_policy',
  'Create a new governance policy. rule_type must be one of: model_whitelist, budget_limit, pii_redact.',
  {
    name: z.string().describe('Human-readable policy name (≤128 chars)'),
    rule_type: z
      .enum(['model_whitelist', 'budget_limit', 'pii_redact'])
      .describe('The kind of rule this policy enforces'),
    config: z
      .record(z.unknown())
      .describe(
        'Rule config. model_whitelist: { allowed_models: string[] }; budget_limit: { max_tokens_per_day?, max_requests_per_day? }; pii_redact: { patterns: string[] }'
      ),
    enabled: z.boolean().optional().describe('Whether the policy is active (default: true)'),
  },
  async ({ name, rule_type, config, enabled }) => {
    try {
      const policy = await createPolicy({ name, rule_type, config, enabled })
      return {
        content: [{ type: 'text', text: `Policy created:\n${formatPolicy(policy)}` }],
      }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)

// update_policy
server.tool(
  'update_policy',
  'Update an existing policy. rule_type is immutable; only name, config, and enabled can change.',
  {
    id: z.string().describe('The policy id to update'),
    name: z.string().optional().describe('New policy name'),
    config: z.record(z.unknown()).optional().describe('New rule config (must match the existing rule_type)'),
    enabled: z.boolean().optional().describe('Enable or disable the policy'),
  },
  async ({ id, name, config, enabled }) => {
    try {
      const policy = await updatePolicy(id, { name, config, enabled })
      return {
        content: [{ type: 'text', text: `Policy updated:\n${formatPolicy(policy)}` }],
      }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)

// delete_policy
server.tool(
  'delete_policy',
  'Delete a governance policy by id.',
  {
    id: z.string().describe('The policy id to delete'),
  },
  async ({ id }) => {
    try {
      await deletePolicy(id)
      return { content: [{ type: 'text', text: `Policy deleted (id: ${id})` }] }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)

// check_policy
server.tool(
  'check_policy',
  'Evaluate the active policies against a prospective request and report whether it is allowed.',
  {
    model: z.string().describe('The model the request would use (e.g. "claude-3-opus")'),
    prompt_tokens: z.number().int().min(0).optional().describe('Estimated prompt tokens — used for budget checks'),
    prompt_preview: z.string().optional().describe('A preview of the prompt content — used for PII redaction checks'),
    user_id: z.string().optional().describe('The user making the request'),
    project: z.string().optional().describe('The project the request belongs to'),
  },
  async ({ model, prompt_tokens, prompt_preview, user_id, project }) => {
    try {
      const res = await checkPolicy({ model, prompt_tokens, prompt_preview, user_id, project })
      return { content: [{ type: 'text', text: formatCheck(res) }] }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)

// ── Start ────────────────────────────────────────────────────────────────────

const transport = new StdioServerTransport()
await server.connect(transport)
