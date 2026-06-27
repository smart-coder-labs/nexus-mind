# Skill: create-issue

Create a GitHub issue following Eliox project conventions.

## Instructions

When the user invokes this skill, create a GitHub issue using `gh issue create`.

### Rules

- All content (title, body) must be written in **English**
- Use conventional commit scope in the title: `feat(scope): description`
- Add relevant labels (see below)
- Add to milestone if the issue belongs to an ongoing epic

### Label conventions

| Label | When to use |
|-------|-------------|
| `epic:foundation` | Core infrastructure work |
| `fenextjs-removal` | Part of the fenextjs removal epic |
| `priority:high` | Blocking or critical path |
| `priority:medium` | Normal priority |
| `priority:low` | Nice to have |

### Body format

```markdown
## Context
[Why this issue exists. What problem it solves.]

## Replacement / Approach
[What the solution looks like. Code snippets if relevant.]

## Tasks
- [ ] Task 1
- [ ] Task 2
- [ ] Task 3

## Acceptance Criteria
- [ ] Criterion 1
- [ ] Criterion 2
```

### Steps

1. Parse `$ARGUMENTS` for issue title and optional description
2. Determine labels and milestone from context
3. Create the issue via `gh issue create`
4. Output the issue URL

## Arguments

`$ARGUMENTS` contains the issue title and optional context.
Example: `/create-issue feat(foundation): create useDisclosure hook to replace fenextjs useModal`
