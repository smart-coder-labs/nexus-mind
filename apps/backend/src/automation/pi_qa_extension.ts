// Trusted factory extension, embedded in the worker binary. Never load a
// repository's or user's MCP configuration for a QA run.
import { readFileSync } from 'node:fs'
import { createMcpExtension, type ExtensionAPI } from '@earendil-works/pi-coding-agent'

export default function (pi: ExtensionAPI) {
  const path = process.env.FACTORY_PI_QA_MCP_CONFIG
  if (!path) throw new Error('pi_qa_mcp_config_missing')
  const config = JSON.parse(readFileSync(path, 'utf8'))
  const playwright = config.mcpServers?.playwright
  if (!playwright || typeof playwright.command !== 'string' || !Array.isArray(playwright.args)) {
    throw new Error('pi_qa_mcp_config_invalid')
  }
  return createMcpExtension({
    // Override default discovery: connect ONLY this run's Playwright server.
    loadConfig: () => ({
      servers: [{ name: 'playwright', config: { ...playwright, exposure: 'direct' }, source: path, scope: 'extension' }],
      autoEnableCodemode: false,
      errors: [],
    }),
    logPath: `${path}.log`,
    startupWaitMs: 30_000,
  })(pi)
}
