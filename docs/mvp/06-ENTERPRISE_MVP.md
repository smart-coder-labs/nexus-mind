# NexusMind — MVP Enterprise Plan (v2)

> **Documento**: 06-ENTERPRISE_MVP.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: MVP pensado para vender a empresas, no solo para que un developer juegue. El foco cambia de "memoria cross-tool" a "control centralizado multi-usuario".

---

## 1. ⚠️ Corrección de Foco

El MVP v1 (documentos 01-05) estaba centrado en:
> "Un developer guarda y recupera memoria entre sesiones de Claude Code"

Pero para vender a una empresa necesitas:
> "El CTO invita a su equipo de 5 developers, cada uno con su API Key, toda la memoria del equipo es visible y gobernable desde un panel, y hay un audit trail de quién usó qué."

**La diferencia es radical.** Sin multi-usuario, sin org/proyecto como tenant, sin admin panel funcional, no tienes demo que mostrarle a un comprador enterprise.

---

## 2. 🎯 Scope Enterprise del MVP (v2)

| Lo que SÍ necesita una demo enterprise | Lo que NO necesita |
|---|---|
| Multi-usuario con API keys individuales | Plugin Claude Code pulido |
| Organizaciones como tenant de datos | Políticas ABAC complejas |
| Admin panel web para el "admin" de la org | TUI (ratatui) |
| Audit trail por usuario visible en panel | SDK en 3 lenguajes |
| Invitar/remover usuarios desde el panel | Vector search / embeddings |
| Separación de datos entre orgs | On-prem ARM |
| Demo preparada con datos de ejemplo | Hash chain / Merkle audit |

---

## 3. 🗺️ Sprint Plan Enterprise (4 Semanas)

### Semana 1: Backend Multi-Tenant (Rust)

**Objetivo**: API con organizaciones, usuarios, API keys por usuario, datos aislados por org.

| Día | Tarea |
|---|---|
| 1 | Modelo de datos multi-tenant: `organizations`, `users`, `org_members` |
| 2 | Auth con API key por usuario + validación de pertenencia a org |
| 3 | Memory API scoped por org (cada query filtra por org_id del API key) |
| 4 | Endpoints: invite user, list members, rotate key |
| 5 | Audit trail con org_id + user_id |
| 6-7 | Tests + seed data para demo (3 orgs, 5 users c/u, 50 memorias) |

### Semana 2: Admin Panel Enterprise (React)

**Objetivo**: Panel web donde el admin de una org ve todo lo que pasa.

| Día | Tarea |
|---|---|
| 1 | Login con API key → detecta org + rol |
| 2 | Dashboard: stats (usuarios activos, memorias totales, búsquedas hoy) |
| 3 | User management: listar, invitar, revocar acceso |
| 4 | Memory browser: ver todas las memorias de la org, filtrar por usuario |
| 5 | Audit log: tabla con fecha, usuario, acción, tool |
| 6 | Settings: API keys, cambiar nombre de org |
| 7 | Polish + responsive |

### Semana 3: Modo Demo Integrado

**Objetivo**: Un script que en 2 comandos levanta el backend + admin + datos de ejemplo.

| Día | Tarea |
|---|---|
| 1 | Script `reset-demo.sh` — crea DB limpia con datos de ejemplo |
| 2 | Datos de ejemplo realistas: "Acme Corp" con 5 usuarios y ~100 memorias |
| 3 | Escenario de demo guiado (PDF o README): "Esto es lo que le muestras al CTO" |
| 4 | Landing page actualizada con "Book a Demo" CTA |
| 5 | Docker compose optimizado para demo (1 comando) |
| 6 | Grabación de demo (opcional) |
| 7 | Bug bash enfocado en flujo enterprise |

### Semana 4: Polish + Release v0.2.0

| Día | Tarea |
|---|---|
| 1 | README enterprise-focused (no "para developers", sino "para equipos") |
| 2 | Makefile + CI |
| 3 | Video/screenshots para el pitch |
| 4 | Bug bash con 3 personas externas simulando ser empresa |
| 5 | **Release v0.2.0** — "Enterprise Demo Ready" |

---

## 4. 📦 Package Structure (Enterprise MVP)

