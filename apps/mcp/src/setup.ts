#!/usr/bin/env node
/**
 * nexusmind-mcp setup
 * Registers the NexusMind plugin + MCP server in Claude Code config files.
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join, dirname } from 'node:path'
import * as readline from 'node:readline/promises'
import { stdin as input, stdout as output } from 'node:process'

const CLAUDE_DIR = join(homedir(), '.claude')
const SETTINGS_PATH = join(CLAUDE_DIR, 'settings.json')  // plugins + hooks live here
const CLAUDE_JSON_PATH = join(homedir(), '.claude.json')  // user MCPs live here
const GITHUB_REPO = 'smart-coder-labs/nexusmind-claude-plugin'
const MARKETPLACE_NAME = 'nexusmind'
const PLUGIN_KEY = `${MARKETPLACE_NAME}@${MARKETPLACE_NAME}`

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

function readJson(path: string): Record<string, unknown> {
  if (!existsSync(path)) return {}
  try { return JSON.parse(readFileSync(path, 'utf8')) } catch { return {} }
}

function writeJson(path: string, data: Record<string, unknown>) {
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, JSON.stringify(data, null, 2) + '\n')
}

// ── Main ──────────────────────────────────────────────────────────────────────

console.log(`\n${c.bold}NexusMind — Claude Code Setup${c.reset}`)
console.log('────────────────────────────────\n')

const rl = readline.createInterface({ input, output })

let apiKey = process.env.NEXUSMIND_API_KEY ?? ''
if (!apiKey) apiKey = await rl.question('NexusMind API key: ')

let baseUrl: string = process.env.NEXUSMIND_BASE_URL ?? ''
if (!baseUrl) baseUrl = await rl.question('NexusMind backend URL (e.g. http://localhost:8080): ')
rl.close()

if (!baseUrl) {
  err('Backend URL is required.')
  process.exit(1)
}

if (!apiKey.trim()) {
  warn('No API key provided — you can set NEXUSMIND_API_KEY later and re-run setup.')
}

// 1. Write MCP server to ~/.claude.json (user MCPs)
const claudeJson = readJson(CLAUDE_JSON_PATH)
const mcpServers = (claudeJson.mcpServers as Record<string, unknown>) ?? {}
mcpServers['nexusmind'] = {
  command: 'npx',
  args: ['-y', '@smart-coder-labs/nexusmind-mcp'],
  env: {
    NEXUSMIND_API_KEY: apiKey.trim(),
    NEXUSMIND_BASE_URL: baseUrl,
  },
}
claudeJson.mcpServers = mcpServers
writeJson(CLAUDE_JSON_PATH, claudeJson)
info(`MCP server written to ${CLAUDE_JSON_PATH}`)

// 2. Register plugin — Claude Code downloads hooks from GitHub automatically
const settings = readJson(SETTINGS_PATH)
const enabledPlugins = (settings.enabledPlugins as Record<string, boolean>) ?? {}
const extraMarketplaces = (settings.extraKnownMarketplaces as Record<string, unknown>) ?? {}

enabledPlugins[PLUGIN_KEY] = true
extraMarketplaces[MARKETPLACE_NAME] = {
  source: { repo: GITHUB_REPO, source: 'github' },
}
settings.enabledPlugins = enabledPlugins
settings.extraKnownMarketplaces = extraMarketplaces
writeJson(SETTINGS_PATH, settings)
info(`Plugin registered — hooks loaded from github.com/${GITHUB_REPO}`)

// 3. Persist env vars to shell rc files
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
console.log('  3. store_memory, search_memory, list_memories are now available')
console.log('')
console.log(`${c.yellow}Note:${c.reset} Run WITHOUT sudo. If you get npm permission errors:`)
console.log('  sudo chown -R $(whoami) ~/.npm\n')
