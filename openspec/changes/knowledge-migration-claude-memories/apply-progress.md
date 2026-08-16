# Apply Progress — Claude Code Connector

> **Change**: `knowledge-migration-claude-memories`
> **Branch**: `sdd/knowledge-migration-core` (mismo PR)
> **Date**: 2026-08-15

---

## Estado: ✅ 13 tareas completas

| Gate | Resultado |
|---|---|
| `cargo test` | **1183 lib + 46 integración + 12 runner — 0 fallos** |
| `cargo clippy -- -D warnings` | limpio |

Tests netos: **+38** (1145 → 1183).

---

## El dry-run contra un `~/.claude` real

```
documents=525  units=525  bytes=2 872 884  estimated_tokens≈718 221
excluded 4055 document(s):
   3240  republishing it is a licensing problem
    809  session transcripts are out of scope
      6  an index over the other files, not a source
```

**3240 assets de terceros excluidos.** Ese es el número que justifica que la exclusión sea no
negociable: sin ella, este conector habría propuesto republicar como harnesses de u2s más de
tres mil skills y agents descargados de marketplaces — incluido el plugin del propio NexusMind.

**809 transcripciones de sesión**, fuera de alcance por decisión y no por olvido.

525 candidatos y ~718 000 tokens. Más caro que `repo-docs` en tokens por unidad, porque los
archivos son más largos; acotar por `--host-scope` de proyecto es lo razonable.

---

## Dos defectos reales encontrados por los tests

### 🔴 La redacción era O(n²) — 72 segundos por 64 KB

Cada carácter reconstruía el resto del texto como `String` nueva y le hacía `to_lowercase()`.
Un test de un asset de 64 KB tardó **72 segundos**; sobre un `~/.claude` real habría sido
inutilizable.

Reescrita comparando solo los primeros `needle.len()` bytes: **72 s → 0,03 s**. Un test fija la
propiedad — 1 MB en menos de 5 segundos, cuando una implementación cuadrática tardaría horas.

Lo encontró un test que buscaba otra cosa. Ese es el argumento para escribir el test del caso
grande aunque parezca redundante.

### 🔴 `redact_emails` reescribía los espacios en blanco de todo lo que tocaba

Usaba `split_whitespace().join(" ")`. Eso colapsa runs de espacios, pierde el salto final y
**destruye la indentación de cualquier script o bloque de código**. Un hook con `    echo` dejaba
de funcionar, y su `sha256` describía el texto destrozado en vez del revisado.

Reescrita para reemplazar solo los tramos que son direcciones, dejando cada otro byte donde
estaba. Tests: `redaction_preserves_every_byte_it_does_not_replace` sobre scripts, bloques de
código y espacios repetidos.

Lo destapó el test de linealidad al fallar por **un byte** de diferencia. Sin esa aserción exacta
—`out.len() == big.len()`— habría pasado inadvertido hasta que alguien instalara un hook roto.

---

## Decisiones

**El tipo local manda.** `metadata.type` decide el destino; el LLM titula y resume, no
reclasifica. `user` → memory `preference` con `scope=personal`, nunca convención.

**Los hashes se calculan del contenido redactado.** Hashear el original produciría un
`component_integrity_mismatch` y, peor, describiría algo distinto de lo que viaja.

**`parse_frontmatter` devuelve el cuerpo como slice del original**, nunca recompuesto desde sus
líneas — la misma clase de bug que tenía `redact_emails`, y aquí importa porque los bloques de
código de una memoria son parte de lo que la hace valiosa.

**Semántica de `documents` unificada**: archivos que produjeron al menos una unidad, igual que en
`repo-docs`. Al principio contaba archivos leídos y reportaba 3588 frente a 525 unidades, dos
números que no se podían comparar entre conectores.

---

## Pendiente

- [ ] Los dos conectores restantes (`git-history`, `db-schemas`) siguen bloqueados por la
      pregunta del NDA.
- [ ] El flake de `crypto::tests::with_key`, todavía sin arreglar (commit propio).
