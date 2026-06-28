# Skill: review-code

Review code for quality, patterns, and Eliox conventions.

## Instructions

When the user invokes this skill, review the specified file(s) or the currently open file.

### Review Checklist:

1. **TypeScript Quality**
   - Proper typing (no `any` types)
   - Interfaces defined for props and data structures
   - Correct use of generics

2. **React Patterns**
   - Named exports (not default exports)
   - React.FC typing for components
   - Proper use of hooks (dependencies, cleanup)
   - No unnecessary re-renders

3. **Tailwind CSS**
   - Using Tailwind utilities instead of custom CSS
   - Consistent use of design tokens (eliox-color-* variables)
   - Responsive design classes where needed

4. **Code Quality**
   - No unused imports or variables
   - No console.log statements (except in dev utilities)
   - Proper error handling
   - Clean and readable code structure

5. **Security**
   - No hardcoded secrets or API keys
   - Proper input validation
   - XSS prevention in JSX

### Output Format:
Provide a structured review with:
- **Issues found** (with severity: critical/warning/suggestion)
- **What's good** (positive feedback)
- **Suggested fixes** (with code examples)

## Arguments
$ARGUMENTS contains the file path(s) to review.
If no arguments, review the currently open/selected file.
Example: `/review-code src/new-components/Avatar/index.tsx`
