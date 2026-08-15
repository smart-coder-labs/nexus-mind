# Tasks: Repo Docs Connector

## Process — strict TDD applies

`strict_tdd: true`, `tdd_scope: backend_and_admin`. Test first in every task.

Sin waiver. El conector es la primera cosa que va a leer material real de un cliente, y el orden
test-primero es lo que impide que "parece que clasifica bien" pase por verificación.

---

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 700–900 |
| 400-line budget risk | Alta |
| Chained PRs recommended | Ya no aplica — por decisión del owner va en el mismo PR que el core |
| Delivery strategy | commits separados dentro del PR #258, un comment por fase |

Solo backend. El admin no cambia: la cola de revisión ya muestra cualquier candidato,
venga del conector que venga.

---

## Fase 1: Mover el contrato a la librería

- [x] T-01: Test de que el trait sigue funcionando desde la librería
  - Files: `src/migration/mod.rs` (nuevo)
  - Scope: mover `Connector`, `SourceItem`, `CandidatePayload`, `ScanOptions` desde
    `bin/migrate_knowledge.rs`. El binario los re-exporta.
  - **Movimiento puro: cero cambio de comportamiento.** Los 11 tests del binario deben seguir
    verdes sin tocar uno solo. Si alguno necesita cambiar, el movimiento está mal.

- [x] T-02: Registrar `src/migration/` en `lib.rs`
  - Files: `src/lib.rs`
  - Gate: `cargo build` y los 11 tests del binario verdes.

---

## Fase 2: Escaneo por sección

- [x] T-03: `scan()` — secciones, no archivos
  - Files: `src/migration/repo_docs.rs` (nuevo)
  - Scope: `walk_docs` → `read_file` → `MarkdownChunker::chunk` → `Vec<SourceItem>`.
  - Tests: `scan_splits_a_document_into_sections`, `a_document_without_headings_yields_one_unit`.

- [x] T-04: Identidad determinista por sección
  - Scope: `repo-docs:{repo}:{path}#{anchor}:{sha16}`. `repo` es el nombre del directorio raíz,
    nunca una ruta absoluta. `anchor` usa el mismo `slugify` que `doc_queries` para que el
    candidato y el chunk indexado coincidan.
  - Tests: `identity_is_stable_across_rescans`,
    `editing_one_section_changes_only_its_identity`, `identity_never_contains_an_absolute_path`.

---

## Fase 3: Reglas de mapeo

- [x] T-05: `propose_destination` — las cinco reglas, en orden
  - Tests: `adr_path_proposes_a_decision_memory`,
    `unchecked_checklist_item_proposes_a_task`, `checked_items_propose_no_task`,
    `rule_shaped_section_proposes_a_convention`,
    `openspec_change_proposes_an_sdd_artifact_only_with_the_flag`,
    `plain_prose_falls_back_to_an_architecture_memory`.

- [x] T-06: Exclusiones por defecto y su reporte
  - Scope: `docs/marketing/**`, `docs/research/**`, `openspec/specs/**`,
    `openspec/changes/archive/**`. Contadas y reportadas, **nunca omitidas en silencio**.
  - Tests: `default_excludes_skip_marketing_research_and_living_specs`,
    `excluded_documents_are_reported_not_omitted`.

---

## Fase 4: Clasificación y fallback

- [x] T-07: `classify_prompt`
  - Scope: sección entera + ruta, pidiendo JSON del candidato. Tres instrucciones: propón no
    decidas; el extracto debe ser literal; puedes devolver `skip` con su razón.
  - Tests: `prompt_includes_the_section_its_path_and_asks_for_a_verbatim_excerpt`.

- [x] T-08: `fallback` — siempre `Some`
  - Scope: destino de las reglas, título del encabezado, contenido de la sección, extracto
    literal, confianza `None`.
  - Tests: `fallback_produces_a_candidate_for_every_unit`,
    `every_candidate_carries_a_verbatim_excerpt`, `fallback_reports_no_confidence`.

---

## Fase 5: Cableado y validación contra este repo

- [x] T-09: Registrar en `connector_for` y en la CLI
  - Files: `src/bin/migrate_knowledge.rs`
  - Scope: `--source repo-docs`, `--include-sdd`. El test que hoy afirma que `repo-docs` es
    rechazado **debe actualizarse**, no borrarse: ahora existe y los otros tres siguen sin existir.
  - Tests: `repo_docs_is_available`, `the_other_three_connectors_still_are_not`.

- [x] T-10: Dry-run con conteos reales
  - Tests: `scan_report_counts_documents_units_and_exclusions`, `scan_report_and_scan_agree`.
  - **Descubierto aquí**: el dry-run del core no cumplía su propio spec — reportaba unidades
    pero no documentos ni exclusiones. Se añadió `ScanReport` al trait.

- [x] T-11: **El test contra este repositorio**
  - Scope: correr el conector sobre `docs/` de este checkout. No afirma un número exacto —el
    corpus cambia—; afirma que `ENGINEERING_PROCESS.md` produce al menos una convención, que
    ningún candidato lleva ruta absoluta, y que todo candidato trae extracto.
  - Es lo que convierte "debería funcionar" en "funciona sobre 162 archivos reales".
  - Tests: `scanning_this_repository_produces_plausible_candidates`.

---

## Gates

| Gate | Comando |
|---|---|
| Backend tests | `cargo test --manifest-path apps/backend/Cargo.toml` |
| Backend lint | `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` |
| Formato | **NO `cargo fmt` a secas** — ver `knowledge-migration-core/apply-progress.md` §1 |

Revisión adversarial antes de cerrar la fase, igual que en el core. **Hecha: 2 hallazgos.**

---

## Riesgos

| Riesgo | Mitigación |
|---|---|
| **T-01 rompe el binario del core.** Mover el trait toca código recién mergeado. | Movimiento puro: los 11 tests existentes son la red y ninguno debe cambiar. **Verificado.** |
| **El heurístico `reads_like_a_rule` clasifica mal.** | Es explícitamente un ordenador de cola, no una decisión. El LLM lo desvía y el humano decide. El test contra este repo mide si acierta en el caso que importa. |
| **El corpus de este repo cambia y rompe T-11.** | El test afirma propiedades, no conteos: "al menos una convención desde `ENGINEERING_PROCESS.md`", no "exactamente 47 candidatos". |
| **Coste de la primera pasada real.** 162 documentos → 3377 secciones. | Medido: **~514 000 tokens**. `--dry-run` primero, `--max-tokens` después, y acotar por subdirectorio en la primera pasada. |
| **3377 candidatos ahogan la revisión humana.** | El veredicto `skip` del clasificador es la mitigación que más pesa; sin él son 3377 decisiones humanas. |
