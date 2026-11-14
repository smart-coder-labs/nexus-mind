#!/usr/bin/env node
// Static design audit for the NexusMind web apps (admin, backoffice, landing).
// Usage: node design-audit.mjs [files|dirs...]   (no args = files changed vs origin/main)
//        node design-audit.mjs --self-test
// Exit 1 when a MUST rule fails. Findings are prompts to look, not proof: read the line before "fixing" it.
import { readFileSync, statSync, readdirSync, existsSync } from 'node:fs';
import { join, extname } from 'node:path';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

// The only files allowed to hold literal colours (each app's token sheet).
// signal*.css land with PR #464; audit1-theme.css stays listed until that merges.
const TOKEN_FILES = [
  'apps/admin/src/styles/signal.css',
  'apps/admin/src/styles/signal-theme.css',
  'apps/admin/src/styles/audit1-theme.css',
  'apps/admin/src/index.css',
  'apps/backoffice/src/styles/signal.css',
  'apps/backoffice/src/index.css',
  'apps/landing/src/styles/smartcoder.css',
];
// WebGL/canvas graphs can't read CSS variables, so their palettes are literal by necessity: warn, don't fail.
const CANVAS = /(graph|Graph)[^/]*\.(tsx?|jsx?)$/;
const WEB_APPS = /^apps\/(admin|backoffice|landing)\//;
const EXTS = new Set(['.tsx', '.ts', '.jsx', '.js', '.css', '.astro']);
const SKIP = /(node_modules|dist\/|\.test\.|\.spec\.|__tests__|\/test\/|\/dev\/|\.claude\/)/;

const PALETTE = 'slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose';
// Lime (--sig, #c3d82e, Tailwind lime-*) is the Signal accent only: never a large fill, never text.
const LIME = '(?:sig|lime-\\d{2,3}|\\[(?:var\\(--sig\\)|#c3d82e)\\])';
const LIME_FILL = new RegExp(`(?<![:\\w-])bg-${LIME}(?:\\/(\\d+))?(?![\\w-])`, 'i');
const LIME_TEXT = new RegExp(`(?<![\\w-])text-${LIME}(?:\\/\\d+)?(?![\\w-])`, 'i');
// A dot or a hairline mark is an accent, not a surface.
const SMALL = /\b(?:size-(?:px|0\.5|1|1\.5|2|2\.5|3)|[wh]-(?:px|0\.5|1|1\.5|2))\b/;
const CSS_LIME = '(?:var\\(--sig\\)|#c3d82e)';
const CSS_LIME_TEXT = new RegExp(`(?:^|[;{\\s])color\\s*:\\s*${CSS_LIME}`, 'i');
const CSS_LIME_FILL = new RegExp(`background(?:-color)?\\s*:\\s*${CSS_LIME}\\s*[;}]`, 'i');
const CSS_SMALL = /\b(?:width|height)\s*:\s*[0-4]px/;
function limeMisuse(line) {
  const fill = line.match(LIME_FILL);
  if (fill && !(fill[1] && Number(fill[1]) <= 30) && !SMALL.test(line)) return true; // ≤30% alpha is a wash
  if (LIME_TEXT.test(line) || CSS_LIME_TEXT.test(line)) return true;
  return CSS_LIME_FILL.test(line) && !CSS_SMALL.test(line);
}

