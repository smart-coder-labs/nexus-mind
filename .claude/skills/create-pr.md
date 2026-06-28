# Skill: create-pr

Create a pull request from the current branch following Eliox project conventions.

## Instructions

When the user invokes this skill, push the current branch and open a PR against `develop`.

### Rules

- All content (title, body) must be written in **English**
- Title follows conventional commits: `feat(scope): description`
- Always target `develop` as the base branch (never `main` directly)
- Always close the related issue with `Closes #N` in the body
- Push the branch with `-u` before creating the PR

### PR body format

```markdown
Closes #N

## Summary
- Bullet 1
- Bullet 2

## Test plan
- [ ] `npm test -- --testPathPatterns="FileName"` → N/N passing
- [ ] Manual check: [describe what to verify manually if needed]
```

### Steps

1. Parse `$ARGUMENTS` for the issue number and optional title override
2. Run `npm run build` — **if it fails, stop and report the errors. Do not create the PR.**
3. Run `git push -u origin <current-branch>`
4. Create the PR via `gh pr create --base develop`
5. Output the PR URL

### Branch naming convention

Branches must follow: `feat/{issue-number}-{kebab-description}`
Example: `feat/324-use-disclosure-hook`

## Arguments

`$ARGUMENTS` contains the related issue number and optional title.
Example: `/create-pr 324` or `/create-pr 324 feat(foundation): create useDisclosure hook`
