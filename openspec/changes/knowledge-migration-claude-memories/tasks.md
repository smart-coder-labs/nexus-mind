# Tasks: Claude Code Connector

## Process — strict TDD applies

Sin waiver. Este conector lee la máquina de una persona: memorias privadas, tokens, rutas de
home. El orden test-primero es lo que impide que "parece que redacta bien" pase por verificación.

---

## Fase 1: Redacción (precondición de todo lo demás)

- [x] T-01: `src/migration/redact.rs`
  - Rutas de home, tokens, connection strings, correos. Reporte por categoría.
  - Tests: 15, incluido `redacted_content_survives_the_real_manifest_validator`, que prueba que
    el contenido redactado pasa el validador **y que el original no**.

- [x] T-02: La redacción debe ser lineal
  - Tests: `redaction_is_linear_not_quadratic`, `redaction_preserves_every_byte_it_does_not_replace`.

---

## Fase 2: Formato local

- [x] T-03: `parse_frontmatter` — devuelve el cuerpo como slice, nunca recompuesto.
- [x] T-04: `destination_for_type` — el tipo declarado manda; `user` se queda personal.
- [x] T-05: `wikilinks` — en orden, sin duplicados.

---

## Fase 3: Harness

- [x] T-06: `AssetKind` — los seis formatos y su `component_kind`.
- [x] T-07: `build_manifest` — hashes del contenido **redactado**, tope de 64 KB, rutas relativas.
- [x] T-08: **`every_emitted_manifest_passes_the_real_validator`** — un manifiesto por formato
  contra `validate_typed_harness_manifest`. Sin él los candidatos solo fallarían en commit-time.

---

## Fase 4: Exclusiones y configuración

- [x] T-09: `plugins/cache/**` y transcripciones, **no overrideable**.
  - Tests: `the_cache_exclusion_cannot_be_overridden` pasa opciones que intentan reintroducirlas.
- [x] T-10: `settings.json`/`.mcp.json` → `harness_config_review`, nunca harness version.

---

## Fase 5: Cableado y medición

- [x] T-11: `connector_for` + `--host-scope`.
- [x] T-12: `scan_report` con exclusiones, semántica de `documents` unificada con `repo-docs`.
- [x] T-13: Dry-run real contra un `~/.claude` de verdad.

---

## Gates

| Gate | Comando |
|---|---|
| Backend tests | `cargo test --manifest-path apps/backend/Cargo.toml` |
| Backend lint | `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` |
| Formato | **NO `cargo fmt` a secas** |

---

## Riesgo que no se cierra con código

**Material de un cliente A en la memoria local de alguien que también trabaja para B.** El
conector no sabe de qué cliente habla un párrafo. La mitigación es el `client_id` obligatorio del
run, el acotado por proyecto y la ruta de origen visible en cada candidato — y decirlo en la UI
en vez de fingir que el sistema lo cubre.
