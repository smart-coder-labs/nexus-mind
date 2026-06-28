# Skill: deploy-check

Run pre-deployment checks to ensure the project is ready for deployment.

## Instructions

When the user invokes this skill, run all quality checks in sequence:

### Steps:

1. **Git Status Check**
   - Show current branch and uncommitted changes
   - Warn if there are untracked files

2. **Lint Check** (`npm run lint`)
   - Run ESLint and report any errors/warnings
   - Offer to auto-fix with `npm run lint:fix` if issues found

3. **TypeScript Check** (`npx tsc --noEmit`)
   - Verify there are no type errors
   - Report any type issues with file locations

4. **Build Check** (`npm run build`)
   - Attempt a production build
   - Report any build errors

5. **Test Check** (`npm run test -- --passWithNoTests`)
   - Run the test suite
   - Report test results

### Output Format:
Provide a summary table:

| Check | Status | Details |
|-------|--------|---------|
| Git Status | PASS/FAIL | ... |
| Lint | PASS/FAIL | ... |
| TypeScript | PASS/FAIL | ... |
| Build | PASS/FAIL | ... |
| Tests | PASS/FAIL | ... |

If all checks pass: "Ready to deploy!"
If any check fails: List what needs to be fixed before deploying.

## Arguments
$ARGUMENTS is unused. Simply invoke `/deploy-check` to run all checks.
