// PostToolUse (Edit|Write) — runs the design audit on the edited web-app file and reports only violations the
// edit ADDED. The admin already carries hundreds of legacy findings; nagging about those would teach the agent
// to ignore this hook. No shell: paths come from tool input and go to git/fs as plain arguments.
const { execFileSync } = require('child_process');
const { readFileSync, existsSync } = require('fs');
const { relative } = require('path');
const { pathToFileURL } = require('url');

let data = '';
process.stdin.on('data', (chunk) => (data += chunk));
process.stdin.on('end', async () => {
  const root = process.env.CLAUDE_PROJECT_DIR || process.cwd();
  let abs = '';
  try { abs = JSON.parse(data).tool_input?.file_path || ''; } catch { return; }
  const path = relative(root, abs);
  if (!/^apps\/(admin|backoffice|landing)\/.*\.(tsx?|jsx?|css|astro)$/.test(path) || !existsSync(abs)) return;

  const { auditSource } = await import(pathToFileURL(`${root}/.claude/skills/nexus-mind-design/scripts/design-audit.mjs`).href);
  let before = '';
  try {
    before = execFileSync('git', ['show', `HEAD:${path}`], { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
  } catch {} // new file: everything in it is new
  const key = (f) => `${f.id}|${f.text}`;
  const old = new Set(auditSource(path, before).map(key));
  const added = auditSource(path, readFileSync(abs, 'utf8')).filter((f) => !old.has(key(f)));
  if (!added.length) return;

  const lines = added.map((f) => `${f.level} ${f.id} ${path}:${f.line} ${f.msg}${f.text ? ` — ${f.text}` : ''}`);
  process.stderr.write(`Design audit (nexus-mind-design skill) on lines this edit added:\n${lines.join('\n')}\n`);
  process.exit(added.some((f) => f.level === 'MUST') ? 2 : 0);
});
