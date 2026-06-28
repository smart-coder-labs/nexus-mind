# Skill: migrate-to-shadcn

Migrate an existing Eliox custom component to use shadcn/ui as its base, preserving business logic and Eliox design tokens.

## Instructions

When the user invokes `/migrate-to-shadcn ComponentName`, follow this process:

### Phase 1: Analysis

1. **Read the target component** in `src/new-components/{ComponentName}/index.tsx`
2. **Identify the shadcn/ui equivalent** from this mapping:

    | Custom Component    | shadcn Component(s)       | Install Command                                  |
    | ------------------- | ------------------------- | ------------------------------------------------ |
    | Avatar              | Avatar                    | `npx shadcn@latest add avatar`                   |
    | UserLetter          | Avatar (fallback)         | `npx shadcn@latest add avatar`                   |
    | Badge               | Badge                     | `npx shadcn@latest add badge`                    |
    | Breadcrumb          | Breadcrumb                | `npx shadcn@latest add breadcrumb`               |
    | Collapse            | Collapsible               | `npx shadcn@latest add collapsible`              |
    | InputCheckbox       | Checkbox                  | `npx shadcn@latest add checkbox`                 |
    | InputSwich          | Switch                    | `npx shadcn@latest add switch`                   |
    | Separator           | Separator                 | `npx shadcn@latest add separator`                |
    | Popover             | Popover                   | `npx shadcn@latest add popover`                  |
    | InputSelect         | Select or Command+Popover | `npx shadcn@latest add select`                   |
    | InputSelectMultiple | Command+Popover+Checkbox  | `npx shadcn@latest add command popover checkbox` |
    | Tab                 | Tabs                      | `npx shadcn@latest add tabs`                     |
    | Table               | Table                     | `npx shadcn@latest add table`                    |
    | Modal/ModalNew      | Dialog                    | `npx shadcn@latest add dialog`                   |
    | NewPagination       | Pagination                | `npx shadcn@latest add pagination`               |
    | MapAddressListPanel | Sheet                     | `npx shadcn@latest add sheet`                    |
    | InputText           | Input+Label               | `npx shadcn@latest add input label`              |
    | InputPassword       | Input (type=password)     | `npx shadcn@latest add input`                    |
    | InputSearch         | Input                     | `npx shadcn@latest add input`                    |
    | InputMoney          | Input                     | `npx shadcn@latest add input`                    |
    | InputDate           | Input (type=date)         | `npx shadcn@latest add input`                    |
    | TextArea            | Textarea                  | `npx shadcn@latest add textarea`                 |
    | GlassDropDown       | DropdownMenu              | `npx shadcn@latest add dropdown-menu`            |

3. **Check if the shadcn base is already installed** in `src/components/ui/`
4. **Catalog custom business logic** that must be preserved (validation, permissions, animations, callbacks)
5. **Identify all consumer files** that import the component (use grep for import paths)

### Phase 2: Installation

1. **Install the shadcn component** if not already present:
    ```bash
    npx shadcn@latest add {component-name}
    ```
2. **Verify installation** in `src/components/ui/{component-name}.tsx`
3. **Ensure wrapper exists in eliox folder** — Check `src/components/eliox/{component-name}.tsx`, if missing create a re-export:
    ```tsx
    export * from "@/components/ui/{component-name}";
    ```
4. The shadcn component installs as a **primitive** — do NOT modify it directly

### Phase 3: Migration Strategy

Use the **wrapper pattern**: the custom component in `src/new-components/` wraps the shadcn primitive from `src/components/ui/`, preserving the existing API.

#### Rules:

1. **Keep the same export name and props interface** — consumers should not need changes
2. **Import shadcn primitives from Eliox wrappers** — ALWAYS use `@/components/eliox` instead of `@/components/ui`
    - WRONG: `import { Button } from "@/components/ui/button"`
    - RIGHT: `import { Button } from "@/components/eliox"`
    - Reason: Eliox wrappers have custom variants like `complementary` and ensure consistent branding
3. **Extract business logic into hooks** when it exceeds 20 lines:
    - Validation logic → `use{Component}Validation.ts`
    - State management → `use{Component}State.ts`
    - Place hooks in the same component folder
4. **Use `cn()` utility** from `utils` for class merging (already available)
5. **Never use hardcoded hex colors** — always use Eliox semantic tokens

#### Color Token Rules:

```
WRONG: text-[#2C3E50]    bg-[#1abc9c]    border-[#ccc]
WRONG: text-eliox-text-primary    bg-eliox-secondary    border-eliox-border   ← DEAD TOKENS (issue #416)
RIGHT: text-foreground    bg-background    border-border
```

