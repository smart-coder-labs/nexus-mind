import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';

const here = path.dirname(fileURLToPath(import.meta.url));
const [caseName, arm, trial, outputRoot] = process.argv.slice(2);
const cases = {
  web: {
    impl: 'src/app.js',
    prompt: 'Implementa una página de catálogo responsive en español en index.html, styles.css y src/app.js. Debe tener búsqueda sin distinguir acentos, filtro de categoría, orden por nombre/precio ascendente/descendente, estado vacío, controles etiquetados, HTML seguro frente a XSS y un layout adaptable. Mantén las exportaciones y pasa node --test; no modifiques las pruebas.',
  },
  corporate: {
    impl: 'src/approvals.js',
    prompt: 'Implementa el módulo ApprovalService para solicitudes de compra: validación de importe finito positivo e ID único, aprobación/rechazo solo por manager o admin ajeno al solicitante, transición solo desde pending, auditoría con actor/acción/fecha, reintentos idempotentes sin eventos duplicados y lecturas que no permitan mutar el estado interno. Mantén la API y pasa node --test; no modifiques las pruebas.',
  },
  service: {
    impl: 'src/server.js',
    prompt: 'Implementa un microservicio HTTP de pedidos sin dependencias externas: GET /health, POST /orders y GET /orders/:id, JSON consistente, validación estricta de sku y cantidad entera positiva, idempotencia por cabecera con 409 si la misma clave trae otro payload, 415 para content-type inválido, 413 para cuerpos mayores de 64 KiB y 404 JSON para rutas desconocidas. Mantén createServer y pasa node --test; no modifiques las pruebas.',
  },
};

if (!cases[caseName] || !['codex', 'nexus'].includes(arm) || !/^\d+$/.test(trial ?? '') || !outputRoot) {
  process.stderr.write('Usage: node run-realworld.mjs <web|corporate|service> <codex|nexus> <trial> <output-root>\n');
  process.exit(2);
}

const fixture = path.join(here, 'fixtures', caseName);
const runDir = path.resolve(outputRoot, `${caseName}-${arm}-${trial}`);
if (existsSync(runDir)) throw new Error(`Run directory already exists: ${runDir}`);
mkdirSync(runDir, { recursive: true });
cpSync(fixture, runDir, { recursive: true });
const git = (...args) => spawnSync('git', ['-C', runDir, ...args], { encoding: 'utf8', check: true });
for (const args of [['init', '-q'], ['config', 'user.name', 'Benchmark'], ['config', 'user.email', 'benchmark@example.invalid'], ['add', '.'], ['commit', '-qm', 'fixture']]) {
  const result = git(...args);
  if (result.status !== 0) throw new Error(`Git setup failed: ${result.stderr}`);
}

const config = cases[caseName];
const codexBin = process.env.BENCH_CODEX_BIN ?? '/private/tmp/nexus-bench-codex-0154/node_modules/.bin/codex';
const command = arm === 'codex' ? codexBin : (process.env.BENCH_NEXUS_BIN ?? 'nexus');
const args = arm === 'codex'
  ? ['exec', '--json', '--ephemeral', '--ignore-user-config', '--sandbox', 'workspace-write', '-c', 'approval_policy=never', '-c', 'shell_environment_policy.ignore_default_excludes=false', '-m', 'gpt-5.6-terra', '-C', runDir, config.prompt]
  : ['--repository', runDir, '--runtime', 'codex-headless', '--decision-engine', 'jev', 'run', config.prompt];
const environment = { ...process.env };
if (arm === 'nexus') {
  for (const name of ['TYPESAFE_API_KEY', 'NEXUSMIND_API_KEY']) {
    if (!environment[name]) throw new Error(`${name} must be supplied in the process environment`);
  }
  Object.assign(environment, {
    NEXUSMIND_BASE_URL: 'https://api.nexusmind.smartcoderlabs.com',
    NEXUSMIND_REQUIRED: '1',
    NEXUS_OPENSHELL_IMAGE: 'localhost/nexus-openshell-rust:7dff79461de6',
    NEXUS_CODEX_MODEL: 'gpt-5.6-terra',
    NEXUS_VERIFY_COMMAND: 'node --test',
    NEXUS_ACCEPTANCE_COMMAND: 'node --test',
  });
}

process.stderr.write(`Starting ${caseName}/${arm}/trial-${trial}\n`);
const startedAt = new Date().toISOString();
const start = performance.now();
const result = spawnSync(command, args, {
  cwd: runDir, env: environment, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 15 * 60 * 1000,
});
const elapsedSeconds = Math.round((performance.now() - start) / 100) / 10;
const redact = (value) => String(value ?? '').replace(/apikey_[A-Za-z0-9_]+/g, '[REDACTED_JEV_KEY]').replace(/nm_[A-Za-z0-9_]+/g, '[REDACTED_NEXUSMIND_KEY]');
writeFileSync(path.join(runDir, 'agent.stdout.log'), redact(result.stdout));
writeFileSync(path.join(runDir, 'agent.stderr.log'), redact(result.stderr));

