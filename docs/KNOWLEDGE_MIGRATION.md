# Knowledge Migration — Operator Guide

> **Documento**: KNOWLEDGE_MIGRATION.md
> **Versión**: 1.0
> **Fecha**: Agosto 2026
> **Propósito**: cómo usar `migrate-knowledge` para importar conocimiento previo a NexusMind

Migra a NexusMind el conocimiento que un equipo ya tenía antes de usarlo: documentación,
memorias de agente, historia de git y esquemas de base de datos. Es una herramienta de
onboarding, no una migración interna de datos.

---

## 1. Los tres invariantes

Antes de cualquier comando, lo que la herramienta garantiza — y lo que no.

**Nada entra sin que un humano lo apruebe.** Ni una memoria, ni una convención, ni una
herramienta ejecutable. La confianza que devuelve el clasificador ordena la cola de revisión;
nunca autoriza nada.

**Correrlo dos veces no duplica.** Cada unidad lleva una identidad determinista derivada de su
procedencia, y una restricción `UNIQUE` en la base de datos —no una comprobación aplicativa que
alguien pueda olvidar— impide commitear el mismo origen dos veces al mismo destino.

**El backend nunca llama a un modelo.** La inferencia corre en tu máquina con `claude -p`; el
backend solo recibe candidatos ya clasificados. Puedes desplegarlo sin credenciales de modelo y
sigue funcionando (principio BYOM, `docs/ENGINEERING_PROCESS.md`).

---

## 2. Antes de correr nada

### 2.1 Comprueba a dónde apuntas

`--api-url` toma su valor por defecto de `NEXUSMIND_BASE_URL`, que en muchas máquinas apunta a
**producción**. Un migrador lanzado sin mirar publica allí.

```bash
echo $NEXUSMIND_BASE_URL
```

Si no es el backend que quieres, fíjalo explícitamente en cada comando:

```bash
--api-url http://localhost:8080
```

### 2.2 Siempre `--dry-run` primero

Escanea entero, no clasifica nada, no publica nada, y te dice cuántos documentos, cuántas
unidades y cuántos tokens costaría la pasada real. Es la diferencia entre decidir y descubrir.

### 2.2bis El clasificador corre aislado de tu configuración

`claude -p` se invoca desde un directorio neutro y con `--system-prompt` +
`--exclude-dynamic-system-prompt-sections`. Sin eso hereda tu `CLAUDE.md` y los
skills del repo, y contesta a *esas* instrucciones: en una corrida real
respondió «Nada que persistir en este turno…» en vez de devolver un candidato.
Como `--output-format json` entrega el **último** mensaje, un comentario final
sustituye al JSON y la unidad cae en fallback — habiéndose pagado igual.

Requiere un `claude` reciente. Si el tuyo no acepta esas banderas, el runner
falla nombrándolas en vez de degradar en silencio.

### 2.2ter `--bulk`: una llamada por lote en vez de por unidad

Hay dos modos con LLM. El de por defecto hace **una llamada por unidad**; `--bulk`
agrupa (20 por lote, o menos si el contenido pesa) y manda el lote entero.

La pasada determinista corre **antes** en ambos modos, y en `--bulk` su resultado
viaja como borrador dentro del prompt. Eso da un suelo: si el modelo trunca,
desordena o no contesta un lote, cada unidad conserva su candidato determinista.
Un lote fallido cuesta tiempo, nunca trabajo.

Medido sobre 24 unidades de este repo:

| | por unidad (estimado) | `--bulk` (medido) |
|---|---|---|
| tokens | ~816.000 | **26.136** |
| tiempo | ~13 min | **4,7 min** |
| clasificadas | — | 24 de 24, 0 fallbacks |

La diferencia es de dónde sale el coste: cada llamada arrastra ~34.000 tokens de
contexto de sistema *antes* de leer el prompt, así que 24 llamadas pagan ese
contexto 24 veces. Repetir las instrucciones dentro del lote sale mucho más
barato que repetir el contexto.

Cuándo **no** usarlo: cuando quieras la máxima atención por documento. El modelo
reparte su esfuerzo entre los 20 elementos del lote.

