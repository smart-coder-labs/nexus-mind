# QUICKSTART — Docker, con modelo de clientes

Levanta NexusMind completo en Docker y valida que el modelo de clientes (`u2s-client-model`) funciona: aislamiento entre clientes, herencia de convenciones y cifrado de credenciales.

Tiempo: ~10 minutos, de los cuales ~5 son la primera compilación de Rust.

---

## 0. Requisitos

- Docker con Compose v2 (`docker compose`, no `docker-compose`)
- `curl` y `jq`
- ~3 GB libres (imagen + volumen + modelo de embeddings)

---

## 1. Configurar el entorno

El backend lee `apps/backend/.env`. Dos variables importan para este arranque:

```bash
cd nexusmind
cp apps/backend/.env.example apps/backend/.env   # si aún no existe

# Clave de cifrado de credenciales — 64 hex (32 bytes)
echo "NEXUSMIND_TOKEN_ENCRYPTION_KEY=$(openssl rand -hex 32)" >> apps/backend/.env

# Búsqueda semántica: apagada por defecto
echo "NEXUSMIND_EMBED_ENABLED=true" >> apps/backend/.env
```

> **`NEXUSMIND_TOKEN_ENCRYPTION_KEY` es dependencia de arranque.** La migración v58 cifra los tokens de GitHub almacenados. Si la base ya tiene conexiones y falta la clave, **la migración aborta** en lugar de copiar credenciales en texto plano, y el backend no levanta. Una base nueva no tiene nada que cifrar y arranca sin ella.
>
> Una vez existen tokens, la clave debe ser estable: rotarla los vuelve indescifrables y hay que re-autorizar las conexiones.

> **`NEXUSMIND_EMBED_ENABLED` falla en silencio si falta.** Sin ella el backend registra `Embedding service disabled` y la búsqueda degrada a texto plano **sin error**. Es el fallo que no se nota hasta que los resultados llevan semanas siendo mediocres — por eso el paso 4 lo verifica explícitamente.

---

## 2. Levantar

```bash
docker compose up -d --build
docker compose ps
```

Tres servicios:

| Servicio | Puerto | Qué es |
|---|---|---|
| `backend` | 8080 | API Rust + SQLite |
| `admin` | 3000 | Panel de administración |
| `backoffice` | 3001 | Backoffice |

Espera a que el healthcheck pase (hasta ~30 s):

```bash
until curl -sf localhost:8080/v1/health > /dev/null; do sleep 2; done && echo "backend arriba"
```

### Comprobar que la migración v58 aplicó

```bash
docker compose exec backend sh -c \
  'apk add --no-cache sqlite 2>/dev/null || apt-get install -y sqlite3 2>/dev/null >&2; \
   sqlite3 /data/nexusmind.db "PRAGMA user_version;"'
```

Debe imprimir **58**. Si imprime menos, revisa los logs — lo más probable es que la migración abortara por falta de clave de cifrado:

```bash
docker compose logs backend | grep -i "migration\|encryption"
```

---

## 3. Sembrar datos de demo

```bash
docker compose exec backend /app/nexusmind-seed /data/nexusmind.db
```

Imprime las claves de API de demo. Guarda la de admin:

```bash
export ADMIN_KEY=nm_demo_acme_admin
export API=http://localhost:8080
```

---

## 4. Validar embeddings e indexación de código

**Embeddings.** Lo primero es confirmar que el servicio arrancó de verdad:

```bash
docker compose logs backend | grep -i embedding
```

Buscas `Embedding service initialized (nomic-embed-text-v1.5)`. Si en su lugar dice `Embedding service disabled` o `Embedding service unavailable`, la búsqueda semántica **no** está activa aunque todo lo demás funcione.

> **La primera inicialización descarga ~274 MB** del modelo a la caché de fastembed. El `Dockerfile` no lo hornea ni persiste esa caché, así que **cada contenedor nuevo vuelve a descargarlo**. Para desarrollo es tolerable; para despliegue real hay que hornearlo en la fase de build o montar la caché en un volumen. Está anotado como trabajo del módulo de terraform.

Prueba de extremo a extremo — guarda una memoria y búscala por significado, no por palabra exacta:

