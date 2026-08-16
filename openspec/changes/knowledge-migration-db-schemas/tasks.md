# Tasks: Database Schema Connector

Último de la épica. Desbloqueado por la ADR `db359a75`.

- [x] T-01: `SchemaReader` como trait — separa la lógica testeable del I/O.
- [x] T-02: Las cuatro puertas de `--include-data` como función pura
  (`SamplingPolicy::authorize`), devolviendo **cuál** falta.
  - Tests: una por puerta, más `all_four_conditions_together_permit_sampling`.
- [x] T-03: `ensure_read_only` — se niega nombrando la tabla escribible.
- [x] T-04: Agrupación por área (schema), con partición por prefijo sobre 25 tablas.
  - Tests: `tables_are_grouped_by_schema_not_emitted_one_by_one`,
    `a_two_hundred_table_schema_stays_reviewable`.
- [x] T-05: Prosa con valores aceptados, comportamiento al borrar, unicidad y comentarios.
- [x] T-06: Redacción de las muestras **antes** de que entren en un candidato.
- [x] T-07: Políticas RLS en modo Supabase.
- [x] T-08: Identidad `pg:{db}:{area}:{ddl_sha16}` — cambia solo el área que cambió.
- [x] T-09: `PgSchemaReader` (sqlx) + `safe_reference_for` y `parse_check_in` testeados.
- [x] T-10: CLI — el DSN **no** se acepta por `argv`; sale de `NEXUSMIND_SOURCE_DSN`.
  - Tests: `a_dsn_passed_as_an_argument_is_refused`, `db_schema_without_a_dsn_says_where_to_put_it`.

---

## Lo que este change no cierra, y está dicho

**`PgSchemaReader` no tiene test automatizado.** Es la traducción de `information_schema` a
structs; su modo de fallo es un nombre de columna mal escrito y lo caza el primer run real. Un
test de integración marcado `#[ignore]` que nadie corre parece cobertura y no lo es.

Sus dos funciones puras —`safe_reference_for` y `parse_check_in`— sí están cubiertas, porque ahí
sí hay lógica que puede estar mal.