### 2.3 `--no-llm` existe y a veces es la respuesta correcta

Cada conector tiene un camino determinista completo. Sin modelo los títulos son peores y no hay
puntuación de confianza, pero **no se pierde ni una unidad**. Úsalo cuando:

- quieras ver la forma de los candidatos sin gastar tokens;
- no haya red o el CLI falle;
- un NDA de cliente prohíba enviar su material a un tercero.

### 2.4 Pon un techo

`--max-tokens N` aborta el run al superarse y **deja intacto lo ya stageado**. Un run abortado no
es un run perdido: el staging vive en el backend y se reanuda.

---

### 2.5 El binario no está en el `PATH`, y corre en tu máquina

`migrate-knowledge` se compila en `apps/backend/target/{debug,release}/`. Invócalo por ruta, o
añade un alias:

```bash
alias migrate-knowledge=/ruta/al/repo/apps/backend/target/debug/migrate-knowledge
```

**No lo corras dentro del contenedor.** No está en la imagen, y no podría hacer su trabajo desde
ahí: necesita tu repositorio, tu `~/.claude` y tu acceso de red a la base del cliente. El
contenedor no tiene ninguna de las tres. El runner vive en el host y habla con el backend por
HTTP; esa frontera es el diseño, no un detalle de empaquetado.

---

## 2bis. El TUI, si prefieres no memorizar flags

`apps/migrator-tui` es un front-end interactivo sobre el mismo runner. No es una
implementación paralela: lanza `migrate-knowledge` y muestra el comando
equivalente en pantalla todo el tiempo, así que lo que hagas ahí lo puedes
scriptear mañana.

```bash
cd apps/backend    && cargo build --release --bin migrate-knowledge
cd apps/migrator-tui && cargo run --release
```

Cubre el ciclo completo — conexión, fuente, opciones, dry-run, corrida con
progreso en vivo, cola de revisión y commit — y mantiene las mismas garantías
que el resto de esta guía:

- La URL por defecto es `localhost`, **no** `NEXUSMIND_BASE_URL`. Si esa
  variable apunta a otro sitio te lo dice y no la usa (§2.1).
- El DSN se lee de `NEXUSMIND_SOURCE_DSN`. No hay campo donde escribirlo y no se
  dibuja nunca (§3.4).
- Los cuatro requisitos de `--include-data` están visibles aunque el muestreo
  esté apagado, para que sepas el precio antes de encenderlo. Apagarlo borra los
  cuatro.
- Los candidatos de tipo harness y los que llevan atestación de cliente quedan
  fuera de la aprobación por lote y aparecen marcados (§4.1).

En la pantalla de corrida se dibuja una mascota atada al estado real de la
corrida (escaneando, stageando, commiteado). **Es decoración y nada más**: todo
lo que representa ya está en pantalla como contador, barra o texto. Si tu
terminal no puede dibujarla —sin truecolor, sin Unicode, o con la ventana
pequeña— la interfaz queda idéntica sin ella, sin hueco ni aviso, y no pierdes
ninguna información. Se apaga con `--no-mascot`, con
`NEXUSMIND_MIGRATE_MASCOT=0`, o con la tecla `m`. Requisitos y detalle en
`apps/migrator-tui/README.md`.

En la pantalla de corrida hay dos paneles: **Agents** (cada intercambio con el
clasificador — qué se pidió, qué respondió, tokens y duración) y **Logs**. `e`
expande cualquiera a pantalla completa. Sirve para algo concreto: un
`classified 0 / fallback 249` solo dice que las respuestas no se pudieron usar;
el panel muestra la respuesta.

Una corrida se puede revisar después: el TUI lista las corridas del backend y
abre la cola de cualquiera, no solo la que acaba de crear.

El modo que lo hace posible sirve solo también: `migrate-knowledge --json` emite
NDJSON —un evento por línea, con flush— en vez de prosa. Útil para `jq`, para CI
o para cualquier supervisor que necesite progreso en lugar de silencio.

## 3. Los cuatro conectores

