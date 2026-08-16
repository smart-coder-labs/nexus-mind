# Apply Progress — Database Schema Connector

> **Change**: `knowledge-migration-db-schemas`
> **Branch**: `sdd/knowledge-migration-core` (mismo PR)
> **Date**: 2026-08-15
> **Desbloqueado por**: ADR `db359a75`

---

## Estado: ✅ 10 tareas completas — la épica queda cerrada

| Gate | Resultado |
|---|---|
| `cargo test` | **1223 lib + 46 integración + 14 runner — 0 fallos** |
| `cargo clippy -- -D warnings` | limpio |

Tests netos: **+24** (1199 → 1223). El módulo `migration` suma **102 tests**.

---

## La decisión de diseño que hace testeable lo que no tiene base de datos

Los otros tres conectores leen el filesystem: un `TempDir` y ya hay test. Este habla con
Postgres, y **no hay Postgres en CI**.

La salida no fue "tests de integración que alguien correrá a mano", sino partir en dos:

- **`SchemaReader` (trait)** — el I/O. Una implementación real con sqlx, una fake en memoria.
- **`DbSchemaConnector`** — las cuatro puertas, la agrupación, la redacción, la prosa.

**Todo lo que puede estar mal de forma interesante vive del lado testeable.** El lado de I/O es
una traducción de SQL a structs cuyo modo de fallo es un nombre de columna mal escrito, y eso lo
caza el primer run real.

Un test de integración marcado `#[ignore]` que nadie corre **parece cobertura y no lo es**. Decir
dónde está el límite es mejor que fingir que no existe.

Es además mejor diseño con independencia del test: el día que haya que soportar MySQL, se
implementa el trait otra vez.

---

## Las cuatro puertas, como función pura

`SamplingPolicy::authorize()` no toca la base de datos. Devuelve **cuál** de las cuatro
condiciones falta, y por eso el mensaje de error dice `--attest is required: a run that reads
client data must record who authorised it` en vez de un "no autorizado" que obliga a adivinar.

Cada puerta tiene su test, y ninguno necesita Postgres:

| Puerta | Qué impide |
|---|---|
| Allowlist explícita | No hay `--all`. El operador escribe qué tablas, una a una. |
| `LIMIT` acotado y determinista | `ORDER BY 1`, nunca `RANDOM()`: dos runs sobre datos sin cambiar deben dar la misma muestra, o la idempotencia del pipeline se rompe. |
| Redacción antes de salir del proceso | El mismo `super::redact` de los otros conectores. |
| Attestation del operador | Queda en el run. Un run con datos siempre dice quién lo autorizó. |

Un candidato con muestra sale como `client_attested`, no `verified_manifest` — así la UI de
revisión obliga a aprobarlo de uno en uno, que es la regla que el core ya tenía.

---

## Detalles que no son cosméticos

**El DSN no pasa por `argv`.** `--dsn` existe **solo para poder rechazarlo** con su explicación:
un DSN en la línea de comandos sobrevive en el historial del shell, en `ps` y en cualquier cosa
que registre comandos. Sale de `NEXUSMIND_SOURCE_DSN`.

**`source_ref` guarda `postgres://host/database`** — sin usuario y sin contraseña. Suficiente
para saber contra qué se corrió, insuficiente para volver a entrar. Hay un test que exige que no
aparezca ningún `@`.

**Solo lectura verificado, no confiado.** Se comprueba con `has_table_privilege(..., 'INSERT')`
sobre las tablas descubiertas, y la negativa nombra la tabla. Quien corrió esto pudo coger el rol
equivocado: es justo el error que la comprobación existe para atrapar.

**Un esquema de 200 tablas no son 200 candidatos.** Se agrupa por schema, y un schema demasiado
grande se parte por prefijo de nombre — la convención de facto cuando todo vive en `public`. El
test exige ≤10 candidatos para 200 tablas, y ≥2 para que no colapse en un bloque ilegible.

**Los valores aceptados de un `CHECK` viajan como conocimiento.** `status accepts only: draft |
sent | paid | void` es una regla de negocio, y es exactamente lo que un agente descubre rompiendo
algo.

---

## La épica queda completa

| Change | Fase |
|---|---|
| `-core` | `verify` ✅ |
| `-repo-docs` | `verify` ✅ |
| `-claude-memories` | `verify` ✅ |
| `-git-history` | `verify` ✅ |
| `-db-schemas` | `verify` ✅ |

---

## Pendiente

- [ ] El flake de `crypto::tests::with_key` — commit propio, sigue sin tocar.
- [ ] Primera pasada real de cada conector, acotada y con `--dry-run` primero.
- [ ] `PgSchemaReader` contra una base real: es lo único de la épica que no ha corrido nunca.
