import { copyFileSync, existsSync, readFileSync, readdirSync, renameSync, unlinkSync } from 'node:fs'
import { join } from 'node:path'

const dir = '/data/factory-evals'
const runId = process.env.NM_EVAL_RUN_ID ?? ''
const requested = (process.env.NM_EVAL_SET_IDS ?? '').split(',').filter(Boolean)
const allowed = new Set(['docs', 'tests', 'ui'])

if (!/^\d+$/.test(runId)) throw new Error('invalid workflow run id')
if (!requested.length || requested.some(id => !allowed.has(id)) || new Set(requested).size !== requested.length) {
  throw new Error('eval set selection must contain unique docs, tests and/or ui ids')
}

const files = readdirSync(dir)
const plans = requested.map(id => {
  const sources = files
    .map(name => ({ name, match: name.match(new RegExp(`^${id}-v(\\d+)\\.jsonl$`)) }))
    .filter(file => file.match)
    .sort((a, b) => Number(a.match[1]) - Number(b.match[1]))
  const source = sources.at(-1)?.name
  if (!source) throw new Error(`no frozen ${id}-vN.jsonl input exists`)
  const sourcePath = join(dir, source)
  const taskLines = readFileSync(sourcePath, 'utf8').split(/\r?\n/).filter(Boolean)
  if (!taskLines.length) throw new Error(`frozen input is empty: ${source}`)
  for (const line of taskLines) JSON.parse(line)
  const name = `${id}-rerun-${runId}.jsonl`
  const target = join(dir, name)
  const temporary = join(dir, `.${name}.tmp`)
  if (existsSync(target) || existsSync(`${target}.done`) || existsSync(temporary)) {
    throw new Error(`rerun target already exists: ${name}`)
  }
  return { id, source: sourcePath, target, temporary, name, taskCount: taskLines.length }
})

const staged = []
try {
  for (const plan of plans) {
    copyFileSync(plan.source, plan.temporary)
    staged.push(plan)
  }
  for (const plan of staged) renameSync(plan.temporary, plan.target)
} catch (error) {
  for (const plan of staged) {
    try { unlinkSync(plan.temporary) } catch {}
    try { unlinkSync(plan.target) } catch {}
  }
  throw error
}

for (const plan of plans) {
  console.log(`queued ${plan.name} from ${plan.source.split('/').at(-1)} (${plan.taskCount} tasks)`)
}
