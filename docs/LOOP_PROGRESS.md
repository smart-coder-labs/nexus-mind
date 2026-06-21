# NexusMind Loop Progress

Registro continuo de todos los cambios realizados por el loop de mejora continua.  
**Restricción**: No se sube nada a GitHub hasta que el usuario lo indique.  
**Migración actual**: v26 (en progreso)

---

## Cómo probar

### Iniciar el entorno local

```bash
# Backend
cd apps/backend && cargo run

# Admin frontend (dev server)
cd apps/admin && npm run dev

# Seed de demo (primera vez o para resetear)
./scripts/reset-demo.sh
```

### Credenciales de demo

| Usuario | API Key |
|---------|---------|
| Acme Corp — admin | `nm_demo_acme_admin` |
| Acme Corp — Sarah Chen | `nm_demo_acme_sarah` |
| TechStartup — admin | `nm_demo_techstartup_admin` |

### Correr tests backend

```bash
cd apps/backend && cargo test
```

### Build frontend

```bash
cd apps/admin && npm run build
```

---

## Estado general

| Ciclo | Tipo | Descripción | Estado |
|-------|------|-------------|--------|
| 1–23 | Feature | Ver historial pre-compaction (fundamentals) | ✅ |
| 24 | UI/UX | Layout notification bell | ✅ |
| 25 | UI/UX | User activity drawer + bulk tag bar | ✅ |
| 26 | UI/UX | Code saved queries + Projects stats + Settings export | ✅ |
| 27 | UI/UX | Collections tab + audit log grouping | ✅ |
| 28 | UI/UX | Dashboard heatmap card | ✅ |
| 29 | UI/UX | Settings memory templates + ApiKeys sweep | ✅ |
| 30 | UI/UX | Tag rename UI + webhook retry + Roles page | ✅ |
| 31 | UI/UX | Dashboard contributors + Code.tsx audit | ✅ |
| 32 | Feature | Bulk memory tag editing | ✅ |
| 33 | Feature | User activity drawer + memory edit history | ✅ |
| 34 | Feature | Per-project memory stats + org settings export | ✅ |
| 35 | Feature | Code search saved queries + (role change ya existía) | ✅ |
| 36 | Feature | Memory collections (v25) + audit log session grouping | ✅ |
| 37 | Feature | Dashboard memory heatmap | ✅ |
| 38 | Feature | Memory templates (localStorage) | ✅ |
| 39 | Feature | Global tag rename + webhook retry | ✅ |
| 40 | Feature | Top contributors dashboard + search highlighting | ✅ |
| 41 | Feature | Code sync status + retention preview (v26) | ✅ |
| 42 | Feature | Manual reindex trigger + (API key expiry pendiente) | ✅ |
| 32 | UI/UX | Code sync badges + Settings retention preview | ✅ |
| 43 | Feature | Audit log full-text search + dashboard quick actions | ✅ |
| 33 | UI/UX | Code reindex button + AuditLog + Login/SetPassword | ✅ |
| 44 | Feature | Tag autocomplete + code search result highlighting | ✅ |
| 34 | UI/UX | AuditLog search input + Dashboard quick actions | ✅ |
| 45 | Feature | Code search file extension filter + API key expiry (v27) | ✅ |
| 35 | UI/UX | TagAutocomplete + Code search highlight styles | ✅ |
| 46 | Feature | User account disable/enable (v28) + memory version history | ✅ |
| 36 | UI/UX | Extension filter chip + ApiKeys expiry + Projects accordion | ✅ |
| 47 | Feature | Dashboard time period selector + keyboard shortcuts panel | ✅ |
| 37 | UI/UX | Users disable buttons + Memories HistoryPanel | ✅ |
| 48 | Feature | Memory admin notes (v29) + project member bulk add | ✅ |
| 38 | UI/UX | Dashboard period toggle + shortcuts panel | ✅ |
| 49 | Feature | Admin announcement banner (v30) + memory scheduled deletion | ✅ |
| 39 | UI/UX | Memory notes UI + Projects bulk add UI | ✅ |
| 50 | Feature | Dashboard card visibility toggle + (cmd palette ya existía) | ✅ |
| 40 | UI/UX | Announcement banner + scheduled deletion chip + Settings section | ✅ |
| 51 | Feature | Settings config import + memory word count | ✅ |
| 41 | UI/UX | Dashboard customize dropdown + cmd palette search sections | ✅ |
| 52 | Feature | Command palette search history + code project archiving (v31) | ✅ |
| 42 | UI/UX | Settings import/export buttons + Memories word count | ✅ |
| 53 | Feature | Notification preferences + memory export by collection | ✅ |
| 43 | UI/UX | CommandPalette history section + Code archive badges | ✅ |
| 54 | Feature | User admin notes (v32) + API key usage counter | ✅ |
| 44 | UI/UX | Layout notification prefs + Memories collection export | ✅ |
| 55 | Feature | Code search export + user last login tracking (v33) | ✅ |
| 45 | UI/UX | Users note section + ApiKeys usage counter | ✅ |
| 56 | Feature | Code project file exclusions (v34) | ✅ |
| 46 | UI/UX | Code search export buttons + Users last login | ✅ |
| 57 | Feature | Org logo/branding (v35) + memory favorites (localStorage) | ✅ |
| 47 | UI/UX | Code ExcludePatternsEditor + pattern pills | ✅ |
| — | Audit | Coverage audit: backend vs frontend vs MCP (5.5% MCP coverage) | ✅ |
| 58 | Feature | Agents page (create/manage/activity) + 5 MCP tools (update/archive/restore/pin/unpin memory) | ✅ |
| 59 | Feature | MCP global_search + list_code_projects + bulk_delete_memories + export buttons | ✅ |
| 60 | Feature | MCP merge_memories + bulk_tag + collections; Policies frontend page | ✅ |
| 48 | UI/UX | Agents page Apple token audit | ✅ |
| 61 | Feature | MCP import_memories + rename_tag + set_announcement + check_policy | ✅ |
| 62 | Feature | MCP get_project_context + list/create projects; backend context.rs conventions | ✅ |
| 63 | Feature | MCP conventions CRUD (list/get/store/update/archive/restore/delete) | ✅ |
| 64 | Feature | MCP users/roles/webhooks/org tools (15+ new tools) | ✅ |
| 65 | Feature | MCP api-keys/audit/stats tools; Conventions page (category sidebar, search, sort) | ✅ |
| 66 | Feature | MCP user management + webhook tools | ✅ |
| 49 | UI/UX | Policies page + Conventions page token audit | ✅ |
| 50 | UI/UX | UI component sweep (Input/Button/Select/Toast/Switch/Table/Sidebar) | ✅ |
| 67 | Feature | Backend migration v36 (conventions table) + 7 API handlers + context injection | ✅ |
| 68 | Feature | Agents page (create/filter/leaderboard/activity) — dedicated management page | ✅ |
| 51 | UI/UX | Input bg-transparent → bg-white/[0.04], Button raw hex → accent-blue tokens | ✅ |
| 52 | UI/UX | Sidebar/Switch/Table/Select token sweep | ✅ |
| 53 | UI/UX | Orgs/Dashboard/Projects pages audit | ✅ |
| 69 | Feature | MCP import_memories + rename_tag + set_announcement (npm package) | ✅ |
| 70 | Feature | Create memory modal (New memory button, TagAutocomplete, project select) + Convention markdown preview | ✅ |
| 54 | UI/UX | Conventions import/export + Create memory modal token compliance | ✅ |
| 71 | Feature | MCP store_memory+search_memory enhanced (collection_id/pinned/archived) + export_memories; Dashboard Conventions widget | ✅ |
| 55 | UI/UX | Create memory modal + Convention markdown preview Raw/Preview toggle audit | ✅ |
| 72 | Feature | MCP get_memory_facets + get_usage_stats + update_session; Session inline rename in Memories.tsx | ✅ |
| 56 | UI/UX | Dashboard Conventions widget + Session rename audit | ✅ |
| 73 | Feature | MCP search_conventions (client-side) + list_sessions; Agents filter bar (All/Active/Inactive/Expired) + leaderboard | ✅ |
| 57 | UI/UX | Agents filter bar active pill (bg-[#272729] font-semibold shadow-sm) + leaderboard card | ✅ |
| — | Commit | Push b50ed56 — cycles 58–73, 30 files, 2573 insertions | ✅ |
| 74 | Feature | Convention inline edit (self-contained ConventionCard, Escape key) + MCP memory_health_check | ✅ |
| 75 | Feature | Convention templates dropdown (5 templates: Clean Arch / Commits / REST / Testing / DB) + MCP check_convention_compliance | ✅ |
| 58 | UI/UX | Convention templates dropdown token audit + inline edit fields | ✅ |
| 76 | Feature | Webhooks.tsx (2-col, event checkboxes, secret toggle) + Collections.tsx (grid, drawer modal, memory assign) | ✅ |
| 59 | UI/UX | Webhooks status dot raw hex → bg-status-success; Collections 0 violations | ✅ |
| 77 | Feature | Tags.tsx (tag cloud, inline rename, table) + MCP batch_archive/batch_restore/search_and_tag | ✅ |
| 60 | UI/UX | Tags.tsx token audit (pills, selected state, inline rename input) | ✅ |
| 78 | Feature | Global search page `/search` (Cmd+K shortcut, type tabs, result cards) + Announcement section in Settings | ✅ |
| 61 | UI/UX | Search.tsx rounded-[11px] + result cards + Settings announcement textarea | ✅ |
| 79 | Feature | MCP delete_session + get_session_memories + create_session; Session delete in Memories.tsx | ✅ |
| 80 | Feature | Memory detail slide-over panel + MCP get_memory_timeline + find_related_memories + pin_convention | 🔄 running |

---

## Historial pre-compaction (ciclos 1–23)

### Infraestructura y base

- **Migración sistema**: v1–v24, trackeado en `apps/backend/src/db/migrations.rs` + assertions en `tests/integration_test.rs`
- **git_cmd() helper** (`api/code.rs`): aumenta PATH con `/usr/bin:/usr/local/bin:/opt/homebrew/bin:/usr/local/git/bin` para que el proceso servidor encuentre `git`
- **Tailwind v4**: `bg-bg-primary/secondary` son inválidos — se usan `bg-[#1d1d1f]` (página) y `bg-[#272729]` (card)

### Features fundamentales (v1–v24)

| Migración | Feature |
|-----------|---------|
| v15 | `projects.event_overrides TEXT` — overrides de eventos por proyecto |
| v16 | `organizations.retention_days` — política de retención por org |
| v17 | `memories.archived_at` — soft delete en memorias |
| v18 | `organizations.custom_instructions` — instrucciones de agente por org |
| v19 | `memories.pinned INTEGER DEFAULT 0` — memories fijadas |
| v20 | Tabla `invite_links` — invitaciones con token de 7 días |
| v21 | Tabla `users` recreada con `email TEXT` nullable |
| v22 | `organizations.min_password_length` — política de contraseñas |
| v23 | `projects.archived_at` + `code_projects.reindex_interval_hours` |
| v24 | Tabla `webhook_deliveries` + índice `(webhook_id, delivered_at DESC)` |

### UI/UX Apple Design System (ciclos 1–23)

Tokens aplicados sistemáticamente en todos los archivos:
- **Pesos**: solo 300/400/600/700 (eliminado `font-medium` de todo el código)
- **Radios**: `rounded-[5px]` chips, `rounded-[8px]` inputs small, `rounded-[11px]` inputs main, `rounded-[18px]` cards, `rounded-full` pills/CTAs
- **Fondos**: `bg-[#1d1d1f]` página, `bg-[#272729]` cards
- **Focus**: `focus:border-accent-blue/60` (eliminado todo `focus:ring-*`)
- **Status**: `text-status-success/error/warning` (eliminado `text-green/red/yellow-*`)

---

## Feature Cycles (detalle)

### Ciclo 32 — Bulk Memory Tag Editing

**Qué se hizo**: Edición masiva de tags en memorias seleccionadas.

**Backend** (`apps/backend/src/api/admin.rs`, `src/db/queries.rs`, `router.rs`):
- `POST /v1/admin/memories/bulk-tag` — recibe `{ memory_ids: [], action: "add"|"remove", tag: "" }`
- Actualiza el JSON array de tags en cada memoria con `json_group_array`

**Frontend** (`apps/admin/src/pages/Memories.tsx`):
- `BulkActionBar` flotante cuando hay memorias seleccionadas
- Botones "Add tag" y "Remove tag" con input inline para el tag
- Flash de "Tags updated" en `text-status-success`

**Cómo probar**:
1. Ir a `/memories`, seleccionar 2+ memorias con checkboxes
2. Aparece la barra flotante en la parte inferior
3. Hacer clic en "Add tag", escribir un tag, Enter
4. Verificar que las memorias ahora tienen el tag

---

### Ciclo 33 — User Activity Drawer + Memory Edit History

**Qué se hizo**:
- A: Side drawer deslizable en Users page con actividad reciente del usuario
- B: Audit log entry al editar contenido de memoria

**Backend** (`apps/backend/src/api/memory.rs`):
- `PATCH /v1/memory/:id` ahora escribe `memory.updated` en audit log

**Frontend** (`apps/admin/src/pages/Users.tsx`):
- Click en fila de usuario → `UserActivityDrawer` slide-in desde la derecha
- `UserActivityFeed`: React Query lazy, últimas 30 entradas, chips de acción coloreados
- Escape key cierra el drawer

**Frontend** (`apps/admin/src/pages/Memories.tsx`):
- Después de guardar edición inline: ícono `History` + "Edited" flash 2s

**Cómo probar**:
1. `/users` → hacer clic en cualquier fila de usuario → abre drawer lateral
2. El drawer muestra acciones recientes con chips coloreados (azul=memory, amarillo=key, rojo=user)
3. `/memories` → editar contenido de una memoria → aparece el flash "Edited" con ícono History

---

### Ciclo 34 — Per-Project Memory Stats + Org Settings Export

**Qué se hizo**:
- A: Stats de memorias en el accordion de cada proyecto
- B: Botón "Export org config" en Settings para descargar JSON

**Backend** (`apps/backend/src/api/admin.rs`, `db/queries.rs`, `router.rs`):
- `GET /v1/projects/:id/stats` → `{ total_memories, memories_this_week, last_memory_at, top_tags }`
- `GET /v1/admin/export` → JSON con org settings, webhooks, projects (sin datos sensibles), `Content-Disposition: attachment`

**Frontend** (`apps/admin/src/pages/Projects.tsx`):
- En cada accordion expandido: fila de stats con total, esta semana, última actividad, chips de top tags

**Frontend** (`apps/admin/src/pages/Settings.tsx`):
- Botón ghost pill "Export org config" con ícono `Download` al final de la página

**Cómo probar**:
1. `/projects` → expandir cualquier proyecto → ver fila de stats arriba del contenido
2. `/settings` → scroll al final → clic "Export org config" → descarga `nexusmind-config.json`

---

### Ciclo 35 — Code Search Saved Queries

**Qué se hizo**: Guardar búsquedas semánticas de código para reutilizar.

**Frontend** (`apps/admin/src/pages/Code.tsx`):
- Botón "Save" aparece junto al input cuando hay query + proyecto seleccionado
- Click → popover inline con input de nombre → guardar en `localStorage` key `nexusmind-code-searches`
- Pill "Saved searches" con badge de conteo → dropdown listando búsquedas guardadas
- Click en búsqueda guardada → rellena proyecto + query + auto-submit
- Ícono trash por búsqueda para eliminar

**Cómo probar**:
1. `/code` → Tab "Search" → seleccionar proyecto + escribir query
2. Aparece botón "Save" → click → nombrar → Enter
3. Aparece pill "BookmarkCheck" con número → click → dropdown con búsqueda guardada
4. Click en búsqueda → auto-ejecuta la búsqueda

---

### Ciclo 36 — Memory Collections + Audit Log Session Grouping

**Qué se hizo**:
- A: Colecciones para organizar memorias (migration v25)
- B: Agrupación de audit log por sesión (client-side)

**Migración v25** (`apps/backend/src/db/migrations.rs`):
```sql
CREATE TABLE collections (id, org_id, name, description, created_at, UNIQUE(org_id, name));
ALTER TABLE memories ADD COLUMN collection_id TEXT REFERENCES collections(id) ON DELETE SET NULL;
CREATE INDEX idx_memories_collection ON memories(org_id, collection_id);
```

**Backend** (`admin.rs`, `queries.rs`, `router.rs`):
- `GET /v1/admin/collections` — lista con count de memorias
- `POST /v1/admin/collections` — crear
- `DELETE /v1/admin/collections/:id` — eliminar (memorias quedan con collection_id=NULL)
- `POST /v1/memories/:id/collection` — asignar/desasignar

**Frontend** (`apps/admin/src/pages/Memories.tsx`):
- 5° tab "Collections" con cards, formulario de creación, eliminación
- Ícono `Folder` por memoria → dropdown para asignar a colección
- Filtro "Collection" en el filter bar

**Frontend** (`apps/admin/src/pages/AuditLog.tsx`):
- Toggle "Group by session" → agrupa entradas por `user_id + día`
- Headers colapsables con conteo de entradas
- Entradas indentadas: `border-l-2 border-l-accent-blue/20 ml-4 pl-4`

**Cómo probar**:
1. `/memories` → tab "Collections" → crear colección → asignar memorias con ícono Folder
2. Filtrar por colección en el filtro bar
3. `/audit` → toggle "Group by session" → ver entradas agrupadas + colapsables

---

### Ciclo 37 — Dashboard Memory Heatmap

**Qué se hizo**: Heatmap estilo GitHub en el Dashboard (últimos 90 días).

**Backend** (`admin.rs`, `queries.rs`, `router.rs`):
- `GET /v1/admin/stats/memory-heatmap` → array de `{ day: "YYYY-MM-DD", count: N }`

**Frontend** (`apps/admin/src/pages/Dashboard.tsx`):
- Card "Memory Activity" con grid 13×7 (columnas=semanas, filas=días)
- 4 niveles de intensidad: `bg-accent-blue/20/40/60` + `bg-accent-blue` sólido
- `title` nativo por celda con fecha + conteo
- Leyenda "Less → More" debajo del heatmap
- Loading skeleton mientras carga

**Cómo probar**:
1. `/` (Dashboard) → scroll hasta "Memory Activity"
2. Celdas vacías = `bg-white/[0.04]`, días con actividad = azul con intensidad relativa
3. Hover sobre celda → tooltip con fecha y número de memorias

---

### Ciclo 38 — Memory Templates

**Qué se hizo**: Templates predefinidos para crear memorias (localStorage).

**Frontend** (`apps/admin/src/pages/Settings.tsx`):
- Sección "Memory Templates" entre Agent Events y Webhooks
- Templates guardados en `localStorage` key `nexusmind-memory-templates`
- Cada template: `{ id, name, type, content }`
- CRUD completo: crear, editar, eliminar
- Badge de tipo coloreado (auto/manual/summary/reflection)

**Cómo probar**:
1. `/settings` → sección "Memory Templates"
2. "Add template" → llenar nombre, tipo, contenido → Save
3. Template aparece en la lista con badge de tipo
4. Editar con ícono lápiz, eliminar con trash

---

### Ciclo 39 — Global Tag Rename + Webhook Retry

**Qué se hizo**:
- A: Renombrar un tag en todas las memorias de un org de una vez
- B: Reintentar webhook deliveries fallidos

**Backend** (`queries.rs`, `admin.rs`, `router.rs`):
- `POST /v1/admin/tags/rename` — `{ from: "old", to: "new" }` → actualiza JSON arrays en transaction
- `POST /v1/webhooks/deliveries/:delivery_id/retry` — re-dispara la entrega original y guarda nueva entrada

**Frontend** (`apps/admin/src/pages/Memories.tsx`, Tags tab):
- Ícono `Edit2` en hover por fila de tag → click → input inline pre-llenado
- Enter = guardar, Escape = cancelar
- Flash "Renamed" en `text-status-success`

**Frontend** (`apps/admin/src/pages/Settings.tsx`, deliveries panel):
- Botón "Retry" (`text-[10px] border ... rounded-full`) en filas de delivery fallido
- Al hacer retry: invalida `['webhook-deliveries']`

**Cómo probar**:
1. `/memories` → tab "Tags" → hover sobre cualquier tag → ícono lápiz
2. Editar nombre → Enter → "Renamed" flash → todas las memorias con ese tag actualizadas
3. `/settings` → sección Webhooks → ver deliveries fallidos → botón "Retry"

---

### Ciclo 40 — Top Contributors Dashboard + Search Highlighting

**Qué se hizo**:
- A: Card "Top Contributors" en Dashboard (últimos 30 días)
- B: Highlight del término de búsqueda en resultados de memorias

**Backend** (`queries.rs`, `admin.rs`, `router.rs`):
- `GET /v1/admin/stats/top-contributors` → top 8 `user_id` por conteo en últimos 30 días

**Frontend** (`apps/admin/src/pages/Dashboard.tsx`):
- Card "Top Contributors" después del heatmap
- Filas con rank #, user_id (monospace), barra de progreso relativa al top, conteo
- Loading skeleton de 3 filas

**Frontend** (`apps/admin/src/pages/Memories.tsx`):
- `highlightMatch(text, query)`: regex-escaped, case-insensitive → `<mark className="bg-accent-blue/20 text-accent-blue rounded-[2px]">`
- Se aplica al contenido de la memoria cuando hay búsqueda activa (≥2 chars)

**Cómo probar**:
1. `/` (Dashboard) → scroll hasta "Top Contributors" → ver usuarios con más memorias
2. `/memories` → buscar cualquier término → el texto en las filas resalta las coincidencias en azul

---

### Ciclo 41 — Code Project Sync Status + Retention Preview ✅

**Qué se hizo**:

**Migración v26** (`migrations.rs`):
```sql
ALTER TABLE code_projects ADD COLUMN last_indexed_at TEXT;
ALTER TABLE code_projects ADD COLUMN last_index_error TEXT;
ALTER TABLE code_projects ADD COLUMN indexed_files_count INTEGER DEFAULT 0;
ALTER TABLE code_projects ADD COLUMN index_status TEXT DEFAULT 'pending';
```

**Backend**:
- Al indexar: actualiza `index_status` a `indexing` → luego `success`/`error` con timestamp
- `GET /v1/admin/settings/retention-preview` → `{ would_delete: N, retention_days: N }`

**Frontend** (`apps/admin/src/pages/Code.tsx`):
- Badge de status por repo card: Pending/Indexing (spinner)/Success (✓ + tiempo)/Error (⚠ + mensaje)
- Chip "N files indexed" si hay archivos indexados

**Frontend** (`apps/admin/src/pages/Settings.tsx`):
- Bajo el select de Data Retention: "X memories would be deleted with current settings"

**Cómo probar**:
1. `/code` → tab "Repositories" → ver badge de estado por repositorio (Pending/Synced/Error)
2. Disparar un reindex → badge cambia a "Indexing…" con `RefreshCw` animado → luego "Synced X ago" con ✓
3. Si indexación falla → badge rojo "Error" con ⚠ (hover = mensaje de error)
4. Chip "N files indexed" aparece cuando hay archivos indexados
5. `/settings` → cambiar Data Retention a "30 days" → ver "X memories would be deleted..."

---

## UI/UX Cycles (detalle)

### Ciclo 24 — Layout: Notification Bell

**Fixes**: `w-full` en nav links, `text-text-secondary` correcto en estado inactivo.

### Ciclo 25 — Users Drawer + Bulk Tag Bar

**Fixes en `Users.tsx`**: Chips de acción raw colors → tokens (`text-accent-blue`, `text-status-warning/error`), skeletons `bg-[#272729]`.
**Fixes en `Memories.tsx`**: `hover:border-border-primary` en botones Add/Remove tag.

### Ciclo 26 — Code Saved Queries + Projects Stats + Settings Export

**Fixes en `Code.tsx`**: Pill → `rounded-full`, badge sólido `w-4 h-4`, dropdown reestructurado, popover input `bg-white/[0.04]`.
**`Projects.tsx`** y **`Settings.tsx`**: ya conformes.

### Ciclo 27 — Collections Tab + Audit Grouping

**Fixes en `Memories.tsx`** (13+ fixes):
- `TYPE_META` raw colors → tokens en los 8 tipos
- `focus:ring-*` eliminado de FacetSelect, search input, collection select
- Tab buttons → `px-3 py-1.5 rounded-full`
- Collection cards `bg-[#272729] p-5`
- Memory count → chip con `rounded-[5px]`
- Assignment dropdown reestructurado

**Fixes en `AuditLog.tsx`**:
- Toggle pill: tamaño correcto, active state `bg-white/[0.06]`
- Group header: `text-[11px] uppercase tracking-wide text-text-quaternary`
- Entry count: `rounded-[5px]`
- Indentation: `border-l-2 border-l-accent-blue/20 ml-4 pl-4`

### Ciclo 28 — Dashboard Heatmap Card

Sin fixes necesarios — la implementación del ciclo 37 fue perfectamente conforme desde el inicio.

### Ciclo 29 — Settings Templates + ApiKeys Sweep

**Fixes en `Settings.tsx`** (`MemoryTemplatesSection`):
- "Add template" button: `py-1.5`, `text-text-secondary`
- Card rows: `bg-[#272729]` (sin opacity variant)
- Form buttons: Cancel → texto plano, Save → `rounded-[8px] text-xs px-3`

**`ApiKeys.tsx`**: 100% conforme, sin fixes.

### Ciclo 30 — Tag Rename + Webhook Retry + Roles Audit

**Fixes en `Memories.tsx`** (Tags tab):
- Rename input: `bg-white/[0.04] rounded-[8px] text-xs focus:border-accent-blue/60`
- Pencil hover: `hover:text-text-primary transition-opacity` (no accent-blue, no bg fill)
- Tag rows: refactor completo de grid → flat list con `border-b border-border-secondary/30`

**`Settings.tsx`**: webhook retry button ya conforme.

**Fixes en `Roles.tsx`**:
- `focus:border-border-focus` → `focus:border-accent-blue/60` en 3 inputs
- `focus:ring-*` en checkbox → `focus:outline-none`
- Role list: rows → individual cards `rounded-[18px] p-5`
- System badge: `bg-white/[0.06] text-text-quaternary rounded-[5px]`

### Ciclo 31 — Dashboard Contributors + Code.tsx

**Fixes en `Dashboard.tsx`**: Loading skeleton para Contributors card (3 filas `animate-pulse`).

**Fixes en `Code.tsx`**:
- Repo cards: agregado `bg-[#272729]`
- Search result cards: agregado `bg-[#272729]`
- URL text: `text-text-tertiary` → `text-text-secondary`

---

## Convenciones de código establecidas

### Apple Design Tokens (OBLIGATORIO)

```tsx
// ❌ Prohibido
className="font-medium focus:ring-2 focus:ring-blue-500 text-green-400 bg-gray-800 rounded-lg"

// ✅ Correcto
className="font-semibold focus:border-accent-blue/60 focus:outline-none text-status-success bg-[#272729] rounded-[18px]"
```

### Backgrounds
```tsx
// Página
"bg-[#1d1d1f]"
// Card / dropdown / panel
"bg-[#272729]"
```

### Radios
```tsx
rounded-[5px]   // chips, badges pequeños
rounded-[8px]   // inputs small, botones de acción
rounded-[11px]  // inputs principales, dropdowns
rounded-[18px]  // cards
rounded-full    // pills, CTAs, botones ghost principales
```

### Pesos tipográficos
```tsx
font-light     // 300 — subtítulos muy pequeños
font-normal    // 400 — texto de cuerpo
font-semibold  // 600 — headings, botones, valores numéricos
font-bold      // 700 — métricas principales únicamente
```

### Focus
```tsx
// ❌
focus:ring-2 focus:ring-accent-blue/20

// ✅
focus:border-accent-blue/60 focus:outline-none
```

### Colores de estado
```tsx
text-status-success   // verde — éxito, activo, reciente
text-status-error     // rojo — error, peligro, eliminar
text-status-warning   // amarillo — advertencia, en progreso
text-accent-blue      // azul — acciones, enlaces, resaltado
```

### Patrón hover en iconos de fila
```tsx
// Íconos de acción que aparecen en hover
"opacity-0 group-hover:opacity-100 transition-opacity text-text-quaternary hover:text-text-primary w-3.5 h-3.5"

// Para acciones destructivas (eliminar)
"opacity-0 group-hover:opacity-100 transition-opacity text-text-quaternary hover:text-status-error w-3.5 h-3.5"
```

---

## Endpoints API añadidos (resumen)

| Método | Ruta | Descripción |
|--------|------|-------------|
| POST | `/v1/admin/memories/bulk-tag` | Bulk add/remove tag |
| POST | `/v1/admin/memories/merge` | Merge memorias duplicadas |
| POST | `/v1/admin/memories/import` | Import masivo JSON |
| GET | `/v1/admin/stats/agent-activity` | Actividad de agentes |
| GET | `/v1/admin/stats/usage` | Stats de uso del org |
| GET | `/v1/admin/stats/tags` | Lista de tags con conteo |
| GET | `/v1/admin/stats/duplicates` | Memorias duplicadas |
| GET | `/v1/admin/stats/trends` | Tendencias de memorias |
| GET | `/v1/admin/stats/memory-heatmap` | Heatmap últimos 90 días |
| GET | `/v1/admin/stats/top-contributors` | Top usuarios por memorias |
| GET | `/v1/admin/notifications` | Notificaciones en-app |
| GET | `/v1/admin/collections` | Listar colecciones |
| POST | `/v1/admin/collections` | Crear colección |
| DELETE | `/v1/admin/collections/:id` | Eliminar colección |
| POST | `/v1/admin/tags/rename` | Renombrar tag globalmente |
| GET | `/v1/admin/export` | Export config org (JSON) |
| GET | `/v1/admin/settings/retention-preview` | Preview de retención |
| POST | `/v1/projects/:id/archive` | Archivar proyecto |
| POST | `/v1/projects/:id/restore` | Restaurar proyecto |
| GET | `/v1/projects/:id/stats` | Stats de memorias del proyecto |
| POST | `/v1/memory/:id/archive` | Archivar memoria |
| POST | `/v1/memory/:id/restore` | Restaurar memoria |
| POST | `/v1/memory/:id/pin` | Pinear memoria |
| POST | `/v1/memory/:id/unpin` | Despinear memoria |
| POST | `/v1/memories/:id/collection` | Asignar a colección |
| POST | `/v1/admin/users/:id/reset-key` | Resetear API key de usuario |
| POST | `/v1/admin/invites` | Crear invite link |
| POST | `/v1/invites/:token/redeem` | Redimir invite |
| GET | `/v1/invites/:token` | Validar invite (público) |
| POST | `/v1/webhooks/:id/test` | Test webhook |
| GET | `/v1/webhooks/:id/deliveries` | Historial de deliveries |
| POST | `/v1/webhooks/deliveries/:id/retry` | Retry delivery fallido |
| PATCH | `/v1/code/projects/:id/schedule` | Actualizar intervalo de reindex |

---

*Actualizado automáticamente por el loop de mejora continua.*
