# Delta spec — autonomous-merge-gate

## ADDED Requirements

### Requirement: Auto-merge is bound to the reviewed commit

The PR reviewer agent MUST merge only the exact commit it reviewed.

#### Scenario: Head moved after review
- **Given** a review ran on commit `A`
- **When** the PR head is `B` at merge time
- **Then** the merge is declined with reason `head_changed_since_review`
- **And** no merge API call is made

#### Scenario: Reviewed commit unknown
- **Given** a run with no `trigger.head_sha`
- **When** auto-merge is evaluated
- **Then** the merge is declined with reason `reviewed_head_unknown`

#### Scenario: Race between check and merge
- **Given** the head equals the reviewed commit when checked
- **When** the merge API is called
- **Then** the request pins `sha` to the reviewed commit, so GitHub rejects the merge if the head moved in between

### Requirement: Auto-merge is limited to low-risk paths

Until the per-action policy engine lands, auto-merge MUST only merge PRs whose every changed path is documentation or tests.

#### Scenario: Docs/tests-only PR
- **Given** all changed files match the docs/tests allowlist
- **And** they have status `added` or `modified`
- **When** checks are green and the review has no blocking findings
- **Then** the PR may be merged

#### Scenario: Any path outside the allowlist
- **Given** at least one changed file is outside the allowlist (e.g. `src/auth.rs`, `.github/workflows/ci.yml`, `Cargo.lock`)
- **Then** the merge is declined with reason `path_not_eligible` and the offending path is reported

#### Scenario: Deleted or renamed-out tests
- **Given** a changed file has status `removed`, or is a rename whose previous path is outside the allowlist
- **Then** the merge is declined with reason `path_not_eligible`

#### Scenario: Unknown or oversized file list
- **Given** the file list cannot be read, or exceeds the cap
- **Then** the merge is declined (fail closed)
