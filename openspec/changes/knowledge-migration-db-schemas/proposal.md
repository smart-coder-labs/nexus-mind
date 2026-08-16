# Proposal — Conector: esquemas Postgres y Supabase

> **Change**: `knowledge-migration-db-schemas`
> **Status**: proposed
> **Owner**: backend + tooling
> **Date**: 2026-08-15
> **Depends on**: `knowledge-migration-core`

---

## 1. Intent

### Problema

El modelo de datos real de un cliente vive en su base, no en su documentación. Los nombres de
tabla, las FKs, los CHECK constraints, los índices y los comentarios son la descripción más
honesta de su negocio que existe — y suelen ser lo único que no está desactualizado.

Un agente que trabaja sobre el proyecto de un cliente no sabe que `invoices.status` solo admite
cinco valores, ni que `orders` referencia `customers` con `ON DELETE RESTRICT` por una razón.
Lo descubre rompiendo algo.

Hoy NexusMind no lee ninguna base externa. `src/backup/` va en dirección contraria: espeja
SQLite → Postgres para respaldo (`BACKUP_DATABASE_URL`).

### Success looks like

- Un operador apunta el conector a la base de un cliente con un rol **read-only** y obtiene
  candidatos que describen su modelo de datos en prosa útil.
- Las políticas RLS de Supabase quedan documentadas como reglas de acceso, que es lo que son.
- **Por defecto no se lee ni una fila** de tablas de negocio.
- Cuando el operador **opta explícitamente** por indexar información, lo hace tabla por tabla,
  con redacción de PII y dejando constancia de quién lo autorizó.

---

## 2. Scope

### In scope

1. **Extracción de esquema (por defecto, y sin datos)** — vía `information_schema` y
   `pg_catalog`: tablas, columnas, tipos, nullability, defaults, PKs, FKs, UNIQUE, CHECK,
   índices, vistas, funciones, triggers, enums, y **`COMMENT ON`** (la documentación que el DBA
   ya escribió y nadie lee).
2. **Específicos de Supabase** — políticas RLS, esquema `auth`, buckets de storage, y los
   archivos `supabase/migrations/*.sql` si están en el repo. Supabase **es** Postgres: comparte
   el 90% del conector y se diferencia en estos extras.
3. **Modo `--include-data` (opt-in, decisión D3)** — permite al operador indexar información de
   tablas concretas para que el LLM infiera semántica ("esta columna `status` guarda
   `draft|sent|paid`"). Bajo cuatro condiciones **acumulativas**:
   - **allowlist explícita de tablas** — nunca `--all`;
   - **`LIMIT` acotado** con muestreo determinista;
   - **redacción de PII antes de que la muestra salga del proceso local** — emails, teléfonos,
     documentos de identidad, tarjetas, nombres en columnas marcadas;
   - **attestation del operador** registrada en el run, reusando
     `provenance_kind='client_attested'` que v56 ya define.
4. **Destinos** — memories `architecture` por área del esquema (no una por tabla: un esquema de
   200 tablas produciría 200 memories inútiles), conventions para las reglas que el esquema
   impone, y opcionalmente tasks para defectos evidentes (tabla sin PK, FK sin índice).
5. **Conexión read-only** — el conector se niega a arrancar si el rol tiene permisos de
   escritura. Reusa el patrón de connection string y pooling de `backup/client.rs`.

### Out of scope

- **Ingesta de datos de negocio completos.** NexusMind no es un data warehouse.
- Otros motores (MySQL, Mongo, SQL Server).
- Monitorización de deriva del esquema. Esto es una migración, no un observador.
- Lectura de `pg_stat_*` y planes de consulta: es rendimiento, no conocimiento.

---

## 3. Approach

```
# por defecto: cero filas leídas
migrate-knowledge --source db-schema --dsn $CLIENT_RO_DSN \
                  --client acme --project acme-billing [--supabase] [--dry-run]

# opt-in explícito, tabla por tabla
migrate-knowledge --source db-schema --dsn $CLIENT_RO_DSN --client acme \
                  --include-data --tables invoices,order_status --sample-limit 10 \
                  --redact-pii --attest "autorizado por <persona> el <fecha>"
```

`source_identity` = `pg:{database}:{schema}.{object}:{ddl_sha}`

El `ddl_sha` hace que una migración del cliente que cambia una tabla vuelva a proponer **esa**
tabla y solo esa.

### Rationale

- **Schema-only por defecto, datos como excepción ruidosa.** El default seguro es el que se
  usa el 95% de las veces; leer datos debe requerir escribir explícitamente qué tablas y por
  qué autorización. Un flag que se olvida activar es un fallo; un flag que hay que activar a
  propósito es una decisión.
- **Agrupar por área, no por tabla.** El conocimiento útil es "el módulo de facturación modela
  X con estas reglas", no 200 fichas sueltas.
- **Rechazar credenciales con escritura.** Una migración no tiene ninguna razón para poder
  escribir en la base de un cliente. Si puede, es un incidente esperando ocurrir.

---

## 4. Risks & open questions

| Riesgo | Mitigación |
|---|---|
| **PII cruzando a NexusMind vía `--include-data`.** Es el riesgo dominante de este change. | Las cuatro condiciones acumulativas de §2.3. La redacción ocurre **en local**, antes de que la muestra entre al pipeline. La attestation deja nombre y fecha. Los tests de redacción son criterio de aceptación. |
| **Datos de cliente enviados a la API de Anthropic** al inferir semántica con `claude -p`. | Misma pregunta de NDA abierta en `git-history`, y aquí es más grave porque son datos, no código. **Recomendación: `--include-data` queda bloqueado hasta que la pregunta de NDA por cliente esté respondida por escrito.** |
| **Credencial de cliente en la línea de comandos** (queda en el historial del shell). | Solo por variable de entorno o prompt interactivo; el DSN nunca como argumento literal, y nunca se persiste en `migration_runs`. |
| **Esquemas enormes.** 500 tablas → coste alto y candidatos inmanejables. | Agrupación por área, filtro por esquema, y `--dry-run` que reporta objetos y coste estimado. |
| **Confundir el esquema de NexusMind con el del cliente.** El conector corre en la misma máquina que el backup mirror. | El DSN es siempre explícito; sin default, sin fallback a `BACKUP_DATABASE_URL`. |

**Pregunta abierta (bloquea `--include-data`, no el resto del change):** la respuesta por
escrito de si algún NDA de cliente prohíbe el procesamiento de sus datos por un tercero. El
conector schema-only puede especificarse e implementarse sin esperarla; el modo de datos no.
