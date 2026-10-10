#!/usr/bin/env node
// Runs the frozen specialist evals unattended, inside the autonomous-worker
// container, and survives the Claude usage limit.
//
// `factory-specialist-eval` stops at the first provider error (a usage limit
// or an API error) and prints `stopped_at`. One subscription window only fits
// a few tasks, so this runner waits and resumes from that task until every set
// is finished. Results are appended per set, and a task already in the results
// is never run again, so the runner can also be restarted (for example after a
// deploy replaced the pod) and continues where it was.
//
// Usage, in the worker container:
//   node run_specialist_evals.mjs --org <org_id> --dir /tmp/evals \
//     [--retry-secs 1800] [--max-wait-hours 14] [--bin /app/factory-specialist-eval]
//   node run_specialist_evals.mjs --self-test
//
// <dir> holds the sets, run in name order: `10-docs.jsonl`, `20-tests.jsonl`…
// For each set it writes `<set>.results.jsonl` (one outcome per line),
// `<set>.summary.json` once finished, and `<set>.done`; `runner.log` logs it all.
// Nothing here reads or prints a credential.

import { spawn } from 'node:child_process'
import { appendFileSync, existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const ARMS = ['baseline', 'specialist']
// A run that fails without `stopped_at` (a bad task, a crash) is retried this
// many times, then the set is skipped so one broken set never blocks the rest.
const MAX_OTHER_FAILURES = 2

function args(argv) {
  const out = { retrySecs: 1800, maxWaitHours: 14, bin: '/app/factory-specialist-eval' }
  for (let i = 0; i < argv.length; i++) {
    const [key, value] = [argv[i], argv[i + 1]]
    if (key === '--self-test') out.selfTest = true
    else if (key === '--org') (out.org = value), i++
    else if (key === '--dir') (out.dir = value), i++
    else if (key === '--retry-secs') (out.retrySecs = Number(value)), i++
    else if (key === '--max-wait-hours') (out.maxWaitHours = Number(value)), i++
    else if (key === '--bin') (out.bin = value), i++
    else throw new Error(`unknown argument ${key}`)
  }
  return out
}

const readLines = path =>
  existsSync(path) ? readFileSync(path, 'utf8').split('\n').filter(line => line.trim()) : []

/** Tasks of a set that have no outcome yet. The eval reports a task only once
 * every arm finished it, so one outcome line means the task is done. */
export function remaining(taskLines, outcomes) {
  const done = new Set(outcomes.map(outcome => outcome.id))
  return taskLines.filter(line => !done.has(JSON.parse(line).id))
}

/** The eval's summary (`specialist_eval::summarize`) over every outcome of a
 * set, including those from earlier resumed runs. */
export function summarize(outcomes) {
  const arm = name => {
    const of = outcomes.filter(outcome => outcome.arm === name)
    if (!of.length) return null
    const accepted = of.filter(outcome => outcome.accepted).length
    const known = of.every(outcome => typeof outcome.cost_usd === 'number')
    const cost = known ? of.reduce((sum, outcome) => sum + outcome.cost_usd, 0) : null
    return {
      arm: name,
      tasks: of.length,
      accepted,
      acceptance_rate: accepted / of.length,
      cost_usd: cost,
      cost_per_accepted_change: cost !== null && accepted > 0 ? cost / accepted : null,
      judge_cost_usd: of.reduce((sum, outcome) => sum + (outcome.judge_cost_usd || 0), 0),
    }
  }
  const [baseline, specialist] = ARMS.map(arm)
  let beats = null
  if (baseline && specialist) {
    const cheaper =
      specialist.cost_per_accepted_change !== null &&
      baseline.cost_per_accepted_change !== null &&
      specialist.cost_per_accepted_change < baseline.cost_per_accepted_change
    beats =
      specialist.acceptance_rate > baseline.acceptance_rate ||
      (specialist.acceptance_rate === baseline.acceptance_rate && specialist.accepted > 0 && cheaper)
  }
  return { arms: [baseline, specialist].filter(Boolean), specialist_beats_baseline: beats }
}

function runOnce(opts, tasks, onOutcome) {
  return new Promise(resolve => {
    const child = spawn(opts.bin, [opts.org], { stdio: ['pipe', 'pipe', 'pipe'] })
    let buffer = ''
    let summary = null
    let stderr = ''
    child.stdout.on('data', chunk => {
      buffer += chunk
      let newline
      while ((newline = buffer.indexOf('\n')) >= 0) {
        const line = buffer.slice(0, newline).trim()
        buffer = buffer.slice(newline + 1)
        if (!line) continue
        try {
          const value = JSON.parse(line)
          if (value.summary) summary = value
          else onOutcome(value, line)
        } catch {
          // Not JSON: never expected on stdout; ignore rather than crash.
        }
      }
    })
    child.stderr.on('data', chunk => (stderr = (stderr + chunk).slice(-2000)))
    child.on('close', code => resolve({ code, summary, stderr: stderr.trim() }))
    child.stdin.end(tasks.join('\n') + '\n')
  })
}

const sleep = secs => new Promise(resolve => setTimeout(resolve, secs * 1000))

async function main() {
  const opts = args(process.argv.slice(2))
  if (opts.selfTest) return selfTest()
  if (!opts.org || !opts.dir) throw new Error('--org and --dir are required')
  const logPath = join(opts.dir, 'runner.log')
  const log = message => appendFileSync(logPath, `${new Date().toISOString()} ${message}\n`)
  // The folder is read again after every set, so a set added while the runner
  // works is picked up without a restart. A set is attempted once per runner.
  const attempted = new Set()
  const nextSet = () =>
    readdirSync(opts.dir)
      .filter(name => name.endsWith('.jsonl') && !name.endsWith('.results.jsonl') && !name.startsWith('.'))
      .sort()
      .find(set => !attempted.has(set) && !existsSync(join(opts.dir, set.replace(/\.jsonl$/, '.done'))))
  log('start')
  for (let set = nextSet(); set; set = nextSet()) {
    attempted.add(set)
    const name = set.replace(/\.jsonl$/, '')
    const results = join(opts.dir, `${name}.results.jsonl`)
    const tasks = readLines(join(opts.dir, set))
    let waitedSecs = 0
    let otherFailures = 0
    for (;;) {
      const todo = remaining(tasks, readLines(results).map(line => JSON.parse(line)))
      if (!todo.length) break
      log(`${name}: running ${todo.length} of ${tasks.length} tasks`)
      const run = await runOnce(opts, todo, (outcome, line) => {
        appendFileSync(results, line + '\n')
        log(`${name}: ${outcome.id} ${outcome.arm} accepted=${outcome.accepted}`)
      })
      if (run.code === 0) break
      const stoppedAt = run.summary && run.summary.stopped_at
      if (stoppedAt) {
        if (waitedSecs / 3600 >= opts.maxWaitHours) {
          log(`${name}: still limited after ${opts.maxWaitHours}h of waiting; giving up`)
          return
        }
        log(`${name}: provider limit at ${stoppedAt}; waiting ${opts.retrySecs}s`)
        await sleep(opts.retrySecs)
        waitedSecs += opts.retrySecs
        continue
      }
      otherFailures += 1
      log(`${name}: failed (exit ${run.code}): ${run.stderr}`)
      if (otherFailures > MAX_OTHER_FAILURES) {
        log(`${name}: skipped after ${otherFailures} failures`)
        break
      }
    }
    const outcomes = readLines(results).map(line => JSON.parse(line))
    const summary = summarize(outcomes)
    writeFileSync(join(opts.dir, `${name}.summary.json`), JSON.stringify(summary, null, 1) + '\n')
    if (!remaining(tasks, outcomes).length) writeFileSync(join(opts.dir, `${name}.done`), '')
    log(`${name}: summary ${JSON.stringify(summary)}`)
  }
  log('finished')
}

function selfTest() {
  const assert = (condition, message) => {
    if (!condition) throw new Error(`self-test failed: ${message}`)
  }
  const tasks = ['{"id":"t-1"}', '{"id":"t-2"}', '{"id":"t-3"}']
  const outcome = (id, arm, accepted, cost) => ({ id, arm, accepted, cost_usd: cost, judge_cost_usd: 0.1 })
  const done = [outcome('t-1', 'baseline', true, 2), outcome('t-1', 'specialist', true, 1)]
  assert(remaining(tasks, done).length === 2, 'a finished task is not run again')
  const tie = summarize(done)
  assert(tie.specialist_beats_baseline === true, 'same acceptance, cheaper specialist wins')
  const worse = summarize([...done, outcome('t-2', 'baseline', true, 1), outcome('t-2', 'specialist', false, 1)])
  assert(worse.specialist_beats_baseline === false, 'lower acceptance loses')
  const unknown = summarize([outcome('t-1', 'baseline', true, null), outcome('t-1', 'specialist', true, 1)])
  assert(unknown.arms[0].cost_per_accepted_change === null, 'an unknown cost is not summed')
  assert(unknown.specialist_beats_baseline === false, 'a tie with an unknown cost does not win')
  console.log('self-test ok')
}

main().catch(error => {
  console.error(error.message)
  process.exit(1)
})
