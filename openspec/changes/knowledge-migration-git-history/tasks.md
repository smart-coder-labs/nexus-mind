# Tasks: Git History Connector

## Process — strict TDD applies

Sin waiver. Desbloqueado por la decisión de NDA `db359a75`: clasificar código de cliente con
`claude -p` está permitido.

---

- [x] T-01: Lectura de historia local (`read_commits`)
  - Separadores de registro `\u{1e}`/`\u{1f}`: ningún mensaje de commit los contiene, así que
    un cuerpo con líneas en blanco no puede confundirse con un límite de registro.
  - Tests: `a_repository_without_a_remote_still_scans`, `a_non_repository_path_is_refused_clearly`.

- [x] T-02: Prefiltro determinista, **antes** de cualquier modelo
  - Chores, bots, merges sin cuerpo, cuerpos por debajo de 40 caracteres útiles (ignorando
    trailers `Co-Authored-By` y `Signed-off-by`, que no son explicación).
  - Tests: `chores_bots_and_bodyless_commits_are_filtered_without_a_model`,
    `a_commit_with_a_real_explanation_survives`, `the_filter_reports_what_it_removed`.

- [x] T-03: Identidad por SHA
  - `git:{repo}:{sha}` o `git:{repo}:pr:{n}`. **Sin hash de contenido**, a diferencia de los
    otros conectores: un commit no cambia.
  - Tests: `identity_is_the_commit_sha_and_is_stable`, `identity_never_contains_an_absolute_path`.

- [x] T-04: Escaneo incremental (`--since-commit`)
  - Tests: `scanning_is_incremental_from_a_given_commit`.

- [x] T-05: Agrupación por PR
  - `git rev-list <merge>^1..<merge>^2` marca lo absorbido. Todo local, sin red.
  - Tests: `a_merged_group_produces_one_unit_not_one_per_commit`.

- [x] T-06: Mapeo y marca de reversión
  - Tests: `fix_with_a_cause_proposes_a_bugfix_memory`, `a_revert_is_marked_as_such`.

- [x] T-07: La fecha viaja con el candidato
  - Tests: `every_candidate_carries_the_date_of_the_work`.

- [x] T-08: Redacción de credenciales en mensajes de commit
  - Tests: `credentials_in_commit_messages_are_redacted`.

- [x] T-09: Prompt y dry-run
  - Tests: `the_prompt_asks_for_the_why_and_allows_skipping`,
    `dry_run_reports_examined_surviving_and_estimated_tokens`.

- [x] T-10: **Contra la historia real de este repo**
  - Tests: `scanning_this_repository_filters_most_of_its_history`. Afirma propiedades:
    que el filtro quita la mayoría, que ninguna identidad lleva ruta absoluta, y —la
    calibración— que los commits de la propia épica sobreviven. Un filtro que los tirara
    estaría mal ajustado.

---

## Fuera de esta entrega, y dicho

**Enriquecimiento con GitHub** (comentarios y reviews de PR). El proposal lo listaba como
opcional. No se implementa porque exige credenciales y red —no se puede testear en CI sin
mockear justo lo que se quiere probar— y porque el asunto y el cuerpo del merge ya traen el
título y la descripción del PR en la mayoría de los casos.

`github_connections` existe con token cifrado por cliente desde v58, así que el día que se haga
hay dónde apoyarse.
