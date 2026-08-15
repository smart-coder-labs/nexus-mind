# Proposal — Conector: historia de git

> **Change**: `knowledge-migration-git-history`
> **Status**: proposed
> **Owner**: backend + tooling
> **Date**: 2026-08-15
> **Depends on**: `knowledge-migration-core`

---

## 1. Intent

### Problema

La indexación de código que ya existe (`POST /v1/code/index`, `src/indexer/`) responde
**qué hace** el código hoy. No responde **por qué es así**, y ese porqué es justo lo que un
agente pierde y un humano tarda meses en reconstruir.

Ese porqué está en la historia: el commit que dice "revert: vuelve a X porque Y rompía Z", la
descripción del PR que discute dos alternativas antes de elegir una, el `CHANGELOG` que fecha
un cambio de contrato. `src/indexer/walker.rs` recorre el árbol de trabajo actual: no lee
commits, ni PRs, ni tags, ni blame.

### Success looks like

- Preguntar "¿por qué el backend no llama a ningún LLM?" devuelve el razonamiento, no solo el
  `Cargo.toml` sin la dependencia.
- Los merges de PR con descripción sustancial producen memories `decision` o `architecture`
  con enlace al PR.
- Los commits de fix con causa raíz en el cuerpo producen memories `bugfix`.
- El ruido —`chore:`, `wip`, bumps de dependencias, merges vacíos— **no produce nada**.
- Re-correr tras 50 commits nuevos procesa 50 commits, no toda la historia.

---

## 2. Scope

### In scope

1. **Lectura de historia local** — `git log` con cuerpo completo, autor, fecha, archivos
   tocados y tags alcanzables. Rango incremental (`--since-commit`), persistido en el run.
2. **Enriquecimiento con GitHub** (opcional) — títulos, cuerpos y comentarios de PR vía la
   API, reusando `github_connections` (que tras v58 ya admite **una conexión por cliente**, con
   token cifrado por `token_cipher`). Sin credenciales, el conector funciona igual con solo la
   historia local: la conexión enriquece, no habilita.
3. **Prefiltro determinista antes del LLM** — descarta por patrón (`chore:`, `wip`,
   `bump`, merges sin cuerpo, commits de bot, cuerpos por debajo de N caracteres). Barato,
   auditable, y evita gastar tokens en ruido.
4. **Clasificación con `claude -p`** de lo que sobrevive el prefiltro, agrupando por PR cuando
   se conoce: un PR de 30 commits es **una** decisión, no treinta.
5. **`CHANGELOG.md` y tags** como fuente de hitos → memories `project`.

### Out of scope

- **Blame por línea** y atribución de autoría a nivel de símbolo: pertenece al grafo de código.
- Issues de GitHub → tasks. El tracker de tareas ya existe y su import es otro problema.
- Repos que no sean git.
- Reescribir el índice de código. Este conector produce **conocimiento**, no chunks de código.

---

## 3. Approach

```
migrate-knowledge --source git-history --repo . --client acme --project acme-billing \
                  [--since-commit <sha> | --since 2026-01-01] [--with-github] [--dry-run]
```

`source_identity` = `git:{repo}:{commit_sha}` — o `git:{repo}:pr:{number}` cuando el candidato
agrupa un PR.

El SHA de commit es el identificador estable ideal: inmutable por construcción. La
idempotencia sale gratis; `migration_provenance` bloquea el re-commit del mismo origen sin
lógica adicional.

### Rationale

- **Prefiltro determinista primero, LLM después.** En un repo con 3.000 commits, la mayoría son
  ruido. Filtrar con reglas cuesta cero tokens y hace el gasto predecible.
- **Agrupar por PR.** La unidad de decisión es el PR, no el commit. Sin agrupar, una decisión
  aparece treinta veces en la cola de revisión y el revisor abandona.
- **GitHub opcional.** Amarrar el conector a credenciales lo haría inútil en el caso más común:
  un repo de cliente clonado sin acceso a su organización de GitHub.

---

## 4. Risks & open questions

| Riesgo | Mitigación |
|---|---|
| **Volumen.** Un monorepo con años de historia son decenas de miles de commits. | `--since` obligatorio si no hay `--since-commit`; el prefiltro corta antes del LLM; `--dry-run` reporta cuántos commits sobreviven y el coste estimado antes de gastar. |
| **Decisiones revertidas migradas como vigentes.** Un commit explica por qué se eligió X, y tres meses después se revirtió. | El candidato incluye la fecha del commit y si hay reverts posteriores que toquen los mismos archivos. La UI lo señala. Detectar la semántica del revert automáticamente queda fuera de alcance. |
| **Secretos en mensajes de commit.** Ocurre. | Escaneo de secretos antes de subir el candidato, reusando el vocabulario `secret_scan_status` que el manifiesto de harness ya define. Un candidato con posible secreto se bloquea, no se stagea. |
| **Historia de cliente bajo NDA saliendo hacia la API de Anthropic** al clasificar con `claude -p`. | **Decisión de negocio, no técnica**: hay que declararla explícitamente por cliente antes de correr el conector. Se registra en `migration_runs` como attestation del operador. Ver pregunta abierta. |

**Pregunta abierta (bloquea el `spec.md` de este change, no el del core):** ¿existe algún
cliente de u2s cuyo NDA prohíba enviar su código o mensajes de commit a un tercero para
procesamiento? Si lo hay, este conector necesita un modo sin LLM —solo prefiltro determinista
y revisión manual— o queda vetado para ese cliente. Es la misma pregunta 1 que quedó abierta en
el memo del company brain y sigue sin responder.
