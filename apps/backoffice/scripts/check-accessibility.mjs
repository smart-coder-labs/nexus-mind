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
for (const background of ['bg-primary', 'surface-primary', 'surface-secondary']) {
  const contrast = (color('text-quaternary') + 0.05) / (color(background) + 0.05)
  assert.ok(contrast >= 4.5, `${background}: quaternary contrast ${contrast.toFixed(2)} < 4.5`)
}
assert.match(css, /@media\s*\(prefers-reduced-motion:\s*reduce\)/)
for (const [page, labels] of Object.entries({
  AuditLog: ['Search the audit log', 'Filter by action'],
  Orgs: ['Search organizations'],
  Search: ['Search organizations and users'],
  Users: ['Search users'],
})) {
  const source = readFileSync(new URL(`../src/pages/${page}.tsx`, import.meta.url), 'utf8')
  for (const label of labels) assert.ok(source.includes(`aria-label="${label}"`), `${page}: missing ${label}`)
}
console.log('Accessibility checks passed: text contrast, reduced motion and five control labels')
