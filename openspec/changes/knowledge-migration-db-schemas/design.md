# Design — Database Schema Connector

> **Change**: `knowledge-migration-db-schemas`
> **Status**: proposed
> **Owner**: backend + tooling
> **Date**: 2026-08-15
> **Depends on**: `knowledge-migration-core`, ADR `db359a75`

Último conector de la épica. Asume leídos `proposal.md` y el delta spec.

---

## 1. El problema de testear esto, y cómo se resuelve

Los otros tres conectores leen el filesystem: un `TempDir` y ya hay un test. Este habla con
Postgres, y **no hay Postgres en CI**.

La salida no es "tests de integración que alguien correrá a mano". Es separar las dos mitades:

```
  ┌─────────────────────────┐        ┌──────────────────────────┐
  │  SchemaReader (trait)   │◄───────│  DbSchemaConnector       │
  │  tables, columns,       │        │  agrupa por área,        │
  │  constraints, comments, │        │  aplica las 4 puertas,   │
  │  policies, sample_rows  │        │  redacta, redacta prosa  │
  └─────────────────────────┘        └──────────────────────────┘
        ▲                 ▲
        │                 │
  PgSchemaReader      FakeSchemaReader
  (sqlx, I/O real)    (en memoria, tests)
```

**Toda la lógica que puede estar mal —las cuatro puertas, la agrupación, la redacción, el
render a prosa— vive del lado testeable.** El lado de I/O es una traducción de SQL a structs,
donde el error posible es un nombre de columna mal escrito, y eso lo caza el primer run real.

Es también mejor diseño con independencia del test: el día que haya que soportar MySQL, se
implementa el trait otra vez.

---

## 2. Las cuatro puertas de `--include-data`

La ADR `db359a75` decidió: schema-only por defecto, datos por decisión explícita del operador.
Cuatro condiciones **acumulativas** — falta una, se rechaza la muestra entera:

| Condición | Por qué esta y no otra |
|---|---|
| **Allowlist explícita de tablas** | Nunca `--all`. El operador escribe qué tablas, una por una. Es la diferencia entre una decisión y un descuido. |
| **`LIMIT` acotado, muestreo determinista** | `ORDER BY` estable, no `RANDOM()`: dos runs sobre datos sin cambiar deben dar la misma muestra, o la idempotencia del pipeline se rompe. |
| **Redacción antes de salir del proceso** | Correos, tokens, connection strings — el mismo `super::redact` de los otros conectores. Ocurre **en local**, antes de que la muestra sea parte de un candidato. |
| **Attestation del operador** | Queda en `migration_runs.attestation`. Un run con datos siempre dice quién lo autorizó y cuándo. `provenance_kind='client_attested'`, que v56 ya define. |

**La verificación es una función pura** (`SamplingPolicy::authorize`) que devuelve el motivo
concreto de rechazo. Eso la hace testeable sin base de datos y hace que el mensaje de error
nombre la condición que falta, en vez de un "no autorizado" que obliga a adivinar.

---

## 3. Solo lectura, verificado y no confiado

El conector se niega a arrancar si el rol puede escribir. No es paranoia: una migración no tiene
ninguna razón para poder escribir en la base de un cliente, y si puede, es un incidente
esperando ocurrir.

Se comprueba con `pg_has_role` / `has_table_privilege` sobre las tablas descubiertas. Si alguna
acepta `INSERT`, se rechaza nombrando la tabla.

**No se confía en que el operador haya usado el rol correcto.** Ese es justo el error que la
comprobación existe para atrapar.

---

## 4. La credencial no pasa por `argv`

`--dsn` **no existe** como flag con valor. El DSN se lee de una variable de entorno o se pide
por prompt.

Un DSN en la línea de comandos queda en el historial del shell, en `ps`, y en los logs de
cualquier cosa que registre comandos. `import_sdd` ya usa `hide_env_values` por la misma razón,
y su comentario explica que sin él clap imprime el valor resuelto en `--help`.

`migration_runs.source_ref` guarda `postgres://<host>/<database>` — host y base, sin usuario ni
contraseña. Suficiente para saber contra qué se corrió; insuficiente para volver a entrar.

---

## 5. Agrupar por área, no por tabla

Un esquema de 200 tablas produciría 200 candidatos, y la revisión humana —que ya es el cuello
de botella medido en `repo-docs`— sería imposible.

**El área es el schema de Postgres** (`public`, `auth`, `billing`…), que es como los equipos
separan dominios en la práctica. Un candidato por área describe sus tablas, sus relaciones y las
reglas que impone.

Si un schema tiene más de N tablas para caber en un candidato razonable, se parte por prefijo de
nombre (`invoice_*`, `order_*`), que es la convención de facto cuando alguien mete todo en
`public`.

---

## 6. Qué prosa se genera

El DDL en crudo no es conocimiento; es una tabla que alguien tiene que leer. El candidato lleva
prosa que responde a lo que un agente necesita saber:

- qué tablas hay y qué representa cada una (usando `COMMENT ON` cuando existe, que es la
  documentación que el DBA ya escribió y nadie lee);
- **qué valores acepta cada columna enumerada** — un `CHECK (status IN (…))` es una regla de
  negocio, y es lo que un agente descubre rompiendo algo;
- **qué relaciones existen y qué pasa al borrar** — `ON DELETE RESTRICT` está ahí por una razón;
- qué es único, qué está indexado.

---

## 7. Identidad

```
pg:{database}:{schema}:{ddl_sha16}
```

El `ddl_sha` es del DDL normalizado del área. Una migración del cliente que cambia una tabla
vuelve a proponer **esa área** y solo esa — el mismo comportamiento que `repo-docs` tiene por
sección.

`database` es el nombre de la base, nunca el host con credenciales.

---

## 8. Tests — orden TDD

Todos contra `FakeSchemaReader`, sin Postgres.

1. `default_options_sample_no_rows` — y el reporte lo dice.
2. `a_writable_role_is_refused_naming_the_table`.
3. `a_read_only_role_proceeds`.
4. `sampling_without_an_allowlist_is_refused` / `..._without_a_limit_` / `..._without_redaction_` /
   `..._without_an_attestation_` — una por puerta, cada una nombrando la que falta.
5. `all_four_conditions_together_permit_sampling`.
6. `a_table_outside_the_allowlist_is_never_sampled`.
7. `sampled_values_are_redacted_before_they_reach_a_candidate`.
8. `tables_are_grouped_by_schema_not_emitted_one_by_one`.
9. `a_two_hundred_table_schema_stays_reviewable`.
10. `check_constraints_appear_as_accepted_values`.
11. `restricted_foreign_keys_report_their_delete_behaviour`.
12. `rls_policies_are_described_in_supabase_mode`.
13. `identity_changes_only_for_the_area_whose_ddl_changed`.
14. `a_dsn_passed_as_an_argument_is_refused`.
15. `the_run_reference_carries_no_credentials`.

---

## 9. Lo que este change no cierra

**`PgSchemaReader` no tiene test automatizado.** Es la traducción de `information_schema` a
structs; su modo de fallo es un nombre de columna mal escrito, y lo caza el primer run real
contra una base de verdad. Decirlo aquí es preferible a un test de integración marcado
`#[ignore]` que nadie corre y que da una falsa sensación de cobertura.

**El primer run contra un cliente sigue siendo un primer run.** `--dry-run` reporta áreas,
tablas y coste antes de gastar nada.
