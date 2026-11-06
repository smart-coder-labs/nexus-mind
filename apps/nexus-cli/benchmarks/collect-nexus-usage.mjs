import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';

const root = process.argv[2];
if (!root) throw new Error('Usage: node collect-nexus-usage.mjs <benchmark-output-root>');
const image = 'localhost/nexus-openshell-rust:7dff79461de6';
const redact = (value) => value.replace(/apikey_[A-Za-z0-9_]+/g, '[REDACTED_JEV_KEY]')
  .replace(/nm_[A-Za-z0-9_]{20,}/g, '[REDACTED_NEXUSMIND_KEY]')
  .replace(/sk-[A-Za-z0-9_-]{20,}/g, '[REDACTED_OPENAI_KEY]')
  .replace(/Bearer\s+[A-Za-z0-9._-]{20,}/gi, 'Bearer [REDACTED_TOKEN]');
function operationalEvent(event) {
  if (event.type === 'response_item') {
    if (event.payload?.type === 'message') return ['user', 'assistant'].includes(event.payload.role);
    return ['custom_tool_call', 'custom_tool_call_output'].includes(event.payload?.type);
  }
  if (event.type === 'event_msg') {
    return ['task_started', 'task_complete', 'token_count', 'item_completed'].includes(event.payload?.type)
      && event.payload?.item?.type !== 'reasoning';
  }
  return event.type === 'token_usage_record';
}
const list = spawnSync('docker', ['ps', '-a', '--format', '{{.Names}}'], { encoding: 'utf8' });
if (list.status !== 0) throw new Error(`Could not list Docker containers: ${list.stderr}`);
const containers = list.stdout.trim().split('\n').filter(Boolean);

function filesUnder(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const location = path.join(directory, entry.name);
    return entry.isDirectory() ? filesUnder(location) : entry.name.endsWith('.jsonl') ? [location] : [];
  });
}

for (const name of readdirSync(root).filter((value) => /^(web|corporate|service)-nexus-\d+$/.test(value))) {
  const runDir = path.join(root, name);
  if (!existsSync(path.join(runDir, 'metrics.json'))) continue;
  const hash = createHash('sha256').update(`${runDir}:codex-headless:${image}`).digest('hex').slice(0, 12);
  const sandbox = `nx-x-${hash}`;
  const container = containers.find((value) => value.includes(`--${sandbox}-`));
  if (!container) {
    process.stderr.write(`No Docker container found for ${name} (${sandbox})\n`);
    continue;
  }
  const copied = path.join(runDir, 'codex-sessions-copy');
  if (!existsSync(copied)) {
    const result = spawnSync('docker', ['cp', `${container}:/sandbox/.nexus-codex-auth/sessions`, copied], { encoding: 'utf8', timeout: 60_000 });
    if (result.status !== 0) {
      process.stderr.write(`Could not copy session files for ${name}: ${result.stderr}\n`);
      continue;
    }
  }
  const files = filesUnder(copied);
  const readings = [];
  files.forEach((file, index) => {
    const content = readFileSync(file, 'utf8');
    const operational = [];
    let last = null;
    for (const line of content.split('\n')) {
      if (!line) continue;
      try {
        const event = JSON.parse(line);
        if (operationalEvent(event)) operational.push(redact(JSON.stringify(event)));
        if (event.payload?.type === 'token_count' && event.payload.info?.total_token_usage) {
          last = event.payload.info.total_token_usage;
        }
      } catch { /* Malformed line omitted; no guessed usage. */ }
    }
    writeFileSync(path.join(runDir, `codex-rollout-${index + 1}.jsonl`), operational.join('\n') + '\n');
    if (last) readings.push(last);
  });
  const usage = readings.reduce((total, current) => {
    for (const key of ['input_tokens', 'cached_input_tokens', 'cache_write_input_tokens', 'output_tokens', 'reasoning_output_tokens', 'total_tokens']) total[key] += Number(current[key] ?? 0);
    return total;
  }, { input_tokens: 0, cached_input_tokens: 0, cache_write_input_tokens: 0, output_tokens: 0, reasoning_output_tokens: 0, total_tokens: 0 });
  writeFileSync(path.join(runDir, 'codex-usage.json'), JSON.stringify({ sandbox, container, sessions: files.length, usage }, null, 2) + '\n');
  const metric = JSON.parse(readFileSync(path.join(runDir, 'metrics.json'), 'utf8'));
  process.stdout.write(`${name}: ${usage.total_tokens} Codex tokens; ${metric.usage?.codex_total_tokens} reported by Nexus; ${usage.cached_input_tokens} cached\n`);
}
