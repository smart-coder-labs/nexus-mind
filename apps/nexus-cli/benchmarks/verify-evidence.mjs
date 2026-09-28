import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), 'evidence', '2026-09-27');
const records = JSON.parse(readFileSync(path.join(root, 'summary.json'), 'utf8'));
const valid = records.filter((record) => record.exitCode === 0);
const censored = records.filter((record) => record.exitCode !== 0);
if (valid.length !== 12 || censored.length !== 2) throw new Error(`Expected 12 valid and 2 censored runs; got ${valid.length} and ${censored.length}`);

const counts = { codex: { visible: 0, heldout: 0, tokens: 0 }, nexus: { visible: 0, heldout: 0, tokens: 0 } };
for (const record of records) {
  const directory = path.join(root, `${record.case}-${record.arm}-${record.trial}`);
  for (const filename of ['timeline.md', 'metrics.json', 'implementation.diff', 'agent.stdout.log', 'visible-tests.log', 'heldout-tests.final.log']) {
    if (!existsSync(path.join(directory, filename))) throw new Error(`Missing ${directory}/${filename}`);
  }
  if (record.exitCode !== 0) continue;
  if (!record.testsUnmodified || !record.diffCheck || record.visible.fail !== 0) throw new Error(`Invalid delivery: ${directory}`);
  counts[record.arm].visible += record.visible.pass;
  counts[record.arm].heldout += record.heldout.pass;
  if (record.arm === 'nexus') {
    if (!existsSync(path.join(directory, 'codex-rollout-1.jsonl')) || !existsSync(path.join(directory, 'nexus-session.json'))) throw new Error(`Missing Nexus transcript: ${directory}`);
    if (record.codexDetailedUsage.usage.total_tokens !== record.usage.codex_total_tokens) throw new Error(`Nexus token mismatch: ${directory}`);
    if (record.decision?.selected !== 'human_review' || record.contextSourceCount !== 0) throw new Error(`Unexpected Nexus status/context: ${directory}`);
    counts.nexus.tokens += record.usage.codex_total_tokens + record.usage.jev_input_tokens + record.usage.jev_output_tokens;
  } else {
    counts.codex.tokens += record.usage.input_tokens + record.usage.output_tokens;
  }
}
if (counts.codex.visible !== 34 || counts.codex.heldout !== 26 || counts.codex.tokens !== 1_339_959) throw new Error('Codex aggregate differs from report');
if (counts.nexus.visible !== 34 || counts.nexus.heldout !== 25 || counts.nexus.tokens !== 693_381) throw new Error('Nexus aggregate differs from report');

const forbidden = /apikey_[A-Za-z0-9_]+|nm_[A-Za-z0-9_]{20,}|Bearer [A-Za-z0-9._-]{20,}|"role":"(?:developer|system)"|"type":"reasoning"/;
for (const record of records) {
  const directory = path.join(root, `${record.case}-${record.arm}-${record.trial}`);
  for (const filename of ['agent.stdout.log', 'timeline.md', 'codex-rollout-1.jsonl', 'codex-internal-prompts.txt', 'nexus-session.json']) {
    const location = path.join(directory, filename);
    if (existsSync(location) && forbidden.test(readFileSync(location, 'utf8'))) throw new Error(`Sensitive transcript content: ${location}`);
  }
}
process.stdout.write(JSON.stringify({ valid: valid.length, censored: censored.length, counts }, null, 2) + '\n');
