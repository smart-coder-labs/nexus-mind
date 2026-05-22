#!/usr/bin/env node
if (process.argv[2] === 'setup') {
  await import('./setup.js')
  process.exit(0)
}

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { z } from 'zod'
import { storeMemory, searchMemories, listMemories } from './client.js'
import type { Memory } from './client.js'

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

// ── Server ───────────────────────────────────────────────────────────────────

const server = new McpServer({
  name: 'nexusmind',
  version: '0.1.0',
})

// store_memory
server.tool(
  'store_memory',
  'Store a memory, decision, or piece of context for later retrieval by the team.',
  {
    content: z.string().describe('The memory content to store (decision, convention, finding, etc.)'),
    project: z.string().optional().describe('Project or repo name (e.g. "nexusmind", "payments-api")'),
    tool:    z.string().optional().describe('Tool name — defaults to "claude-code"'),
    tags:    z.array(z.string()).optional().describe('Tags for categorization (e.g. ["auth", "convention"])'),
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
    tool:    z.string().optional().describe('Filter by tool (e.g. "claude-code", "cursor")'),
    limit:   z.number().int().min(1).max(100).optional().describe('Max results (default: 20)'),
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

// ── Start ────────────────────────────────────────────────────────────────────

const transport = new StdioServerTransport()
await server.connect(transport)
