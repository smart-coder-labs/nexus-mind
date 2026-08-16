# Design — Git History Connector

> **Change**: `knowledge-migration-git-history`
> **Status**: proposed
> **Owner**: backend + tooling
> **Date**: 2026-08-15
> **Depends on**: `knowledge-migration-core`, y la decisión de NDA `db359a75`

---

## 1. Qué aporta que no aporte el índice de código

`POST /v1/code/index` responde **qué hace** el código hoy. No responde **por qué es así**, y ese
porqué es justo lo que un agente pierde y un humano tarda meses en reconstruir.

Está en la historia: el commit que dice "revert: vuelve a X porque Y rompía Z", el merge cuyo
cuerpo discute dos alternativas antes de elegir una. `src/indexer/walker.rs` recorre el árbol de
trabajo actual — no lee commits.

Este repo tiene **452 commits**. La mayoría son ruido.

---

## 2. El prefiltro determinista es el diseño, no una optimización

Llamar al modelo por cada commit de un repo con años de historia es caro y, sobre todo,
**inútil**: `chore(deps): bump serde` no contiene conocimiento. El filtro corre **antes** de
cualquier clasificación y cuesta cero tokens.

Descarta:

| Regla | Por qué |
|---|---|
| Prefijos `chore:`, `style:`, `ci:`, `build:`, `wip`, `bump`, `Merge branch` sin cuerpo | mantenimiento mecánico |
| Autor bot (`[bot]`, `dependabot`, `renovate`, `github-actions`) | no hay decisión humana detrás |
| Cuerpo por debajo de 40 caracteres útiles | el asunto solo no explica nada |
| `Merge pull request #N` **sin** cuerpo | el merge vacío no dice qué se decidió |

**Se reporta lo descartado, con su razón.** Un run que dice "escaneé 12 commits" sobre 452 sin
decir qué pasó con los otros 440 es un run que miente — la misma regla que en `repo-docs`.

**Un merge CON cuerpo sobrevive**: es exactamente donde está la discusión de alternativas.

---

## 3. La unidad de decisión es el PR, no el commit

Un PR de 30 commits es **una** decisión, no treinta. Sin agrupar, esa decisión aparece treinta
veces en la cola de revisión y el revisor abandona — que es el modo de fallo que más nos
preocupa desde que `repo-docs` midió 3377 candidatos.

**Cómo se agrupa sin red ni credenciales**: un merge commit cuyo asunto nombra el PR
(`Merge pull request #250 from …`, o `Título (#252)`) define un grupo. Los commits que ese merge
trajo se marcan como absorbidos y no producen unidad propia.

Se resuelve con `git log --merges` y `git rev-list <merge>^1..<merge>^2` — todo local.

Un commit que no pertenece a ningún merge conocido produce su propia unidad. Un repo con
rebase-merge (sin merge commits) degrada a una unidad por commit, y eso está bien: el filtro ya
quitó el ruido.

---

## 4. Enriquecimiento con GitHub: fuera de esta entrega, y dicho

El proposal lo listaba como **opcional**. No se implementa aquí, por dos razones concretas:

1. Exige credenciales y red, así que **no se puede testear en CI** sin mocks que probarían el
   mock.
2. El valor es incremental: el asunto y el cuerpo del merge ya traen el título y la descripción
   del PR en la mayoría de los casos.

Queda como trabajo posterior anotado, no como algo que se olvidó. `github_connections` ya existe
con token cifrado por cliente desde v58, así que el día que se haga hay dónde apoyarse.

---

## 5. Identidad

```
git:{repo}:{commit_sha}          # commit suelto
git:{repo}:pr:{number}           # grupo de un PR
```

El SHA es inmutable por construcción: la idempotencia sale gratis y `migration_provenance`
bloquea el re-commit sin lógica adicional. `repo` es el nombre del directorio, nunca una ruta
absoluta.

**Ojo**: a diferencia de `repo-docs`, la identidad **no** lleva hash de contenido. Un commit no
cambia; si se reescribe la historia, el SHA cambia y es un commit distinto. Es correcto y hay
que decirlo, porque rompe la simetría con los otros conectores.

---

## 6. Mapeo

| Forma del commit | Destino |
|---|---|
| `fix:`/`bugfix` con causa en el cuerpo | memory `bugfix` |
| `revert:` | memory `decision`, marcado como reversión |
| `feat:` con cuerpo que discute alternativas | memory `decision` |
| Tag o entrada de `CHANGELOG` | memory `project` |
| Resto de lo que sobrevive al filtro | memory `architecture` |

**La reversión se marca explícitamente.** Un commit que explica por qué se eligió X y que fue
revertido tres meses después sigue siendo conocimiento, pero conocimiento *histórico*. El
candidato lo dice; detectar la semántica completa del revert queda fuera de alcance.

---

## 7. La fecha viaja con el candidato

Toda decisión de commit lleva su fecha en el `destination_hint`. Un revisor tiene que poder
pesar si algo de hace dos años sigue vigente, y esa es información que solo el conector tiene.

Es el mismo problema que `repo-docs` anotó con la documentación obsoleta, y la misma respuesta:
la máquina no adivina qué envejeció, el humano decide con el dato delante.

---

## 8. Redacción

Igual que en `claude-memories`, y reusando el mismo `super::redact`. Los mensajes de commit
llevan tokens con más frecuencia de la que a nadie le gustaría admitir.

---

## 9. Tests — orden TDD

1. `a_repository_without_a_remote_still_scans`.
2. `a_non_repository_path_is_refused_clearly`.
3. `chores_bots_and_bodyless_merges_are_filtered_without_a_model`.
4. `a_merge_with_a_body_survives_the_filter`.
5. `the_filter_reports_what_it_removed`.
6. `identity_is_the_commit_sha_and_is_stable`.
7. `identity_never_contains_an_absolute_path`.
8. `scanning_is_incremental_from_a_given_commit`.
9. `a_merged_group_produces_one_unit_not_one_per_commit`.
10. `fix_with_a_cause_proposes_a_bugfix_memory`.
11. `a_revert_is_marked_as_such`.
12. `every_candidate_carries_the_date_of_the_work`.
13. `credentials_in_commit_messages_are_redacted`.
14. `dry_run_reports_examined_surviving_and_estimated_tokens`.
15. **`scanning_this_repository_filters_most_of_its_452_commits`** — contra este mismo repo.
    Afirma propiedades, no conteos: que sobrevive una minoría, que ninguna identidad lleva ruta
    absoluta, y que los commits de la épica `knowledge-migration` sobreviven al filtro (tienen
    cuerpo largo y explican decisiones — si el filtro los tirara, estaría mal calibrado).
