# Skill: generate-component

Generate a new React component following Eliox project conventions.

## Instructions

When the user invokes this skill, generate a new component in `src/new-components/`.

### Eliox Component Conventions:

1. **TypeScript** - Always use `.tsx` files with explicit interfaces for props
2. **Tailwind CSS** - Use Tailwind utility classes (no SCSS, no inline styles)
3. **Named exports** - Use `export const ComponentName` (never `export default`)
4. **React.FC** - Use `React.FC<Props>` typing
5. **Folder structure** - Each component lives in its own folder: `src/new-components/ComponentName/index.tsx`
6. **Optional props** - Use `?` for optional props with default values via destructuring
7. **CSS variables** - Use semantic shadcn tokens: `bg-background`, `text-foreground`, `text-muted-foreground`, `bg-card`, `border-border`, `bg-primary`, etc. NEVER use `var(--eliox-color-*)` — those vars are undefined (eliox-ui CSS is not imported)
8. **Size variants** - Follow the pattern: `"small" | "medium" | "large"` for size props
9. **Always use Eliox wrappers for shadcn components** - When using shadcn/ui primitives (Button, Badge, Input, Card, Dialog, etc.), import from `@/components/eliox` instead of `@/components/ui`

### Steps:

1. Ask the user for: component name, description, and props (if not provided)
2. Check if the component already exists in `src/new-components/`
3. Look at a similar existing component for reference (use `get_component_template` MCP tool if available)
4. Create the component file at `src/new-components/{ComponentName}/index.tsx`
5. Show the generated code to the user

### Template Pattern:

```tsx
import React from "react";

interface ComponentNameProps {
    propName: string;
    optionalProp?: boolean;
}

export const ComponentName: React.FC<ComponentNameProps> = ({
    propName,
    optionalProp = false,
}) => {
    return (
        <div className="tailwind-classes-here">{/* Component content */}</div>
    );
};
```

## Arguments

$ARGUMENTS contains the component name and optional description.
Example: `/generate-component UserCard - displays user info with avatar`
