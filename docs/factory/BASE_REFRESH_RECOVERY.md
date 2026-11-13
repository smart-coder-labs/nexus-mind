# Fresh-base publication recovery

`base_moved_reverify` is a valid publication hold, not a source-code failure.
Do not bypass the base check or reuse passing evidence for a different base.

When the first `policy-publisher` attempt reports that hold, and no repository
publication has started, recovery can atomically retire the old run and queue
another run of the **same definition and task** with fresh sources. It happens
before optional decision-model recovery, so no model is required. `resume` also
handles already-held runs through the same guard.

- Current `recover` policy must allow it; the new run separately requires `fix`
  authorization. Old approvals are not copied.
- The replacement keeps only the predecessor's remaining dollar budget, further
  capped by the definition budget. It gets no old passing evidence or candidate
  heads. Preparation must fetch and fence the current branch before any check.
- At most **two** replacements per lineage. This is not a code repair and does
  not increment its repair counter.
- Partial publications or prior publication attempts remain held: do not abandon
  possible external writes or duplicate PRs automatically.
- Creation, predecessor cancellation and lineage events are one transaction.
  Failed validation leaves the predecessor and run history intact.
- `run.base_refreshed` links to the replacement; `run.restarted_from` records its
  origin, generation and remaining budget. The UI links the retired run to it.

Functional/design/security findings remain advisory with their real failed
artifacts. Fresh-source build/tests, the gate, independent review and action
policies still apply. Merge/deployment authorization is unchanged.

Check: `cargo test --manifest-path apps/backend/Cargo.toml --test factory_workflows moved_bases_restart_atomically`