| Conector | Qué lee | Medido en este repo |
|---|---|---|
| `repo-docs` | Markdown del repositorio, por secciones | 162 docs → 3377 unidades, ~514k tokens |
| `claude-memories` | `~/.claude`, `CLAUDE.md`, skills, hooks | 525 unidades, ~718k tokens, 4055 excluidos |
| `git-history` | commits y merges, tras un prefiltro | 452 commits → 202 sobreviven, ~33k tokens |
| `db-schema` | catálogo de Postgres/Supabase | según el esquema |

Las cifras son de un dry-run real, no estimaciones de diseño. Sirven para calibrar `--max-tokens`.

### 3.1 `repo-docs`

La unidad es la **sección**, no el archivo: un documento con principios y tablas de stack
produce convenciones y contexto por separado, en vez de obligarte a elegir.

```bash
migrate-knowledge --source repo-docs --path /ruta/al/repo --dry-run

migrate-knowledge --source repo-docs --path /ruta/al/repo \
  --api-url http://localhost:8080 --api-key "$KEY" --client "$CLIENT_ID" \
  --include docs/adr              # acota la primera pasada
```

`--include` y `--exclude` filtran por fragmento de ruta y aceptan varios separados por comas.
Sobre este repo, `--include docs/adr` baja la pasada de ~514k tokens a ~15k: es la diferencia
entre probar y comprometerse.

**Excluye por defecto** marketing, research, `openspec/specs/` (la especificación viva la mantiene
el flujo de archivado) y `openspec/changes/archive/`. Todo lo excluido se reporta con su razón.

`--include-sdd` deja que `openspec/changes/**` produzca artefactos SDD. Apagado por defecto
porque `import-sdd` ya los backfillea en este repo, y dos caminos al mismo destino es como
aparecen los duplicados. Enciéndelo al migrar un repo ajeno.

### 3.2 `claude-memories`

```bash
migrate-knowledge --source claude-memories --path ~/.claude --host-scope global --dry-run

migrate-knowledge --source claude-memories --path ~/.claude --host-scope global \
  --api-url http://localhost:8080 --api-key "$KEY" --client "$CLIENT_ID"
```

`--host-scope` es `global` o el slug de un proyecto. **Nunca el nombre de la máquina ni del
usuario**: acabaría dentro de una clave primaria.

Qué hace con cada cosa:

| Origen | Destino |
|---|---|
| `memory/*.md` con frontmatter | memory del tipo declarado |
| `type: user` | memory `preference`, **scope personal** — nunca convención |
| `CLAUDE.md`, `AGENTS.md`, `.cursor/rules` | convention |
| `agents/`, `skills/`, `commands/`, `hooks/`, `output-styles/`, temas | harness tipado |
| `settings.json`, `.mcp.json` | config review redactada, **nunca** harness |
| `plugins/cache/**` | **excluido, no negociable** |
| `*.jsonl` (transcripciones) | fuera de alcance |

**La redacción es precondición, no higiene.** El validador de manifiestos rechaza cualquier
contenido con `/users/`, `bearer `, `ghp_` o una clave de OpenAI. Sin redactar, este conector no
produce un solo harness válido. Los hashes se calculan del contenido **ya redactado**.

**Los assets de terceros no se republican.** Un `~/.claude` real tiene miles de skills bajadas de
marketplaces; republicarlas como harnesses del equipo es un problema de licencia, no una feature.
La exclusión **no acepta override**.

### 3.3 `git-history`

Aporta el *porqué* que el índice de código no puede tener: el razonamiento del mensaje de commit
y de la descripción del PR.

```bash
migrate-knowledge --source git-history --path /ruta/al/repo --dry-run

migrate-knowledge --source git-history --path /ruta/al/repo \
  --api-url http://localhost:8080 --api-key "$KEY" --client "$CLIENT_ID"

# segunda pasada, solo lo nuevo:
migrate-knowledge --source git-history --path /ruta/al/repo --since-commit <sha>
```

Un **prefiltro determinista** corre antes de cualquier modelo y cuesta cero tokens. Descarta
chores, bots, merges sin cuerpo y mensajes sin sustancia — y lo reporta con su razón.

