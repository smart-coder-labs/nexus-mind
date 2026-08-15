# Apply Progress — Git History Connector

> **Change**: `knowledge-migration-git-history`
> **Branch**: `sdd/knowledge-migration-core` (mismo PR)
> **Date**: 2026-08-15
> **Desbloqueado por**: ADR `db359a75`

---

## Estado: ✅ 10 tareas completas

| Gate | Resultado |
|---|---|
| `cargo test` | **1199 lib + 46 integración + 12 runner — 0 fallos** |
| `cargo clippy -- -D warnings` | limpio |

Tests netos: **+16** (1183 → 1199).

---

## El prefiltro medido contra 452 commits reales

```
documents=202  units=202  bytes=131 609  estimated_tokens≈32 902
excluded 250:
   111  nothing to learn                      (cuerpo por debajo del umbral)
    82  part of a merged group                (representados por su merge)
    42  no human decision behind it           (bots)
    15  no decision behind it                 (chores)
```

**El prefiltro quita el 55% antes de gastar un token.** Y el resultado es **~33 000 tokens**
frente a los ~514 000 de `repo-docs`: quince veces más barato, porque un mensaje de commit es
corto y el filtro es agresivo.

Los 82 commits absorbidos por su merge son la parte que más importa para la revisión humana: sin
agrupar, esas decisiones aparecerían 82 veces más en la cola. Es el modo de fallo que `repo-docs`
midió con 3377 candidatos.

---

## Decisiones

**Separadores de registro `\u{1e}`/`\u{1f}` en el `--format` de git.** Un cuerpo de commit tiene
líneas en blanco; usar `\n\n` como límite de registro habría partido mensajes por la mitad. Esos
dos caracteres no aparecen en ningún mensaje.

**Los trailers no cuentan como cuerpo.** `Co-Authored-By:` y `Signed-off-by:` engordan un mensaje
sin explicar nada, así que el umbral de 40 caracteres los ignora. Sin eso, cualquier commit con
trailer pasaría el filtro.

**La identidad no lleva hash de contenido**, a diferencia de los otros conectores. Un commit no
cambia; si se reescribe la historia, el SHA cambia y es otro commit. La asimetría es correcta y
está dicha en el código, porque rompe la simetría con `repo-docs` y `claude-memories`.

**La agrupación por PR es local.** `git rev-list <merge>^1..<merge>^2` da lo que trajo un merge
sin tocar la red ni credenciales. Un repo con rebase-merge degrada a una unidad por commit, y
está bien: el filtro ya quitó el ruido.

**La reversión se marca, no se interpreta.** Un commit que explica por qué se eligió X y que fue
revertido sigue siendo conocimiento, pero histórico. El candidato lo dice; detectar la semántica
completa del revert queda fuera de alcance.

---

## Fuera de esta entrega, y dicho

**Enriquecimiento con GitHub.** Exige credenciales y red, así que no se puede testear en CI sin
mockear lo que se quiere probar; y el asunto y cuerpo del merge ya traen el título y la
descripción del PR en la mayoría de los casos. Anotado como trabajo posterior, no olvidado.

---

## Pendiente

- [ ] `db-schemas`, el último del épica.
- [ ] El flake de `crypto::tests::with_key`, todavía sin arreglar.
