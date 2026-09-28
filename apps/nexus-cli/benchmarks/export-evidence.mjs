import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const sourceRoot = process.argv[2];
if (!sourceRoot) {
  process.stderr.write('Usage: node export-evidence.mjs <benchmark-output-root>\n');
  process.exit(2);
}
const here = path.dirname(fileURLToPath(import.meta.url));
const evidenceRoot = path.join(here, 'evidence', '2026-09-27');
mkdirSync(evidenceRoot, { recursive: true });
const names = readdirSync(sourceRoot).filter((name) => /^(web|corporate|service)-(codex|nexus)-\d+$/.test(name));
const redact = (value) => value.replace(/apikey_[A-Za-z0-9_]+/g, '[REDACTED_JEV_KEY]')
  .replace(/nm_[A-Za-z0-9_]{20,}/g, '[REDACTED_NEXUSMIND_KEY]');

function parseTestOutput(output) {
  return { pass: Number(output.match(/ℹ pass (\d+)/)?.[1] ?? 0), fail: Number(output.match(/ℹ fail (\d+)/)?.[1] ?? 0) };
}
const short = (value, limit = 180) => String(value ?? '').replace(/\s+/g, ' ').slice(0, limit).replace(/\|/g, '\\|');
function parseJsonl(location) {
  return readFileSync(location, 'utf8').split('\n').filter(Boolean).flatMap((line) => {
    try { return [JSON.parse(line)]; } catch { return []; }
  });
}
function writeTimeline(destination, metric, session) {
  const lines = [
    `# ${metric.case} / ${metric.arm} / iteración ${metric.trial}`,
    '',
    '## Pedido exacto de esta iteración',
    '',
    `> ${metric.prompt}`,
    '',
    '## Eventos de ejecución',
    '',
    '| Hora UTC | Etapa/herramienta | Evidencia abreviada |',
    '| --- | --- | --- |',
  ];
  const events = metric.arm === 'codex'
    ? parseJsonl(path.join(destination, 'agent.stdout.log'))
    : readdirSync(destination).filter((filename) => /^codex-rollout-\d+\.jsonl$/.test(filename))
      .flatMap((filename) => parseJsonl(path.join(destination, filename)));
  const internalPrompts = [];
  for (const event of events) {
    if (metric.arm === 'codex') {
      if (event.type === 'item.started' || event.type === 'item.completed') {
        const item = event.item ?? {};
        if (item.type === 'command_execution') lines.push(`| orden ${lines.length - 8} | ${item.type} (${event.type}) | ${short(item.command)} |`);
        else if (item.type === 'mcp_tool_call') lines.push(`| orden ${lines.length - 8} | ${item.type} (${event.type}) | ${short(item.server + '.' + item.tool)} |`);
        else if (item.type === 'file_change') lines.push(`| orden ${lines.length - 8} | ${item.type} (${event.type}) | ${short(item.changes?.map((change) => change.path).join(', '))} |`);
      }
      continue;
    }
    const timestamp = event.timestamp ?? '—';
    if (event.type === 'response_item' && event.payload?.type === 'message' && event.payload.role === 'user') {
      for (const content of event.payload.content ?? []) if (content.type === 'input_text') internalPrompts.push(content.text);
    }
    if (event.type === 'response_item' && event.payload?.type === 'custom_tool_call') {
      const input = String(event.payload.input ?? '');
      const nested = [...new Set([...input.matchAll(/tools\.([A-Za-z_0-9]+)/g)].map((match) => match[1]))];
      lines.push(`| ${timestamp} | ${short(event.payload.name)}${nested.length ? ' → ' + nested.join(', ') : ''} | ${short(input)} |`);
    }
  }
  if (internalPrompts.length) writeFileSync(path.join(destination, 'codex-internal-prompts.txt'), internalPrompts.join('\n\n--- NEXT USER MESSAGE ---\n\n'));
  if (metric.arm === 'nexus') {
    const checks = session?.state?.verification?.commands ?? [];
    for (const command of checks) lines.push(`| antes de JEV | verificación OpenShell | ${short(command)} |`);
    for (const decision of session?.decisions ?? []) {
      lines.push(`| ${decision.timestamp ?? '—'} | JEV (${short(decision.decision_type)}) | ${short(decision.reason, 300)}; selección: ${short(decision.selected)} |`);
    }
    if (!session?.decisions?.length) lines.push('| — | JEV no invocado | La ejecución terminó antes de la fase de decisión. |');
  }
  lines.push('', 'La transcripción JSONL conserva mensajes operativos, llamadas a herramientas y sus resultados. Se omiten instrucciones internas, razonamiento privado y credenciales. El cuerpo HTTP completo de la petición/respuesta de JEV no fue persistido por esta versión del harness; `nexus-session.json` conserva la decisión y su marca temporal.', '');
  writeFileSync(path.join(destination, 'timeline.md'), lines.join('\n'));
}
function reconstructJevRequest(session) {
  const state = session.state;
  return {
    note: 'Reconstrucción desde el estado guardado y request_body() de src/jev.rs; NO es una captura del cuerpo HTTP.',
    body: {
      model: 'jev-latest',
      state: {
        objective: state.objective,
        requirements_total: state.requirements_total,
        requirements_covered: state.requirements_covered,
        unresolved_requirements: state.unresolved_requirements,
        changed_files: state.changed_files,
        changed_symbols: state.changed_symbols,
        change_summary: state.change_summary,
        impact_radius: state.impact_radius,
        affected_processes: state.affected_processes,
        affected_contracts: state.affected_contracts,
        context_sources: state.context_sources,
        verification: {
          commands_executed: state.verification.commands.length,
          tests_passed: state.verification.tests_passed,
          tests_failed: state.verification.tests_failed,
          lint_passed: state.verification.lint_passed,
          typecheck_passed: state.verification.typecheck_passed,
        },
        unresolved_evidence: state.unresolved_evidence,
        attempts: state.attempts,
        tool_errors: state.tool_errors,
      },
      questions: {
        next_action: {
          type: 'choice',
          instructions: 'Based only on the supplied task evidence, what is the next action? Choose human_review when the evidence cannot support a confident decision.',
          criteria: {
            finish: 'All requirements are covered, no known gaps or failures remain, and relevant verification passed.',
            retrieve_more: 'Requirements, scope, or implementation evidence is insufficient and more context is needed.',
            run_tests: 'Implementation may be complete, but relevant verification is missing or incomplete.',
            retry: 'Known implementation or verification failure requires another attempt.',
            human_review: 'The supplied evidence is ambiguous, conflicting, or needs a person\'s judgment.',
          },
        },
        task_is_incomplete: {
          type: 'noul',
          instructions: 'Does the supplied evidence show that the objective is incomplete?',
        },
      },
    },
  };
}
const records = [];
for (const name of names) {
  const runDir = path.join(sourceRoot, name);
  const original = JSON.parse(readFileSync(path.join(runDir, 'metrics.json'), 'utf8'));
  const destination = path.join(evidenceRoot, name);
  mkdirSync(destination, { recursive: true });
  for (const filename of ['agent.stdout.log', 'agent.stderr.log', 'visible-tests.log', 'implementation.diff']) {
    cpSync(path.join(runDir, filename), path.join(destination, filename));
  }
  cpSync(path.join(runDir, 'heldout-tests.log'), path.join(destination, 'heldout-tests.initial.log'));
  if (existsSync(path.join(runDir, 'codex-usage.json'))) {
    cpSync(path.join(runDir, 'codex-usage.json'), path.join(destination, 'codex-usage.json'));
  }
  for (const filename of readdirSync(runDir).filter((value) => /^codex-rollout-\d+\.jsonl$/.test(value))) {
    cpSync(path.join(runDir, filename), path.join(destination, filename));
  }
  const sessionsDir = path.join(runDir, '.nexus', 'sessions');
  let newest = null;
  if (existsSync(sessionsDir)) {
    const sessions = readdirSync(sessionsDir).filter((filename) => filename.endsWith('.json'));
    newest = sessions.map((filename) => JSON.parse(readFileSync(path.join(sessionsDir, filename), 'utf8')))
      .sort((a, b) => String(b.updated_at).localeCompare(String(a.updated_at)))[0];
    if (newest) writeFileSync(path.join(destination, 'nexus-session.json'), redact(JSON.stringify(newest, null, 2)) + '\n');
    if (newest?.decisions?.length) writeFileSync(path.join(destination, 'jev-request-reconstruction.json'), redact(JSON.stringify(reconstructJevRequest(newest), null, 2)) + '\n');
  }
  const implementation = original.case === 'web' ? 'src/app.js' : original.case === 'corporate' ? 'src/approvals.js' : 'src/server.js';
  mkdirSync(path.join(destination, 'src'), { recursive: true });
  cpSync(path.join(runDir, implementation), path.join(destination, implementation));
  if (original.case === 'web') {
    for (const filename of ['index.html', 'styles.css']) cpSync(path.join(runDir, filename), path.join(destination, filename));
  }
  const check = spawnSync('node', ['--test', path.join(here, 'heldout', `${original.case}.test.mjs`)], {
    cwd: runDir, env: { ...process.env, BENCH_IMPL: path.join(runDir, implementation) }, encoding: 'utf8', timeout: 60_000,
  });
  const output = String(check.stdout ?? '') + String(check.stderr ?? '');
  writeFileSync(path.join(destination, 'heldout-tests.final.log'), output);
  const final = {
    ...original,
    heldoutInitial: original.heldout,
    heldout: { exitCode: check.status, ...parseTestOutput(output) },
    codexDetailedUsage: existsSync(path.join(runDir, 'codex-usage.json'))
      ? JSON.parse(readFileSync(path.join(runDir, 'codex-usage.json'), 'utf8'))
      : null,
    evidence: {
      initialRunnerMetrics: path.join(runDir, 'metrics.json'),
      initialHeldoutLog: path.join(runDir, 'heldout-tests.log'),
      finalHeldoutRubric: path.join(here, 'heldout', `${original.case}.test.mjs`),
    },
  };
  writeFileSync(path.join(destination, 'metrics.json'), JSON.stringify(final, null, 2) + '\n');
  writeTimeline(destination, final, newest);
  records.push(final);
}

