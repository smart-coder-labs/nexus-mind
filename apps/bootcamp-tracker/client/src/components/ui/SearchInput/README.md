# SearchInput

A searchable input component with dropdown results, built with compound components pattern.

## Installation

```bash
# The component is copied to your project with de add cli command
# Import path depends on your project's configuration
import { SearchInput } from '@/components/ui/SearchInput';
# or
import { SearchInput } from '../components/ui';
```

## Basic Usage

```tsx
import { useState } from "react";
import { SearchInput } from "@/components/ui/SearchInput";

function Example() {
  const [query, setQuery] = useState("");

  return (
    <SearchInput
      value={query}
      onChange={setQuery}
      placeholder="Search..."
      onSearch={(value) => console.log("Search:", value)}
    />
  );
}
```

## With Debounce

```tsx
<SearchInput
  value={query}
  onChange={setQuery}
  debounceTime={300}
  onSearch={(value) => console.log("Search:", value)}
/>
```

## With Dropdown Results (Compound Components)

```tsx
<SearchInput value={query} onChange={setQuery} isLoading={isLoading}>
  <SearchInput.Dropdown
    show={showResults}
    hasResults={hasResults}
    query={query}
  >
    {subtopics.length > 0 && (
      <SearchInput.Section title="Subtopics">
        {subtopics.map((s) => (
          <SearchInput.Item key={s.id} onClick={() => handleSelect(s)}>
            <SearchInput.ItemIcon type={s.iconType} />
            <SearchInput.ItemContent
              label={s.label}
              subtitle={`${s.topic} · ${s.section}`}
            />
            <SearchInput.TrailingBadge variant="warning">
              {s.priority}
            </SearchInput.TrailingBadge>
          </SearchInput.Item>
        ))}
      </SearchInput.Section>
    )}

    {resources.length > 0 && (
      <SearchInput.Section title="Resources">
        {resources.map((r) => (
          <SearchInput.Item key={r.id}>
            <SearchInput.ItemIcon type={r.type} />
            <SearchInput.ItemContent
              label={r.label}
              subtitle={`${r.topic} · ${r.section}`}
            />
          </SearchInput.Item>
        ))}
      </SearchInput.Section>
    )}
  </SearchInput.Dropdown>
</SearchInput>
```

## API

### Props

| Prop           | Type                      | Default       | Description                         |
| -------------- | ------------------------- | ------------- | ----------------------------------- |
| `value`        | `string`                  | -             | Input value (controlled)            |
| `onChange`     | `(value: string) => void` | -             | Called when input changes           |
| `onSearch`     | `(value: string) => void` | -             | Called on Enter or debounce         |
| `onClear`      | `() => void`              | -             | Called when clear button is clicked |
| `isLoading`    | `boolean`                 | `false`       | Shows spinner on right side         |
| `debounceTime` | `number`                  | `0`           | Debounce delay for onSearch (ms)    |
| `size`         | `'sm' \| 'md' \| 'lg'`    | `'md'`        | Input size                          |
| `variant`      | `'default' \| 'error'`    | `'default'`   | Visual variant                      |
| `placeholder`  | `string`                  | `'Search...'` | Placeholder text                    |
| `disabled`     | `boolean`                 | `false`       | Disables the input                  |
| `label`        | `string`                  | -             | Label text displayed above input    |

## Compound Components

### SearchInput.Input

Custom input with icons (used inside compound pattern).

### SearchInput.Dropdown

Dropdown container for search results.

| Prop         | Type      | Description                           |
| ------------ | --------- | ------------------------------------- |
| `show`       | `boolean` | Controls visibility                   |
| `hasResults` | `boolean` | Shows "No results" message when false |
| `query`      | `string`  | Query string for "No results" message |

### SearchInput.Section

Section header with title.

| Prop       | Type        | Description     |
| ---------- | ----------- | --------------- |
| `title`    | `string`    | Section title   |
| `children` | `ReactNode` | Section content |

### SearchInput.Item

Clickable or static item row.

| Prop        | Type         | Description                       |
| ----------- | ------------ | --------------------------------- |
| `onClick`   | `() => void` | Click handler (makes it a button) |
| `children`  | `ReactNode`  | Item content                      |
| `className` | `string`     | Additional classes                |

### SearchInput.ItemContent

Label and subtitle content.

| Prop        | Type     | Description        |
| ----------- | -------- | ------------------ |
| `label`     | `string` | Main label text    |
| `subtitle`  | `string` | Secondary text     |
| `className` | `string` | Additional classes |

### SearchInput.ItemIcon

Icon for resources/items.

| Prop        | Type                                         | Description                  |
| ----------- | -------------------------------------------- | ---------------------------- |
| `type`      | `'paper' \| 'book' \| 'course' \| 'website'` | Predefined icon              |
| `icon`      | `ReactNode`                                  | Custom icon (overrides type) |
| `className` | `string`                                     | Additional classes           |

### SearchInput.TrailingBadge

Badge displayed on the right side of an item.

| Prop        | Type                                                         | Default     | Description        |
| ----------- | ------------------------------------------------------------ | ----------- | ------------------ |
| `children`  | `ReactNode`                                                  | -           | Badge content      |
| `variant`   | `'default' \| 'primary' \| 'success' \| 'warning' \| 'error` | `'default'` | Visual style       |
| `className` | `string`                                                     | -           | Additional classes |

## Examples

### With Label

```tsx
<SearchInput value={query} onChange={setQuery} label="Search topics" />
```

### Sizes

```tsx
// Small
<SearchInput value={q} onChange={set} size="sm" />

// Medium (default)
<SearchInput value={q} onChange={set} size="md" />

// Large
<SearchInput value={q} onChange={set} size="lg" />
```

### Error State

```tsx
<SearchInput
  value={query}
  onChange={setQuery}
  variant="error"
  placeholder="Search (invalid)"
/>
```

## Notes

- The component uses `forwardRef` for the root element
- Fully accessible with ARIA labels
- Supports dark mode via CSS tokens
- Animations use Framer Motion (150ms transitions)
- Uses `class-variance-authority` for variants

---

## Architecture Decisions

### Why compound components in one file?

This component follows the **Compound Components Pattern** with all subcomponents in the same file. This decision is based on:

1. **Shared Context**: All subcomponents (`Input`, `Dropdown`, `Item`, etc.) depend on `SearchInputContext`. Keeping them together makes the relationship clear.

2. **Proximity**: The subcomponents are tightly coupled - `SearchInput.Item` needs access to context to know if the parent is disabled. Splitting them would require more complex setup.

3. **File size**: At ~300 lines, the file is readable and maintainable. According to project architecture rules, only split when exceeding ~400 lines or when a subcomponent has independent complex state.

### When to split?

If this component grows significantly, consider extracting:

- **SearchInput.Item**: If it needs independent state (virtualization, complex keyboard navigation)
- **SearchInput.Dropdown**: If it needs external libraries (portal, positioning lib)
- **SearchInput.Input**: If it gets reused outside SearchInput context

### File Structure

```
SearchInput/
├── SearchInput.tsx       # Main + all compound components
├── SearchInput.types.ts  # Types + cva variants
└── index.ts             # Barrel exports
```

This follows the "keep together until necessary" principle from the project architecture guidelines.
