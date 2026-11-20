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
  // The worker adds the trusted `slack` server only for runs that deliver to Slack.
  const slack = config.mcpServers?.slack
  if (slack && (typeof slack.url !== 'string' || !slack.url.startsWith('https://'))) {
    throw new Error('pi_qa_mcp_config_invalid')
  }
  const servers = [{ name: 'playwright', config: { ...playwright, exposure: 'direct' }, source: path, scope: 'extension' }]
  if (slack) servers.push({ name: 'slack', config: { ...slack, exposure: 'direct' }, source: path, scope: 'extension' })
  return createMcpExtension({
    // Override default discovery: connect ONLY this run's trusted servers.
    loadConfig: () => ({
      servers,
      autoEnableCodemode: false,
      errors: [],
    }),
    logPath: `${path}.log`,
    startupWaitMs: 30_000,
  })(pi)
}
