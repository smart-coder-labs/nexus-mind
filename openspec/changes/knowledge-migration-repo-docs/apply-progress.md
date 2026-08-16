# Apply Progress — Repo Docs Connector

> **Change**: `knowledge-migration-repo-docs`
> **Branch**: `sdd/knowledge-migration-core` (mismo PR que el core, por decisión del owner)
> **Date**: 2026-08-15

---

## Estado: ✅ las 11 tareas completas

| Gate | Resultado |
|---|---|
| `cargo test` | **1145 lib + 46 integración + 12 runner — 0 fallos** (2 corridas consecutivas limpias) |
| `cargo clippy -- -D warnings` | limpio |

Tests netos: **+24** (1121 → 1145). El binario pasó de 11 a 12.

---

## El número que importa: el dry-run real

Corrido contra este mismo repositorio, sin gastar un token:

```
dry run — source=repo-docs documents=162 units=3377 bytes=2057239 estimated_tokens≈514309
excluded 26 document(s)
```

**162 documentos** — el proposal citaba 161 y este change añadió el suyo; ahora es medido, no estimado.
**3377 unidades** y **~514 000 tokens** para una pasada de clasificación completa.

Dos consecuencias que conviene mirar antes del primer run de verdad:

1. **Medio millón de tokens no es un gasto que se hace por accidente.** `--max-tokens` existe
   y ahora hay una cifra concreta con la que fijarlo. Correr por subdirectorios
   (`--include docs/adr`) es lo razonable para la primera pasada.
2. **3377 candidatos convierten la revisión humana en el cuello de botella**, que es
   exactamente el riesgo que el proposal del core anotó. Las tres mitigaciones ya existen:
   el orden por confianza en la UI, la aprobación en lote para procedencia verificada, y —la
   que más pesa— el veredicto `skip` que el prompt permite devolver al clasificador. Sin ese
   `skip`, 3377 unidades son 3377 decisiones humanas y nadie hace ese trabajo.

---

## Desviaciones respecto al plan

### 1. El dry-run no cumplía su propio spec

El requisito "Cost Is Estimable Before It Is Spent" pide **documentos, unidades y estimación**.
El runner del core solo reportaba unidades. Se añadió `ScanReport` al trait `Connector`, con
implementación por defecto honesta (`documents: 0` para un conector que no lo rastrea) y una
real en `repo-docs`.

Detectado corriendo el dry-run de verdad, no leyendo el código. Es la diferencia entre un test
que pasa y una herramienta que sirve.

### 2. El movimiento del contrato fue puro, y se verificó como tal

Mover `Connector`, `SourceItem`, `CandidatePayload` y `ScanOptions` de
`bin/migrate_knowledge.rs` a `src/migration/mod.rs` dejó **los 11 tests del binario verdes sin
tocar ninguno**. Ese era el criterio de T-01.

### 3. El test que menos parece un test es el que más vale

`scanning_this_repository_produces_plausible_candidates` corre el conector sobre `docs/` de
este checkout. No afirma conteos —el corpus cambia— sino propiedades: que
`ENGINEERING_PROCESS.md` produce al menos una convención, que ningún candidato lleva ruta
absoluta, y que todos traen extracto.

Es lo que convierte "el heurístico debería acertar" en "el heurístico acierta en el caso para
el que se escribió", medido contra 161 archivos reales en vez de tres fixtures.

### 4. Los marcadores de regla son bilingües a propósito

`RULE_MARKERS` incluye "siempre/nunca/debe" junto a "always/never/must". La documentación de
este repo mezcla español e inglés, y un conector que solo reconociera uno se dejaría la mitad.

---

## Revisión adversarial — 2 hallazgos

### 🟠 Mayor — una sección con doce casillas producía UNA tarea

`scan()` emitía una unidad por sección, así que un roadmap con doce `- [ ]` se colapsaba en un
solo candidato titulado con la primera casilla. Un revisor aprobándolo habría creído que el
roadmap quedó capturado, y **once tareas se perdían en silencio**.

Ahora una sección con N casillas sin marcar emite **N unidades**, cada una con su propia
identidad (`{anchor}-task{idx}`) y su propio extracto literal. Las casillas marcadas siguen sin
producir nada.

Excepción deliberada: los checklists dentro de un ADR o de un `openspec/changes/**` **no** se
convierten en tareas — son la lista de seguimiento de esa decisión, no el backlog del equipo.
Tests: `each_unchecked_item_becomes_its_own_task_unit`, `task_units_have_distinct_identities`,
`checklists_inside_adrs_do_not_become_tasks`.

### 🟡 Menor (mío, corregido) — mi test de BYOM mutaba el entorno del proceso

`backend_pipeline_succeeds_with_no_model_credentials` llamaba a `std::env::remove_var`. El
entorno es global al proceso y la suite corre en paralelo, así que un test que borra una
variable puede romper otro a media ejecución. La afirmación BYOM no lo necesitaba: el fixture ya
se construye sin servicio de embeddings y el crate no tiene cliente de LLM. Eliminado.

### Reportado, no corregido — flake preexistente en `crypto`

`crypto::tests::with_key` hace `set_var` y luego `remove_var` sobre **la misma** variable, y
varios tests de ese módulo lo llaman en paralelo: uno borra la clave mientras otro la está
usando. `decrypt_rejects_tampered_blob` falló una vez en una corrida completa y pasa 3/3
aislado.

**No lo toco**: es código ajeno a este change y arreglarlo (un mutex alrededor de `with_key`, o
`#[serial]`) merece su propio commit para que el diff diga lo que hace. Queda anotado porque va
a volver a fallar en CI y conviene que la causa esté escrita.

---

## Pendiente

- [x] Revisión adversarial — 2 hallazgos, corregidos.
- [ ] Arreglar el flake de `crypto::tests::with_key` — commit propio.
- [ ] Primera pasada real acotada (`--include docs/adr`) antes de una completa.
- [ ] Los tres conectores restantes siguen en `propose`.
