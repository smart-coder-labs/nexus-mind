import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const css = readFileSync(new URL('../src/index.css', import.meta.url), 'utf8')
const color = name => {
  const match = css.match(new RegExp(`--color-${name}: #([a-f0-9]{6});`, 'i'))
  assert.ok(match, `Missing ${name} token`)
  const channels = match[1].match(/../g).map(hex => parseInt(hex, 16) / 255)
    .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4)
  return channels.reduce((sum, channel, i) => sum + channel * [0.2126, 0.7152, 0.0722][i], 0)
}
const ratio = (a, b) => { const [x, y] = [color(a), color(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05) }
for (const background of ['bg-primary', 'surface-primary', 'surface-secondary']) {
  const contrast = ratio('text-quaternary', background)
  assert.ok(contrast >= 4.5, `${background}: quaternary contrast ${contrast.toFixed(2)} < 4.5`)
  // Signal: the green accent is also used as link/label text on these surfaces.
  const accent = ratio('accent-primary', background)
  assert.ok(accent >= 4.5, `${background}: accent text contrast ${accent.toFixed(2)} < 4.5`)
}
// Primary buttons put bg-primary ink on the green fill.
assert.ok(ratio('bg-primary', 'accent-primary') >= 4.5, 'button text on accent fill < 4.5')
assert.match(css, /@media\s*\(prefers-reduced-motion:\s*reduce\)/)
// The Signal layer is shared with the admin and must not drift between the two copies.
const signal = new URL('../src/styles/signal.css', import.meta.url)
assert.equal(readFileSync(signal, 'utf8'), readFileSync(new URL('../../admin/src/styles/signal.css', import.meta.url), 'utf8'), 'signal.css differs from apps/admin')
for (const [page, labels] of Object.entries({
  AuditLog: ['Search the audit log', 'Filter by action'],
  Orgs: ['Search organizations'],
  Search: ['Search organizations and users'],
  Users: ['Search users'],
})) {
  const source = readFileSync(new URL(`../src/pages/${page}.tsx`, import.meta.url), 'utf8')
  for (const label of labels) assert.ok(source.includes(`aria-label="${label}"`), `${page}: missing ${label}`)
}
console.log('Accessibility checks passed: text and accent contrast, reduced motion and five control labels')