```
nexus-mind/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── lib.rs
│   ├── auth/
│   │   ├── mod.rs
│   │   └── api_keys.rs          # Key generation + org scoping
│   ├── db/
│   │   ├── mod.rs
│   │   ├── connection.rs        # SQLite + WAL
│   │   ├── migrations.rs        # Schema multi-tenant
│   │   └── queries.rs           # Queries scoped by org_id
│   ├── api/
│   │   ├── mod.rs
│   │   ├── router.rs            # Axum router
│   │   ├── middleware.rs        # Auth + org scoping
│   │   ├── health.rs
│   │   ├── memory.rs            # Scoped by org from API key
│   │   ├── audit.rs             # Scoped by org
│   │   ├── users.rs             # Invite, list, remove
│   │   └── admin.rs             # Org settings, stats
│   └── models/
│       ├── mod.rs
│       └── types.rs             # Org, User, Memory, AuditEvent
├── admin/                       # React panel (enterprise-focused)
│   ├── src/
│   │   ├── pages/
│   │   │   ├── Login.tsx        # Login con API key
│   │   │   ├── Dashboard.tsx    # Stats + activity
│   │   │   ├── Users.tsx        # Invitar/remover usuarios
│   │   │   ├── Memories.tsx     # Ver toda la memoria de la org
│   │   │   ├── AuditLog.tsx     # Audit trail completo
│   │   │   └── Settings.tsx     # Org config, API keys
│   │   └── api/client.ts
│   └── ...
├── scripts/
│   └── reset-demo.sh            # Recrea DB con datos de ejemplo
├── demo/
│   ├── DEMO_SCRIPT.md           # Guía paso a paso para la demo
│   └── screenshots/             # Para el pitch deck
├── docker-compose.yml
├── Dockerfile
└── README.md
```

---

## 5. 🗄️ Data Model (Multi-Tenant)

```sql
-- Organizaciones (tenant principal)
CREATE TABLE organizations (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    slug        TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Usuarios (pertenecen a una org)
CREATE TABLE users (
    id          TEXT PRIMARY KEY,
    org_id      TEXT NOT NULL REFERENCES organizations(id),
    email       TEXT NOT NULL,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'member',  -- 'admin', 'member', 'viewer'
    status      TEXT NOT NULL DEFAULT 'active',   -- 'active', 'invited', 'suspended'
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(org_id, email)
);

-- API keys (por usuario, scoped a su org)
CREATE TABLE api_keys (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    org_id      TEXT NOT NULL REFERENCES organizations(id),
    key_hash    TEXT NOT NULL UNIQUE,
    label       TEXT NOT NULL,
    last_used   TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    revoked     INTEGER NOT NULL DEFAULT 0
);

-- Memorias (scoped por org, no por usuario individual)
CREATE TABLE memories (
    id          TEXT PRIMARY KEY,
    org_id      TEXT NOT NULL REFERENCES organizations(id),
    user_id     TEXT NOT NULL REFERENCES users(id),
    project     TEXT NOT NULL DEFAULT 'default',
    tool        TEXT NOT NULL,
    content     TEXT NOT NULL,
    tags        TEXT DEFAULT '[]',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- FTS5 index (scoped queries via org_id + MATCH)
CREATE VIRTUAL TABLE memories_fts USING fts5(
    content, tags,
    content='memories',
    content_rowid='rowid'
);

-- Audit trail (scoped por org)
CREATE TABLE audit_logs (
    id              TEXT PRIMARY KEY,
    org_id          TEXT NOT NULL REFERENCES organizations(id),
    user_id         TEXT NOT NULL REFERENCES users(id),
    timestamp       TEXT NOT NULL DEFAULT (datetime('now')),
    action          TEXT NOT NULL,
    resource_type   TEXT NOT NULL,
    resource_id     TEXT,
    metadata        TEXT DEFAULT '{}'
);
```

**Regla de oro**: Toda query en `queries.rs` recibe `org_id` del token de auth. Nunca se filtra por user_id en memoria — la memoria es de la org.

---

## 6. 🖥️ Admin Panel: Páginas Clave para la Demo

