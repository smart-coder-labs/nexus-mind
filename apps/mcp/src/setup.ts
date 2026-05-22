#!/usr/bin/env node
/**
 * nexusmind-mcp setup
 * Installs the NexusMind MCP server + Claude Code hooks into ~/.claude/settings.json
 */
import { execFileSync, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import * as readline from 'node:readline/promises'
import { stdin as input, stdout as output } from 'node:process'

const __dirname = dirname(fileURLToPath(import.meta.url))
const PLUGIN_DIR = join(__dirname, '..', 'plugin')
const CLAUDE_DIR = join(homedir(), '.claude')
const SETTINGS_PATH = join(CLAUDE_DIR, 'settings.json')
const DEFAULT_BASE_URL = 'https://nexusmind-backend.fly.dev'

// ── Helpers ───────────────────────────────────────────────────────────────────

const c = {
  reset: '\x1b[0m',
  bold:  '\x1b[1m',
  green: '\x1b[32m',
  blue:  '\x1b[34m',
  yellow:'\x1b[33m',
  red:   '\x1b[31m',
}

const info    = (msg: string) => console.log(`${c.blue}[nexusmind]${c.reset} ${msg}`)
const success = (msg: string) => console.log(`${c.green}[nexusmind]${c.reset} ${msg}`)
const warn    = (msg: string) => console.log(`${c.yellow}[nexusmind] WARNING:${c.reset} ${msg}`)
const err     = (msg: string) => console.error(`${c.red}[nexusmind] ERROR:${c.reset} ${msg}`)

function readSettings(): Record<string, unknown> {
  if (!existsSync(SETTINGS_PATH)) return {}
  try { return JSON.parse(readFileSync(SETTINGS_PATH, 'utf8')) } catch { return {} }
}

function writeSettings(settings: Record<string, unknown>) {
  mkdirSync(CLAUDE_DIR, { recursive: true })
  writeFileSync(SETTINGS_PATH, JSON.stringify(settings, null, 2) + '\n')
}

// ── Main ──────────────────────────────────────────────────────────────────────

console.log(`\n${c.bold}NexusMind — Claude Code Setup${c.reset}`)
console.log('────────────────────────────────\n')

// 1. Collect config — only ask for API key
const rl = readline.createInterface({ input, output })

let apiKey = process.env.NEXUSMIND_API_KEY ?? ''
if (!apiKey) apiKey = await rl.question('NexusMind API key: ')
rl.close()

const baseUrl: string = process.env.NEXUSMIND_BASE_URL ?? DEFAULT_BASE_URL

if (!apiKey.trim()) {
  warn('No API key provided — you can set NEXUSMIND_API_KEY later and re-run setup.')
}

// 2. Read current settings
const settings = readSettings()

// 3. Merge MCP server
const mcpServers = (settings.mcpServers as Record<string, unknown>) ?? {}
mcpServers['nexusmind'] = {
  command: 'npx',
  args: ['-y', '@smart-coder-labs/nexusmind-mcp'],
  env: {
    NEXUSMIND_API_KEY: '${NEXUSMIND_API_KEY}',
    NEXUSMIND_BASE_URL: baseUrl,
  },
}
settings.mcpServers = mcpServers
info('MCP server entry added.')

// 4. Merge hooks
const hooksPath = join(PLUGIN_DIR, 'hooks', 'hooks.json')
if (existsSync(hooksPath)) {
  const pluginHooks = JSON.parse(readFileSync(hooksPath, 'utf8')) as {
    hooks: Record<string, unknown[]>
  }
  const existing = (settings.hooks as Record<string, unknown[]>) ?? {}

  for (const [event, entries] of Object.entries(pluginHooks.hooks)) {
    if (!existing[event]) existing[event] = []
    for (const entry of entries as Array<{ hooks?: Array<{ command?: string }>, command?: string }>) {
      // Resolve ${CLAUDE_PLUGIN_ROOT} to actual plugin dir
      const resolved = JSON.parse(
        JSON.stringify(entry).replaceAll('${CLAUDE_PLUGIN_ROOT}', PLUGIN_DIR)
      )
      // Dedup by command
      const existingCmds = (existing[event] as Array<{ command?: string, hooks?: Array<{ command?: string }> }>)
        .flatMap(e => [e.command, ...(e.hooks ?? []).map(h => h.command)])
        .filter(Boolean)
      const newCmd: string = resolved.command ?? resolved.hooks?.[0]?.command ?? ''
      if (!existingCmds.includes(newCmd)) {
        existing[event].push(resolved)
      }
    }
  }
  settings.hooks = existing
  info('Hooks merged.')
} else {
  warn(`Hooks not found at ${hooksPath} — skipping hooks installation.`)
}

// 5. Write settings
writeSettings(settings)
success(`Settings written to ${SETTINGS_PATH}`)

// 6. Persist env vars to shell rc files
function appendEnvVar(rcFile: string, name: string, value: string) {
  if (!existsSync(rcFile)) return
  const content = readFileSync(rcFile, 'utf8')
  if (content.includes(`export ${name}=`)) {
    warn(`${name} already in ${rcFile} — skipping.`)
    return
  }
  writeFileSync(rcFile, content + `\n# NexusMind\nexport ${name}="${value}"\n`)
  success(`Wrote ${name} to ${rcFile}`)
}

if (apiKey.trim()) {
  appendEnvVar(join(homedir(), '.zshrc'),  'NEXUSMIND_API_KEY', apiKey.trim())
  appendEnvVar(join(homedir(), '.bashrc'), 'NEXUSMIND_API_KEY', apiKey.trim())
}
appendEnvVar(join(homedir(), '.zshrc'),  'NEXUSMIND_BASE_URL', baseUrl)
appendEnvVar(join(homedir(), '.bashrc'), 'NEXUSMIND_BASE_URL', baseUrl)

// ── Done ──────────────────────────────────────────────────────────────────────

console.log(`\n${c.bold}${c.green}Done!${c.reset}\n`)
console.log('Next steps:')
console.log('  1. Restart your shell or run: source ~/.zshrc')
console.log('  2. Open Claude Code — NexusMind connects automatically')
console.log('  3. store_memory, search_memory, list_memories are now available\n')