function testResult(file, extraEnv = {}) {
  const check = spawnSync('node', ['--test', file], { cwd: runDir, env: { ...environment, ...extraEnv }, encoding: 'utf8', timeout: 60_000 });
  const log = redact((check.stdout ?? '') + (check.stderr ?? ''));
  return { exitCode: check.status, pass: Number(log.match(/ℹ pass (\d+)/)?.[1] ?? 0), fail: Number(log.match(/ℹ fail (\d+)/)?.[1] ?? 0), log };
}
const visible = testResult(path.join(runDir, 'test', `${caseName === 'corporate' ? 'approvals' : caseName === 'service' ? 'server' : 'web'}.test.js`));
const hidden = testResult(path.join(here, 'heldout', `${caseName}.test.mjs`), { BENCH_IMPL: path.join(runDir, config.impl) });
writeFileSync(path.join(runDir, 'visible-tests.log'), visible.log);
writeFileSync(path.join(runDir, 'heldout-tests.log'), hidden.log);

const diff = git('diff', '--', 'src', 'index.html', 'styles.css');
writeFileSync(path.join(runDir, 'implementation.diff'), redact(diff.stdout));
const diffCheck = git('diff', '--check');
const changedPaths = git('diff', '--name-only').stdout.trim().split('\n').filter(Boolean);
const testFile = path.join(runDir, 'test', `${caseName === 'corporate' ? 'approvals' : caseName === 'service' ? 'server' : 'web'}.test.js`);
const fixtureTestFile = path.join(fixture, 'test', path.basename(testFile));
const testsUnmodified = createHash('sha256').update(readFileSync(testFile)).digest('hex') === createHash('sha256').update(readFileSync(fixtureTestFile)).digest('hex');

let usage = null;
let decision = null;
let nexusSession = null;
if (arm === 'codex') {
  const events = (result.stdout ?? '').split('\n').filter(Boolean).flatMap((line) => { try { return [JSON.parse(line)]; } catch { return []; } });
  usage = events.filter((event) => event.type === 'turn.completed').at(-1)?.usage ?? null;
} else {
  const sessionsDir = path.join(runDir, '.nexus', 'sessions');
  const sessions = existsSync(sessionsDir) ? readdirSync(sessionsDir).filter((name) => name.endsWith('.json')) : [];
  const entries = sessions.map((name) => JSON.parse(readFileSync(path.join(sessionsDir, name), 'utf8')));
  nexusSession = entries.sort((a, b) => String(b.updated_at).localeCompare(String(a.updated_at)))[0] ?? null;
  const last = nexusSession?.decisions?.at(-1);
  const match = last?.reason?.match(/tokens (\d+)\/(\d+)/);
  usage = { codex_total_tokens: nexusSession?.state?.tokens_used ?? null, jev_input_tokens: match ? Number(match[1]) : null, jev_output_tokens: match ? Number(match[2]) : null };
  decision = last ? { selected: last.selected, confidence: last.confidence, reason: last.reason } : null;
}

const metrics = {
  case: caseName, arm, trial: Number(trial), startedAt, elapsedSeconds,
  tool: arm === 'codex' ? 'codex-cli 0.154.0' : 'nexus + OpenShell + codex-cli 0.154.0 + JEV + NexusMind',
  model: 'gpt-5.6-terra', prompt: config.prompt, exitCode: result.status, signal: result.signal,
  usage, nexusStatus: nexusSession?.status ?? null, decision,
  visible: { exitCode: visible.exitCode, pass: visible.pass, fail: visible.fail },
  heldout: { exitCode: hidden.exitCode, pass: hidden.pass, fail: hidden.fail },
  changedPaths, testsUnmodified, diffCheck: diffCheck.status === 0,
  contextSourceCount: nexusSession?.state?.context_sources?.length ?? null,
  hashes: {
    fixtureTestSha256: createHash('sha256').update(readFileSync(fixtureTestFile)).digest('hex'),
    implementationDiffSha256: createHash('sha256').update(diff.stdout).digest('hex'),
    stdoutSha256: createHash('sha256').update(result.stdout ?? '').digest('hex'),
  },
};
writeFileSync(path.join(runDir, 'metrics.json'), JSON.stringify(metrics, null, 2) + '\n');
process.stdout.write(JSON.stringify({ case: caseName, arm, trial, exitCode: metrics.exitCode, elapsedSeconds, usage, visible: metrics.visible, heldout: metrics.heldout, decision: decision?.selected, runDir }) + '\n');