**La unidad de decisión es el PR, no el commit.** Un PR de treinta commits es una decisión; sin
agrupar aparecería treinta veces en la cola y el revisor abandonaría.

### 3.4 `db-schema`

```bash
export NEXUSMIND_SOURCE_DSN='postgres://readonly_user:pw@host:5432/db'

migrate-knowledge --source db-schema --dry-run
migrate-knowledge --source db-schema --supabase \
  --api-url http://localhost:8080 --api-key "$KEY" --client "$CLIENT_ID"
```

**El DSN no se pasa por línea de comandos.** `--dsn` existe solo para rechazarlo con su
explicación: un DSN en `argv` sobrevive en el historial del shell, en `ps` y en cualquier cosa
que registre comandos. Va en `NEXUSMIND_SOURCE_DSN`.

**El rol debe ser de solo lectura.** El conector se niega a arrancar si puede escribir, y nombra
la tabla. Una migración no tiene ninguna razón para poder escribir en la base de un cliente.

**Por defecto no lee ni una fila** de negocio. Solo catálogo: tablas, columnas, tipos, claves,
CHECKs, índices, vistas y `COMMENT ON`.

#### Leer datos: las cuatro puertas

Leer filas exige **las cuatro condiciones a la vez**. Falta una y se rechaza, nombrando cuál:

```bash
migrate-knowledge --source db-schema \
  --include-data \
  --tables invoices,order_status \      # allowlist explícita, nunca --all
  --sample-limit 10 \                   # muestreo acotado y determinista
  --redact-pii \                        # redacción en local, antes de salir del proceso
  --attest "autorizado por <persona> el <fecha>"
```

Un candidato con muestra sale como `client_attested`, así que la UI de revisión obliga a
aprobarlo **de uno en uno**.

> Decisión registrada (ADR `db359a75`): clasificar código de cliente con un modelo está
> permitido; leer sus datos queda a decisión explícita del operador. Un flag que se olvida
> activar es un fallo; un flag que hay que activar a propósito es una decisión.

---

## 4. Revisar y commitear

Todo lo anterior **stagea**. Nada ha entrado todavía.

En el panel de administración → **Migration**:

1. Elige el run. La cola se ordena por confianza, la más alta arriba.
2. **Inspect** en un candidato: verás el contenido propuesto, el **extracto literal del origen** y
   el destino. No deberías tener que abrir el archivo original para juzgarlo.
3. **Approve** o **Reject**. Aprobar en lote está disponible, pero se **bloquea** si la selección
   incluye algún candidato con procedencia `client_attested`: esos descansan en la palabra de
   alguien y se leen de uno en uno.
4. **Commit** los aprobados.

Si otra persona actúa sobre el mismo candidato mientras tu cola está abierta, verás un conflicto
de versión y la cola se recarga. Es deliberado: hay que mirar de nuevo antes de decidir.

### 4.1 Aprobar un harness no lo instala

Son dos preguntas distintas con dos responsables distintos:

- **la aprobación de migración** decide *"esto pasa a ser herramienta del equipo"*;
- **la aprobación de instalación** decide *"dejo que esto corra en mi máquina"*, y la da quien lo
  recibe, después.

### 4.2 `pending_index` no es un error

El commit reporta cuántos artefactos quedaron persistidos **sin vectorizar**. Ocurre cuando no hay
servicio de embeddings configurado o cuando falla. El artefacto existe y es correcto; solo no es
buscable por similitud todavía, y la reconciliación lo recoge después.

`GET /v1/docs/index-status` reporta el pendiente en cualquier momento.

---

## 5. Un stack local para probar

El runner corre **en tu máquina**, no en el contenedor: necesita tu repo, tu `~/.claude` y la red
del cliente. El backend puede estar donde sea.

```bash
# 1. Backend + admin, volumen limpio y sin datos de demo
docker compose down -v && docker compose up -d --build

# 2. El binario del runner (debug basta para probar)
cargo build --manifest-path apps/backend/Cargo.toml --bin migrate-knowledge
```