### Dashboard (para el CTO)
- Total de memorias almacenadas
- Usuarios activos (últimas 24h)
- Búsquedas realizadas hoy
- Tools más usadas
- Timeline de actividad reciente

### Users (para el admin)
- Lista de miembros con rol + status
- Botón "Invite User" → envía link (o copia API key)
- Botón "Revoke Access" → desactiva usuario
- Ver API keys de cada usuario

### Memories (para el compliance officer)
- Todas las memorias de la org
- Filtros: por usuario, por tool, por proyecto, por fecha
- Búsqueda full-text
- Botón "Export" (CSV)

### Audit Log (para el compliance officer)
- Toda acción registrada con timestamp
- Filtros: usuario, acción, tool, rango de fechas
- Exportable

---

## 7. 🎭 Demo Script

```
ESCENA: Llamada de ventas con VP Eng de una empresa mediana

1. "Mira, en 2 minutos tienes el control plane funcionando"
   → docker compose up -d
   → Abrir admin portal

2. "Esta es tu organización. Ya tiene 5 usuarios de ejemplo."
   → Dashboard muestra 5 usuarios activos, 89 memorias, 3 tools

3. "Cada developer tiene su propia API Key"
   → Sección Users: ver keys, invitar nuevo usuario

4. "Toda la memoria del equipo es visible y searchable"
   → Memories: buscar "authentication" → ver memorias de 3 usuarios distintos

5. "Sabes exactamente qué pasó y quién lo hizo"
   → Audit Log: "Ayer a las 14:32, Ana (Claude Code) guardó 'cambiar a OAuth2'"

6. "Puedes exportar todo si necesitas auditoría"
   → Export CSV del audit log

7. "Y si mañana alguien se va, revocas su acceso en 1 click"
   → Revoke user → su API key deja de funcionar
```

---

## 8. 📊 Lo que Cambia del MVP v1 al v2

| Aspecto | MVP v1 (Developer) | MVP v2 (Enterprise) |
|---|---|---|
| **Audiencia** | Developer individual | CTO / VP Eng / Compliance |
| **Unidad de datos** | Por usuario/ proyecto | Por organización |
| **Multi-usuario** | No | Sí (desde el día 1) |
| **Admin panel** | CRUD básico | Dashboard + Users + Audit |
| **Auth** | API key única | API key por usuario + roles |
| **Demo-ready** | No (solo curl) | Sí (script reset + datos) |
| **Valor de venta** | "Memoria cross-tool" | "Gobierno centralizado" |
| **MCP plugin** | P0 | P1 (post-MVP) |
| **Claude Code** | Única integración | Se pospone |

**Conclusión**: Sin multi-tenancy y sin admin panel funcional, no tienes producto que mostrar en una llamada enterprise.

---

## 9. ⏱️ Estimación Ajustada

| Componente | Esfuerzo | Prioridad |
|---|---|---|
| Backend multi-tenant (orgs, users, API keys scoped) | 5 días | P0 |
| Admin panel (Dashboard + Users + Memories + Audit) | 7 días | P0 |
| Script reset-demo + datos de ejemplo | 2 días | P0 |
| Demo script + screenshots | 1 día | P0 |
| Docker compose + README enterprise | 2 días | P0 |
| MCP plugin Claude Code | Se omite en v2 | P1 |
| **Total** | **~17 días** | |

---

## 10. 🧪 Cómo Probar el MVP Enterprise

```bash
# 1. Levantar todo
docker compose up -d

# 2. Seed con datos de ejemplo (3 orgs, 15 usuarios, ~200 memorias)
./scripts/reset-demo.sh

# 3. Abrir admin panel
open http://localhost:3000

# 4. Login como admin de "Acme Corp"
#    Email: admin@acme.com
#    API Key: nm_demo_admin_acme_xxx

# 5. Explorar:
#    - Dashboard: stats de la org
#    - Users: ver los 5 miembros, invitar uno nuevo
#    - Memories: buscar "authentication", filtrar por usuario
#    - Audit: ver timeline completo

# 6. Login como member de "Acme Corp"
#    Solo puede ver sus propias memorias (o todas, según política)
```

---

*Fin de 06-ENTERPRISE_MVP.md*