const forbidden = /apikey_[A-Za-z0-9_]+|nm_[A-Za-z0-9_]{20,}/g;
for (const name of names) {
  const files = [
    'agent.stdout.log', 'agent.stderr.log', 'visible-tests.log', 'heldout-tests.initial.log', 'heldout-tests.final.log',
    'heldout-tests.export-sandbox-error.log',
    'implementation.diff', 'metrics.json', 'src/app.js', 'src/approvals.js', 'src/server.js',
    'index.html', 'styles.css', 'codex-usage.json', 'nexus-session.json',
    'timeline.md', 'codex-internal-prompts.txt', 'jev-request-reconstruction.json',
    ...readdirSync(path.join(evidenceRoot, name)).filter((value) => /^codex-rollout-\d+\.jsonl$/.test(value)),
  ];
  for (const filename of files) {
    const location = path.join(evidenceRoot, name, filename);
    if (existsSync(location) && forbidden.test(readFileSync(location, 'utf8'))) {
      throw new Error(`Evidence contains a credential-shaped string: ${name}/${filename}`);
    }
    forbidden.lastIndex = 0;
  }
}
writeFileSync(path.join(evidenceRoot, 'summary.json'), JSON.stringify(records, null, 2) + '\n');
process.stdout.write(`Exported ${records.length} runs to ${evidenceRoot}\n`);