`apps/backend/.env` debe existir — el compose lo exige. Mínimo:

```bash
SUPERUSER_KEY=<algo-largo>
NEXUSMIND_TOKEN_ENCRYPTION_KEY=<64 hex>   # sin esto run_v58 falla al arrancar, a propósito
NEXUSMIND_EMBED_ENABLED=false             # los embeddings tardan en inicializar
```

### 5.1 Bootstrap sin datos de seed

```bash
# organización + usuario admin + API key
curl -s -X POST localhost:8080/v1/orgs \
  -H "Authorization: Bearer $SUPERUSER_KEY" -H 'Content-Type: application/json' \
  -d '{"org_name":"u2s","org_slug":"u2s","admin_email":"tu@correo","admin_name":"Tu Nombre"}'
```

La respuesta trae `api_key`. Para entrar al panel hace falta contraseña: sin SMTP configurado, el
token de alta se escribe en el log del contenedor.

```bash
docker logs nexusmind-backend-1 2>&1 | grep "password setup token" | tail -1

curl -s -X POST localhost:8080/v1/admin/auth/set-password -H 'Content-Type: application/json' \
  -d '{"token":"<token>","password":"<contraseña>"}'
```

### 5.2 Quien gestiona clientes tiene que ser `super_user`

**Crear un cliente no te hace miembro de él**, y las lecturas están acotadas por membresía. El
admin recién creado no verá el cliente que acaba de crear, y tampoco podrá añadirse como miembro
—esa ruta también exige verlo—.

```bash
curl -s -X PATCH localhost:8080/v1/users/<user_id>/role \
  -H "Authorization: Bearer $API_KEY" -H 'Content-Type: application/json' \
  -d '{"role":"super_user"}'
```

Es coherente con el modelo —el `super_user` gestiona clientes— pero el flujo de bootstrap no lo
deja claro, y es la primera piedra con la que se tropieza.

### 5.3 Una base de origen para `db-schema`

Cualquier Postgres sirve. Necesita un rol de solo lectura, o el conector se niega:

```sql
CREATE ROLE migration_reader LOGIN PASSWORD '...';
GRANT CONNECT ON DATABASE <db> TO migration_reader;
GRANT USAGE  ON SCHEMA public TO migration_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO migration_reader;
```

---

## 6. Cosas con las que se tropieza

| Síntoma | Causa |
|---|---|
| Publicó en producción | `NEXUSMIND_BASE_URL` traía la URL de producción. Pasa `--api-url` siempre. |
| `writable_role: … can write to X` | El rol de Postgres no es de solo lectura. Correcto: arréglalo, no lo saltes. |
| `dsn_in_argv` | Usa `NEXUSMIND_SOURCE_DSN`. |
| `prepared statement "sqlx_s_N" already exists` | El DSN apunta a un pooler en modo `transaction` (PgBouncer, o el pooler de Supabase en el 6543). El conector ya no usa sentencias preparadas, así que si vuelve a aparecer, usa la conexión directa (5432 en Supabase) o pon el pooler en `session`. |
| `sampling_refused: --attest is required` | Falta una de las cuatro puertas. El mensaje dice cuál. |
| El cliente no aparece | Membresía. Ver §5.2. |
| `pending_index` > 0 tras commitear | Sin servicio de embeddings. No es un fallo; ver §4.2. |
| El clasificador falló y siguió | Diseñado: cae al fallback determinista y no aborta el run. |
| `run_v60 aborted: … holds N rows` | La migración se niega a recrear tablas con datos. Míralos antes de nada. |

---

## 7. Qué no hace

- **No lee transcripciones de sesión.** Volumen enorme, señal baja, y el material más sensible de
  la máquina.
- **No republica assets de terceros**, y esa exclusión no se puede desactivar.
- **No instala nada.** Un harness migrado espera su propia aprobación de instalación.
- **No enriquece con GitHub** (comentarios y reviews de PR). El asunto y el cuerpo del merge ya
  traen título y descripción en la mayoría de los casos.
- **No migra `openspec/specs/`**: esa es la especificación viva y la mantiene el flujo de
  archivado, no un importador.