**ONLY use shadcn semantic tokens** (defined in `globals.css` `:root`/`.dark` blocks — they adapt automatically):

- **Text**: `text-foreground`, `text-muted-foreground`, `text-primary-foreground`
- **Background**: `bg-background`, `bg-card`, `bg-muted`, `bg-primary`, `bg-secondary`, `bg-accent`, `bg-popover`
- **Border**: `border-border`, `border-input`
- **Status**: `text-destructive`, `bg-destructive`
- **Ring/Focus**: `ring-ring`

**NEVER use** `text-eliox-text-*`, `bg-eliox-*`, `border-eliox-*`, or `var(--eliox-color-*)` — those CSS vars are undefined (eliox-ui styles are not imported) and render as transparent/invisible.

#### Dark Mode:

- Eliox uses class-based dark mode: `body.dark`
- Tailwind v4 custom variant: `dark:` prefix works via `@custom-variant dark (&:where(.dark, .dark *))`
- Eliox CSS variables (`--eliox-color-*`) automatically switch in dark mode — no need for `dark:` prefix on eliox tokens
- Shadcn HSL variables also switch automatically via `:root` / `.dark` in `globals.css`

### Phase 4: Implementation

#### Template for wrapper component:

```tsx
import React from "react";
import { cn } from "utils";
// Import shadcn primitive
import { ShadcnComponent } from "@/components/ui/shadcn-component";

// Keep the SAME interface name
interface ComponentNameProps {
    // Preserve all existing props
    existingProp: string;
    // Add shadcn-compatible props if needed
    className?: string;
}

// Keep the SAME export name
export const ComponentName: React.FC<ComponentNameProps> = ({
    existingProp,
    className,
    ...props
}) => {
    // Custom business logic stays here or in a hook
    return (
        <ShadcnComponent
            className={cn(
                // Eliox-specific styles using semantic tokens
                "text-eliox-text-primary",
                className,
            )}
            {...props}
        />
    );
};
```

#### Size variant mapping:

```
Eliox "small"  → shadcn "sm"
Eliox "medium" → shadcn "default"
Eliox "large"  → shadcn "lg"
```

### Phase 5: Validation

1. **TypeScript check**: Run `npx tsc --noEmit` to verify no type errors
2. **Search for broken imports**: Grep for the component name across `src/pages/` and `src/new-components/`
3. **Visual check**: Confirm the component renders correctly (suggest running `npm run dev`)
4. **Test**: Run `npm run test` if tests exist for the component

### Phase 6: Report

After migration, output a summary:

```
## Migration Complete: {ComponentName}

| Item | Status |
|------|--------|
| shadcn base installed | {component-name}.tsx |
| Wrapper updated | src/new-components/{ComponentName}/index.tsx |
| Hooks extracted | {list or "none"} |
| Consumer files affected | {count} files |
| Breaking changes | {list or "none"} |
| TypeScript check | PASS/FAIL |
```

## Components NOT to Migrate

These components should NOT be migrated (too custom or no shadcn equivalent):

- **Button** — 8 variants + glassmorphism + loading (keep custom)
- **CollapseNew** — Status/validation/loader system (keep custom)
- **TextArea** — Complex async validation (keep custom, or migrate only base)
- **GlassCard** — Eliox glassmorphism design (keep custom)
- **Link** — 6 templates + Next.js router (keep custom)
- **Back** — Navigation logic (keep custom)
- **Tag** — Eliox flags integration (keep custom)
- **LoaderDot** — Custom animation, integrated in Button (keep custom)
- **SidebarMenu** — localStorage + permissions + routes (keep custom)
- **PhoneInput** — USA phone formatting (keep custom)
- **InputGoogleAutocomplete** — Google Places API (keep custom)
- **Layout/LayoutSingle/LayoutTable** — Layout patterns, not UI primitives
- **Tree/TreeNode** — SVG hierarchy rendering (no equivalent)
- **Counters/CounterList/PieChart** — Data visualization (no equivalent)
- **RoleGuard** — Business logic, not UI
- **logo/SVG** — Brand assets and icons
- **Map components** — Google Maps integration (no equivalent)

If the user requests migrating one of these, warn them about the risk and suggest keeping it custom while optionally using shadcn sub-components internally.

## Batch Migration

To install all recommended shadcn components at once:

```bash
npx shadcn@latest add avatar badge breadcrumb checkbox collapsible separator switch popover select tabs table pagination dialog sheet command input label textarea dropdown-menu
```

## Arguments

$ARGUMENTS contains the component name to migrate.
Examples:

- `/migrate-to-shadcn Avatar`
- `/migrate-to-shadcn InputCheckbox`
- `/migrate-to-shadcn Tab`
- `/migrate-to-shadcn batch` (installs all shadcn primitives at once)
