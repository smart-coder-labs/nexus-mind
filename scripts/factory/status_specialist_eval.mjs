import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const dir = '/data/factory-evals'
const lines = path => existsSync(path)
  ? readFileSync(path, 'utf8').split(/\r?\n/).filter(line => line.trim())
  : []
const parseLines = path => lines(path).flatMap(line => {
  try {
    const value = JSON.parse(line)
    return value && typeof value === 'object' && !Array.isArray(value) ? [value] : []
  } catch {
    return []
  }
})
const isError = outcome => Array.isArray(outcome.reasons)
  && outcome.reasons.some(reason => typeof reason === 'string' && reason.startsWith('error:'))

const files = readdirSync(dir)
const runFiles = files.filter(name => /^(?:(?:10|20|30)-)?(docs|tests|ui)-rerun-[0-9]+\.jsonl$/.test(name)).sort()
const sets = []
for (const file of runFiles) {
  const [, set] = file.match(/^(?:(?:10|20|30)-)?(docs|tests|ui)-rerun-[0-9]+\.jsonl$/)
  const total = lines(join(dir, file)).length
  const outcomes = parseLines(join(dir, file.replace(/\.jsonl$/, '.results.jsonl')))
    .filter(outcome => typeof outcome.id === 'string' && ['baseline', 'specialist'].includes(outcome.arm))
  const latest = new Map()
  const models = {}
  for (const outcome of outcomes) {
    latest.set(`${outcome.id}\0${outcome.arm}`, outcome)
    for (const step of Array.isArray(outcome.steps) ? outcome.steps : []) {
      const model = typeof step.model === 'string' && /^[A-Za-z0-9._/-]{1,100}$/.test(step.model)
        ? step.model
        : null
      if (model) models[model] = (models[model] ?? 0) + 1
    }
  }
  const current = [...latest.values()]
  const finishedIds = new Set()
  const byId = new Map()
  for (const outcome of current) {
    if (!byId.has(outcome.id)) byId.set(outcome.id, [])
    byId.get(outcome.id).push(outcome)
  }
  for (const [id, pair] of byId) {
    if (pair.length === 2 && pair.every(outcome => !isError(outcome))) finishedIds.add(id)
  }
  const arms = ['baseline', 'specialist'].map(arm => {
    const rows = current.filter(outcome => outcome.arm === arm)
    return {
      arm,
      outcomes: rows.length,
      accepted: rows.filter(outcome => outcome.accepted === true).length,
      rejected: rows.filter(outcome => outcome.accepted === false).length,
      errors: rows.filter(isError).length,
    }
  })
  const summaryPath = join(dir, file.replace(/\.jsonl$/, '.summary.json'))
  let specialistBeatsBaseline = null
  if (existsSync(summaryPath)) {
    try {
      const summary = JSON.parse(readFileSync(summaryPath, 'utf8'))
      if (typeof summary.specialist_beats_baseline === 'boolean') {
        specialistBeatsBaseline = summary.specialist_beats_baseline
      }
    } catch {}
  }
  sets.push({
    set,
    tasks_total: total,
    tasks_complete: finishedIds.size,
    tasks_remaining: Math.max(0, total - finishedIds.size),
    arms,
    model_steps: models,
    done: files.includes(file.replace(/\.jsonl$/, '.done')),
    specialist_beats_baseline: specialistBeatsBaseline,
  })
}

let runnerActive = false
for (const entry of readdirSync('/proc')) {
  if (!/^\d+$/.test(entry)) continue
  try {
    const argv = readFileSync(`/proc/${entry}/cmdline`, 'utf8').split('\0')
    const isRunner = argv.some(arg => arg.endsWith('/run.mjs') || arg.endsWith('/run_specialist_evals.mjs'))
    if (isRunner && argv.includes('--org') && argv.includes('--dir')) runnerActive = true
  } catch {}
}

console.log(JSON.stringify({
  runner_active: runnerActive,
  stop_marker: files.includes('STOP'),
  sets,
}, null, 2))
