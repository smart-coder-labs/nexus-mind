# Skill: review-pr-comments

Review automated or peer PR comments, assess their validity, and reply inline.

## Instructions

When the user invokes this skill, fetch all inline comments on the given PR, evaluate each one, and post replies.

### Rules

- All replies must be written in **English**
- Always verify claims against the actual code before agreeing or dismissing
- Never agree with a comment without checking the relevant file/config/test
- Prefix each reply with an emoji that signals the verdict (see table below)

### Verdict emoji

| Emoji | Meaning |
|-------|---------|
| ✅ | Valid — confirmed against the code |
| 🚫 | Not applicable — explain why in context |
| ⚠️ | Valid concern but low priority — acknowledge and defer |
| 💡 | Suggestion worth considering — no action required now |

### Evaluation checklist

For each comment, verify:
1. **Indentation / formatting** → check `.editorconfig` and run `npm run format` mentally
2. **Missing behavior** → check if it's tested, intentional, or out of scope
3. **Architecture concerns** → check if the suggestion belongs in this layer or another
4. **Type safety** → verify against TypeScript strict mode and existing patterns

### Steps

1. Parse `$ARGUMENTS` for the PR number
2. Fetch inline comments via `gh api repos/{owner}/{repo}/pulls/{pr}/comments`
3. For each comment: read the referenced file and lines, evaluate validity
4. Post a reply via `gh api .../comments/{id}/replies -X POST -f body="..."`
5. Output a summary table of all comments and their verdicts

## Arguments

`$ARGUMENTS` contains the PR number.
Example: `/review-pr-comments 379`