```bash
curl -s -X POST $API/v1/memory/store \
  -H "Authorization: Bearer $ADMIN_KEY" -H 'Content-Type: application/json' \
  -d '{"content":"Elegimos SQLite en vez de Postgres por simplicidad de despliegue","title":"Elección de base de datos","type":"decision","project":"default","tool":"curl"}' | jq -r '.id'

curl -s -X POST $API/v1/memory/search \
  -H "Authorization: Bearer $ADMIN_KEY" -H 'Content-Type: application/json' \
  -d '{"query":"por qué no usamos un motor relacional cliente-servidor","limit":3}' | jq '.[].title'
```

Debe devolver «Elección de base de datos» aunque la consulta no comparta ninguna palabra clave con el contenido. Si no aparece, la búsqueda está cayendo a texto plano.

**Indexación de código.** Es por proyecto y se dispara por API:

```bash
curl -s $API/v1/code/projects -H "Authorization: Bearer $ADMIN_KEY" | jq
```

---

## 5. Validar el modelo de clientes

Esta es la parte nueva. Cuatro comprobaciones.

### 5.1 Crear dos clientes y sus proyectos

```bash
CLI_A=$(curl -s -X POST $API/v1/clients -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' -d '{"name":"Client A","slug":"client-a"}' | jq -r '.id')
CLI_B=$(curl -s -X POST $API/v1/clients -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' -d '{"name":"Client B","slug":"client-b"}' | jq -r '.id')

curl -s -X POST $API/v1/projects -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' -d "{\\"name\\":\\"a-billing\\",\\"client_id\\":\\"$CLI_A\\"}" | jq -r '.name'
curl -s -X POST $API/v1/projects -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' -d "{\\"name\\":\\"b-api\\",\\"client_id\\":\\"$CLI_B\\"}" | jq -r '.name'

# Sin client_id = proyecto interno de u2s. NULL significa "interno", no "sin asignar".
curl -s -X POST $API/v1/projects -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' -d '{"name":"internal-tooling"}' | jq -r '.name'
```

### 5.2 El slug es inmutable, y único por organización

```bash
# 409 — slug duplicado
curl -s -o /dev/null -w '%{http_code}\n' -X POST $API/v1/clients \
  -H "Authorization: Bearer $ADMIN_KEY" -H 'Content-Type: application/json' \
  -d '{"name":"Otro","slug":"client-a"}'

# 400 — el slug no se puede cambiar
curl -s -o /dev/null -w '%{http_code}\n' -X PATCH $API/v1/clients/$CLI_A \
  -H "Authorization: Bearer $ADMIN_KEY" -H 'Content-Type: application/json' \
  -d '{"slug":"nuevo-slug"}'
```

Esperado: `409` y luego `400`. El `PATCH` **rechaza** el campo en vez de ignorarlo — descartar en silencio un campo que quien llama creía estar cambiando es peor que un error.

### 5.3 Borrar un cliente con proyectos se rechaza

```bash
curl -s -o /dev/null -w '%{http_code}\n' -X DELETE $API/v1/clients/$CLI_A \
  -H "Authorization: Bearer $ADMIN_KEY"
```

Esperado: **422**. Dar de baja un cliente es una transición de estado (`status = "offboarded"`), nunca una cascada que se llevaría por delante su historial.

### 5.4 Aislamiento entre clientes — la comprobación que importa

Crea un usuario, hazlo miembro sólo del cliente A, y comprueba qué ve:

```bash
USER_B=$(curl -s -X POST $API/v1/users/invite -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' -d '{"email":"dev@u2s.io","name":"Dev","role":"member"}')
DEV_KEY=$(echo "$USER_B" | jq -r '.api_key // .key')
DEV_ID=$(echo "$USER_B" | jq -r '.user.id // .id')

curl -s -X POST $API/v1/clients/$CLI_A/members -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' -d "{\\"user_id\\":\\"$DEV_ID\\",\\"role\\":\\"member\\"}"

# Ve sólo el cliente A
curl -s $API/v1/clients -H "Authorization: Bearer $DEV_KEY" | jq -r '.[].slug'

# El cliente B responde 404, NO 403
curl -s -o /dev/null -w '%{http_code}\n' -X PATCH $API/v1/clients/$CLI_B \
  -H "Authorization: Bearer $DEV_KEY" -H 'Content-Type: application/json' -d '{"name":"x"}'
```

Esperado: la lista muestra sólo `client-a`, y el acceso al cliente B devuelve **404**.

> **Que sea 404 y no 403 es deliberado.** Un 403 confirmaría que el recurso existe, que es justo lo que un cliente competidor no debe poder averiguar. Por la misma razón, un cliente **inexistente** también responde 404: ausente y prohibido son indistinguibles desde fuera.

