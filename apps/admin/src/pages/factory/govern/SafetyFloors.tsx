import { ShieldAlert } from 'lucide-react'

/**
 * Deterministic floors that always send an action to a person, whatever a
 * policy says. Mirrors docs/factory/PLAN.md §4 (step 2) and the merge risk
 * floor in apps/backend/src/automation/merge_gate.rs (owner rubric v1).
 * Update both together.
 */
const EVERY_ACTION = [
  'Authentication, authorization, secrets or crypto',
  'Payments and billing',
  'Database migrations and schema changes',
  'Dependency, lockfile or build manifest changes',
  'CI, infrastructure or infrastructure-as-code',
  'Any required check failing',
  'A second failed repair attempt',
  'A fix requested from Gmail or a transcript',
]

const MERGES_ALSO = ['Webhooks, OAuth and other external providers', 'Agent instructions (skills, prompts, AGENTS.md)']

export function SafetyFloors() {
  return (
    <section
      aria-labelledby="floors-title"
      className="min-w-0 rounded-xl border border-border-primary bg-card p-4 sm:p-5"
    >
      <div className="flex flex-wrap items-start gap-3">
        <ShieldAlert className="mt-0.5 h-5 w-5 shrink-0 text-text-secondary" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <h2 id="floors-title" className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Always a person</h2>
          <p className="mt-1 text-sm text-text-secondary">
            These safety floors hold an action for a person whatever a policy says. No policy can lift them.
          </p>
          <ul className="mt-3 grid list-none gap-x-6 gap-y-1.5 p-0 text-sm text-text-primary sm:grid-cols-2 xl:grid-cols-3">
            {EVERY_ACTION.map(item => (
              <li key={item} className="flex gap-2">
                <span aria-hidden="true" className="mt-[7px] h-1 w-1 shrink-0 rounded-full bg-text-tertiary" />
                {item}
              </li>
            ))}
          </ul>
          <p className="mt-3 text-[12px] text-text-tertiary">
            Merges are also held when they touch {MERGES_ALSO.map(item => item.charAt(0).toLowerCase() + item.slice(1)).join(', or ')}.
          </p>
        </div>
      </div>
    </section>
  )
}
