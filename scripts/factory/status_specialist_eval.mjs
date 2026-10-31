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

// Expose only allowlisted failure classes for diagnosing aggregate eval
// regressions. Review/judge prose can contain task text, paths or code, so it
// must never be copied into workflow logs.
const checkCodes = new Set([
  'doc_paths_only', 'links_resolve', 'identifiers_exist',
  'test_paths_only', 'test_reads_source', 'no_focused_or_skipped_tests',
  'tests_pass', 'mutation_score', 'seeded_fault_caught',
  'ui_paths_only', 'ui_build', 'ui_lint', 'ui_tests', 'ui_screenshots',
])
function reasonClass(reason) {
  if (typeof reason !== 'string') return null
  if (reason === 'no_change' || reason === 'checks_missing' || reason === 'checks:failed') return reason
  if (reason.startsWith('error:')) {
    const code = reason.slice('error:'.length)
    return `error:${/^[a-z][a-z0-9_]{0,79}$/.test(code) ? code : 'other'}`
  }
  if (reason.startsWith('checks:')) {
    const code = reason.slice('checks:'.length).split(':', 1)[0]
    return checkCodes.has(code) ? `checks:${code}` : 'checks:other'
  }
  if (reason.startsWith('specialist_review_rejected:')) return 'specialist_review_rejected'
  if (reason.startsWith('judge:')) return 'judge_rejected'
  if (reason.startsWith('merge_floor:')) return 'merge_floor'
  return 'other'
}

// Show only bounded runner events. Raw stderr, prompts, diffs and task content
// can contain repository data, so do not copy those into Actions logs.
const runnerEvents = lines(join(dir, 'runner.log')).flatMap(line => {
  const match = line.match(/^(\d{4}-\d\d-\d\dT[0-9:.]+Z) (.*)$/)
  if (!match) return []
  const [, timestamp, message] = match
  if (message === 'start' || message === 'finished') return [`${timestamp} ${message}`]
  const running = message.match(/^([A-Za-z0-9._-]+): running (\d+) of (\d+) tasks$/)
  if (running) return [`${timestamp} ${running[1]}: running ${running[2]} of ${running[3]} tasks`]
  const limited = message.match(/^([A-Za-z0-9._-]+): provider limit at ([A-Za-z0-9._-]+); waiting (\d+)s$/)
  if (limited) return [`${timestamp} ${limited[1]}: provider limit at ${limited[2]}; waiting ${limited[3]}s`]
  const outcome = message.match(/^([A-Za-z0-9._-]+): [A-Za-z0-9._-]+ (baseline|specialist) accepted=(true|false)$/)
  if (outcome) return [`${timestamp} ${outcome[1]}: ${outcome[2]} accepted=${outcome[3]}`]
  const failure = message.match(/^([A-Za-z0-9._-]+): failed \(exit (\d+)\):/)
  if (failure) return [`${timestamp} ${failure[1]}: failed (exit ${failure[2]})`]
  return []
}).slice(-20)

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
    const errorCodes = {}
    const rejectionClasses = {}
    for (const outcome of rows) {
      for (const reason of Array.isArray(outcome.reasons) ? outcome.reasons : []) {
        const category = reasonClass(reason)
        if (category) rejectionClasses[category] = (rejectionClasses[category] ?? 0) + 1
        if (typeof reason === 'string' && reason.startsWith('error:')) {
          const code = reason.slice('error:'.length)
          const safeCode = /^[a-z][a-z0-9_]{0,79}$/.test(code) ? code : 'other'
          errorCodes[safeCode] = (errorCodes[safeCode] ?? 0) + 1
        }
      }
    }
    return {
      arm,
      outcomes: rows.length,
      accepted: rows.filter(outcome => outcome.accepted === true).length,
      rejected: rows.filter(outcome => outcome.accepted === false).length,
      errors: rows.filter(isError).length,
      error_codes: errorCodes,
      rejection_classes: rejectionClasses,
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
  runner_events: runnerEvents,
  sets,
}, null, 2))
