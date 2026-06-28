# Skill: tanstack-query

Patterns for replacing fenextjs `useQuery` / `useAction` / `useData` with `@tanstack/react-query` v5.

## When to Use

- Migrating any hook that uses fenextjs `useQuery`, `useAction`, or event-bus patterns
- Any modal or page hook that fetches data (`GET`) or mutates data (`POST/PUT/DELETE`)
- Replacing `useAction` event bus with cache invalidation

---

## 1. Query Key Factory (always use this pattern)

```ts
// src/lib/query-keys/index.ts
export const queryKeys = {
  condominiums: {
    all: () => ['condominiums'] as const,
    list: (filters?: object) => ['condominiums', 'list', filters] as const,
    detail: (id: string) => ['condominiums', id] as const,
  },
  users: {
    all: () => ['users'] as const,
    me: () => ['users', 'me'] as const,
    detail: (id: string) => ['users', id] as const,
  },
  contacts: {
    all: () => ['contacts'] as const,
    list: (filters?: object) => ['contacts', 'list', filters] as const,
  },
  // Add entities as needed
}
```

---

## 2. useQuery — Fetching Data

```ts
// BEFORE (fenextjs)
const { data, isLoading, error } = useQuery({
  action: fetchCondominiums,
  id: 'condominiums-list',
})

// AFTER (@tanstack/react-query v5)
import { useQuery } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

const { data, isLoading, error } = useQuery({
  queryKey: queryKeys.condominiums.list(filters),
  queryFn: () => fetchCondominiums(filters),
  staleTime: 5 * 60 * 1000, // 5 min — adjust per use case
})
```

### Conditional / enabled queries

```ts
const { data } = useQuery({
  queryKey: queryKeys.condominiums.detail(id),
  queryFn: () => fetchCondominium(id),
  enabled: !!id,  // only run when id is available
})
```

### Search-on-type (debounced)

```ts
const { data } = useQuery({
  queryKey: ['search', debouncedTerm],
  queryFn: () => searchAdministrators(debouncedTerm),
  enabled: debouncedTerm.length >= 2,
})
```

---

## 3. useMutation — Creating / Updating / Deleting

```ts
// BEFORE (fenextjs)
const { onSubmit } = useData({
  action: createCondominium,
  onActionSuccess: () => FenextFireAction('condominiumCreated'),
})

// AFTER
import { useMutation, useQueryClient } from '@tanstack/react-query'

const queryClient = useQueryClient()

const { mutate, isPending } = useMutation({
  mutationFn: createCondominium,
  onSuccess: () => {
    queryClient.invalidateQueries({ queryKey: queryKeys.condominiums.all() })
    modal.close()
    toast.success('Condominium created')
  },
  onError: (error) => {
    toast.error(error.message ?? 'Error saving')
  },
})

// In submit handler:
form.handleSubmit((data) => mutate(data))()
```

---

## 4. Replacing useAction (event bus) with invalidateQueries

```ts
// BEFORE (fenextjs event bus)
// Publisher:
FenextFireAction('budgetSaved')
// Subscriber:
useAction({ action: 'budgetSaved', onAction: refetchList })

// AFTER (cache invalidation — no event bus needed)
// In the mutation that saves:
onSuccess: () => {
  queryClient.invalidateQueries({ queryKey: queryKeys.budgets.all() })
}
// The list component re-fetches automatically via useQuery
```

---

## 5. Polling / Notifications

```ts
// Replaces: fenextjs useQuery with polling interval
const { data } = useQuery({
  queryKey: queryKeys.notifications.all(),
  queryFn: fetchNotifications,
  refetchInterval: 30_000, // poll every 30s
  refetchIntervalInBackground: false,
})
```

---

## 6. Prefetching (optional, for performance)

```ts
await queryClient.prefetchQuery({
  queryKey: queryKeys.condominiums.detail(id),
  queryFn: () => fetchCondominium(id),
})
```

---

## 7. Full Modal Hook Pattern

Replaces `useData + useModal + useAction` in a single pattern:

```ts
// src/lib/modal/ModalAddContact/hook.tsx
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useDisclosure } from '@/lib/hooks/useDisclosure'
import { useAppForm } from '@/lib/hooks/useAppForm'
import { contactSchema } from './schema'
import { createContact } from '@/lib/api/contacts'
import { queryKeys } from '@/lib/query-keys'
import { toast } from 'sonner'

export function useModalAddContact() {
  const modal = useDisclosure()
  const queryClient = useQueryClient()

  const form = useAppForm({
    schema: contactSchema,
    defaultValues: { name: '', email: '', phone: '' },
  })

  const { mutate, isPending } = useMutation({
    mutationFn: createContact,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.contacts.all() })
      form.reset()
      modal.close()
      toast.success('Contact created')
    },
    onError: (error) => {
      form.setError('root', { message: error.message })
    },
  })

  return {
    modal,
    form,
    isPending,
    onSubmit: form.handleSubmit((data) => mutate(data)),
  }
}
```

---

## 8. Update Modal (fetch existing data first)

```ts
export function useModalUpdateContact(contactId: string) {
  const modal = useDisclosure()
  const queryClient = useQueryClient()

  // Fetch existing data to pre-populate form
  const { data: contact, isLoading } = useQuery({
    queryKey: queryKeys.contacts.detail(contactId),
    queryFn: () => fetchContact(contactId),
    enabled: modal.isOpen && !!contactId,
  })

  const form = useAppForm({
    schema: contactSchema,
    defaultValues: { name: '', email: '', phone: '' },
  })

  // Sync fetched data into form
  useEffect(() => {
    if (contact) form.reset(contact)
  }, [contact, form.reset])

  const { mutate, isPending } = useMutation({
    mutationFn: (data) => updateContact(contactId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.contacts.all() })
      modal.close()
      toast.success('Contact updated')
    },
  })

  return { modal, form, isLoading, isPending, onSubmit: form.handleSubmit((data) => mutate(data)) }
}
```

---

## Critical Rules

- **Always use `queryKeys.*` factory** — never raw strings like `['contacts']`
- **`invalidateQueries` with `.all()`** after mutations that affect lists
- **`enabled: !!id`** for queries that depend on a parameter
- **`staleTime: 5 * 60 * 1000`** as default (5 min) — reduces waterfall re-fetches
- **`toast` from `sonner`** for success/error feedback (NOT fenextjs NotificationPop)
- **`form.setError('root', ...)`** for server errors that should appear in the form
- **Never use `FenextFireAction` or `useAction`** — use `invalidateQueries` instead

## Arguments

`$ARGUMENTS` contains the hook/component to migrate.
Example: `/tanstack-query useCondominiumsList`