Cada intento denegado deja rastro:

```bash
curl -s "$API/v1/audit?action=resource.hidden_access_denied" \
  -H "Authorization: Bearer $ADMIN_KEY" | jq '.[0]'
```

---

## 6. Validar la herencia de convenciones

La regla es **org → cliente → proyecto**, y cada nivel **suma**: uno más específico nunca reemplaza al más amplio. Si pudiera, los estándares propios de u2s dejarían de ser exigibles.

```bash
PID_A=$(curl -s $API/v1/projects -H "Authorization: Bearer $ADMIN_KEY" | jq -r '.[] | select(.name=="a-billing") | .id')

# Nivel organización
curl -s -X POST $API/v1/conventions -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"title":"Commits convencionales","content":"Usar conventional commits"}' > /dev/null

# Nivel proyecto
curl -s -X POST $API/v1/conventions -H "Authorization: Bearer $ADMIN_KEY" \
  -H 'Content-Type: application/json' \
  -d "{\\"title\\":\\"Facturación en centavos\\",\\"content\\":\\"Nunca floats para dinero\\",\\"project_id\\":\\"$PID_A\\"}" > /dev/null

# El contexto del proyecto debe traer AMBAS
curl -s $API/v1/context/a-billing -H "Authorization: Bearer $ADMIN_KEY" | jq -r '.conventions[].title'
```

Esperado: **las dos** convenciones. Si sólo aparece una, la herencia está reemplazando en vez de sumar.

> El nivel de cliente se asigna hoy por SQL directo (`UPDATE conventions SET client_id = …`): la API de convenciones aún no expone `client_id` en el body. Está anotado como pendiente.

Y el proyecto interno **no** debe ver convenciones de ningún cliente:

```bash
curl -s $API/v1/context/internal-tooling -H "Authorization: Bearer $ADMIN_KEY" | jq -r '.conventions[].title'
```

---

## 7. Validar el cifrado de credenciales

Ninguna credencial debe ser legible en el archivo de base de datos:

```bash
docker compose exec backend sh -c \
  'sqlite3 /data/nexusmind.db "SELECT access_token FROM github_connections;"'
```

Si hay filas, deben ser hex opaco (nonce + ciphertext), nunca algo que empiece por `ghp_` o `gho_`.

Y la clave primaria admite ahora una conexión **por cliente**:

```bash
docker compose exec backend sh -c \
  'sqlite3 /data/nexusmind.db ".schema github_connections" | grep "PRIMARY KEY"'
```

Esperado: `PRIMARY KEY (org_id, client_id, github_login)`. La antigua era `PRIMARY KEY (org_id)` — una sola cuenta de GitHub por organización, inservible para una consultora donde cada cliente tiene su propia org.

---

## 8. Panel de administración

<http://localhost:3000>, entrando con `nm_demo_acme_admin`.

---

## Apagar

```bash
docker compose down          # conserva el volumen de datos
docker compose down -v       # borra también la base de datos
```

---

## Problemas frecuentes

| Síntoma | Causa |
|---|---|
| `user_version` < 58 | La migración abortó. Revisa `docker compose logs backend` — casi siempre falta `NEXUSMIND_TOKEN_ENCRYPTION_KEY` con conexiones ya almacenadas. |
| La búsqueda devuelve sólo coincidencias literales | `NEXUSMIND_EMBED_ENABLED` no es `true`, o el modelo no cargó. Degrada en silencio: confírmalo en los logs. |
| El backend tarda ~1 min en el primer arranque | Descarga del modelo de embeddings (274 MB). El `Dockerfile` no lo hornea, así que ocurre en cada contenedor nuevo. |
| 403 donde esperabas 404 | Falta el permiso (`client:read` / `client:write`), que es distinto de que el recurso esté oculto. Permisos y visibilidad son ejes separados. |
| Un admin ve clientes de los que no es miembro | No debería. Sólo `super_user` tiene visibilidad org-wide; `admin` es privilegiado para permisos pero sigue acotado por membresía en lecturas. |

---

## Lo que este quickstart todavía no cubre

- **`client_id` en el body de convenciones y políticas.** El nivel de cliente existe en el esquema y en la resolución, pero se asigna por SQL directo.
- **Endpoint HTTP de resolución de proyectos.** `report_project_resolution` está implementado y probado, sin ruta.
- **Módulo de terraform para AWS.** Es un change de SDD aparte; incluye hornear el modelo de embeddings y sacar la clave de cifrado de Parameter Store.