const RULES = [
  { id: 'D-TOK-01', level: 'MUST', msg: 'literal colour; use a token (references/tokens.md)',
    re: /(?<![\w&/-])(#[0-9a-fA-F]{3,4}|#[0-9a-fA-F]{6}|#[0-9a-fA-F]{8})(?![\w-])|\b(rgba?|hsla?|oklch)\(\s*\d/,
    skipLine: /href=|id=|url\(|^\s*\*|^\s*\/\/|\/\*|\b(?:issue|PR|pr)\s+#|placeholder=|hint=/ },
  { id: 'D-TOK-02', level: 'MUST', msg: 'Tailwind palette colour; use a semantic role (status-*, primary, muted-foreground, chart-*)',
    re: new RegExp(`\\b(?:text|bg|border|ring|fill|stroke|from|to|via|outline|divide|decoration|accent|caret|placeholder)-(?:${PALETTE})-\\d{2,3}\\b`) },
  { id: 'D-TOK-03', level: 'SHOULD', msg: 'arbitrary px value; prefer the Tailwind scale (text-xs=12, text-sm=14, gap-2=8px) or a token',
    re: /\b(?:p[xytblr]?|m[xytblr]?|gap(?:-[xy])?|space-[xy]|text|w|h|rounded)-\[\d+(?:\.\d+)?px\]/ },
  { id: 'D-COL-01', level: 'MUST', msg: 'lime used as a large-surface fill or as text; lime is the accent only (focus, live dot, small mark, gradient end). Use bg-primary / text-primary-tint-foreground',
    test: limeMisuse },
  { id: 'D-TOK-04', level: 'SHOULD', msg: 'text-white on a primary fill; use text-primary-foreground (white on forest green in light, ink on the dark-theme green)',
    re: /\bbg-(?:primary|action-primary)\b[^"'`]*\btext-white\b|\btext-white\b[^"'`]*\bbg-(?:primary|action-primary)\b/ },
];

function changedFiles() {
  try {
    const out = execFileSync('git', ['diff', '--name-only', '--diff-filter=ACMR', 'origin/main...HEAD'], { encoding: 'utf8' })
      + execFileSync('git', ['diff', '--name-only', '--diff-filter=ACMR'], { encoding: 'utf8' });
    return [...new Set(out.split('\n').filter(Boolean))].filter((p) => WEB_APPS.test(p) && existsSync(p));
  } catch { return []; }
}

function walk(p, acc = []) {
  if (SKIP.test(p)) return acc;
  const st = statSync(p);
  if (st.isDirectory()) for (const f of readdirSync(p)) walk(join(p, f), acc);
  else if (EXTS.has(extname(p))) acc.push(p);
  return acc;
}

export function auditSource(path, src) {
  const findings = [];
  const isTokenFile = TOKEN_FILES.some((t) => path.endsWith(t));
  const isCanvas = CANVAS.test(path);
  src.split('\n').forEach((line, i) => {
    for (const r of RULES) {
      if (isTokenFile && r.id !== 'D-TOK-03') continue;
      if (r.skipLine && r.skipLine.test(line)) continue;
      if (!(r.test ? r.test(line) : r.re.test(line))) continue;
      const level = isCanvas && r.id === 'D-TOK-01' ? 'SHOULD' : r.level;
      findings.push({ path, line: i + 1, ...r, level, text: line.trim().slice(0, 120) });
    }
  });
  if (/\.css$/.test(path) && /@keyframes|\btransition\s*:|\banimation\s*:/.test(src) && !/prefers-reduced-motion/.test(src))
    findings.push({ path, line: 0, id: 'D-MOT-01', level: 'MUST', msg: 'animation without a prefers-reduced-motion counterpart in this file', text: '' });
  for (const m of src.matchAll(/<img\b[^>]*>/gs))
    if (!/\balt\s*=/.test(m[0]))
      findings.push({ path, line: src.slice(0, m.index).split('\n').length, id: 'D-A11Y-01', level: 'MUST', msg: '<img> without alt', text: m[0].slice(0, 80) });
  for (const m of src.matchAll(/<button\b[^>]*>\s*<(?:svg|[A-Z]\w*)\b[^>]*\/>\s*<\/button>/gs))
    if (!/aria-label(?:ledby)?\s*=|title\s*=/.test(m[0]))
      findings.push({ path, line: src.slice(0, m.index).split('\n').length, id: 'D-A11Y-02', level: 'MUST', msg: 'icon-only button without aria-label', text: m[0].slice(0, 80) });
  return findings;
}

function selfTest() {
  const cases = [
    ['apps/admin/src/pages/X.tsx', '<div className="text-zinc-500" />', 'D-TOK-02'],
    ['apps/backoffice/src/pages/X.tsx', "<p className='text-red-400'>", 'D-TOK-02'],
    ['apps/admin/src/pages/X.tsx', "const c = '#c3d82e';", 'D-TOK-01'],
    ['apps/admin/src/pages/X.tsx', "bg: 'rgba(52,211,153,0.1)'", 'D-TOK-01'],
    ['apps/admin/src/pages/X.tsx', '<a href="#add">x</a>', null],
    ['apps/admin/src/pages/X.tsx', '// see issue #265', null],
    ['apps/admin/src/pages/X.tsx', '<div className="p-[13px]" />', 'D-TOK-03'],
    ['apps/admin/src/pages/X.tsx', '<button className="bg-primary text-white">', 'D-TOK-04'],
    ['apps/admin/src/pages/X.tsx', '<button className="bg-primary text-primary-foreground">', null],
    ['apps/admin/src/pages/X.tsx', '<img src="x.png" />', 'D-A11Y-01'],
    ['apps/admin/src/pages/X.tsx', '<button onClick={f}><Trash2 size={16} /></button>', 'D-A11Y-02'],
    ['apps/admin/src/pages/X.tsx', '<button aria-label="Delete" onClick={f}><Trash2 size={16} /></button>', null],
    ['apps/admin/src/x.css', '.x { transition: opacity 1s; }', 'D-MOT-01'],
    ['apps/admin/src/styles/audit1-theme.css', ':root { --x: #fff; }', null],
    ['apps/admin/src/styles/signal-theme.css', ':root { --primary:#356b26; }', null],
    // D-COL-01: lime is the accent only.
    ['apps/admin/src/pages/X.tsx', '<section className="bg-sig rounded-xl p-6">', 'D-COL-01'],
    ['apps/admin/src/pages/X.tsx', '<div className="bg-lime-400 p-4" />', 'D-COL-01'],
    ['apps/admin/src/pages/X.tsx', '<div className="h-40 bg-[#c3d82e]" />', 'D-COL-01'],
    ['apps/admin/src/pages/X.tsx', '<p className="text-sig">Body copy</p>', 'D-COL-01'],
    ['apps/backoffice/src/pages/X.tsx', '<span className="text-[var(--sig)] text-sm">', 'D-COL-01'],
    ['apps/admin/src/x.css', '.hero { background: var(--sig); }', 'D-COL-01'],
    ['apps/admin/src/x.css', '.lead { color: #c3d82e; }', 'D-COL-01'],
    ['apps/admin/src/pages/X.tsx', '<span aria-hidden="true" className="sig-live size-1.5" />', null],
    ['apps/admin/src/pages/X.tsx', '<span className="bg-sig size-2 rounded-full" />', null],
    ['apps/backoffice/src/components/X.tsx', "'before:h-4 before:bg-sig before:rounded-full'", null],
    ['apps/admin/src/pages/X.tsx', '<div className="bg-sig/10 p-4" />', null],
    ['apps/admin/src/pages/X.tsx', '<span className="bg-sig text-on-sig h-1 w-8" />', null],
    ['apps/admin/src/x.css', '.mark { width: 2px; background: var(--sig); }', null],
    ['apps/admin/src/styles/signal.css', '.sig-live { background: var(--sig); }', null],
    ['apps/landing/src/styles/smartcoder.css', ':root{--sc-lime:#c3d82e}', null],
    ['apps/admin/src/pages/graph/graphChrome.ts', "Memory: '#3b82f6',", 'D-TOK-01'],
    ['apps/admin/src/pages/X.tsx', '<div className="bg-primary text-primary-foreground text-muted-foreground p-4" />', null],
  ];
  let failed = 0;
  for (const [p, src, want] of cases) {
    const got = auditSource(p, src);
    const ids = got.map((f) => f.id);
    const ok = want ? ids.includes(want) : got.length === 0;
    if (!ok) { failed++; console.error(`self-test FAIL ${p}: ${src} → ${ids.join(',') || 'none'} (want ${want ?? 'none'})`); }
  }
  // Canvas palettes are demoted, not silenced.
  const canvas = auditSource('apps/admin/src/pages/graph/graphChrome.ts', "Memory: '#3b82f6',")[0];
  if (canvas?.level !== 'SHOULD') { failed++; console.error('self-test FAIL canvas palette should be SHOULD'); }
  console.log(failed ? `${failed} self-test case(s) failed` : `self-test: ${cases.length + 1} cases OK`);
  process.exit(failed ? 1 : 0);
}

function main() {
  const args = process.argv.slice(2);
  if (args.includes('--self-test')) selfTest();
  const files = (args.length ? args : changedFiles()).flatMap((p) => (existsSync(p) ? walk(p) : []));
  if (!files.length) { console.log('design-audit: no files to check'); process.exit(0); }

  const all = files.flatMap((f) => auditSource(f, readFileSync(f, 'utf8')));
  for (const f of all) console.log(`${f.level.padEnd(6)} ${f.id}  ${f.path}:${f.line}  ${f.msg}${f.text ? `\n         ${f.text}` : ''}`);
  const must = all.filter((f) => f.level === 'MUST').length;
  console.log(`\ndesign-audit: ${files.length} files · ${must} MUST · ${all.length - must} SHOULD`);
  process.exit(must ? 1 : 0);
}

// Run the CLI only when executed directly; the design-guard hook imports auditSource.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
