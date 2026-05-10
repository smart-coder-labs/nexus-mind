# NexusMind — Authentication & Authorization Specification

> **Documento**: AUTH_SPEC.md
> **Versión**: 1.1
> **Fecha**: Mayo 2026
> **Propósito**: Sistema de identidad, autenticación, autorización y aislamiento de memoria para el control plane. El mecanismo que responde a: ¿quién se conecta? ¿desde dónde? ¿qué puede hacer? ¿qué memoria puede leer?

---

## 1. Principios de Diseño

1. **Identity-first** — Toda interacción con NexusMind está ligada a una identidad verificable. No hay acceso anónimo.
2. **Zero Trust** — Cada request se autentica y autoriza individualmente. No hay sesiones largas ni confianza implícita.
3. **Granularidad por herramienta** — Un usuario no tiene un permiso global. Tiene permisos por herramienta, por modelo, por proyecto.
4. **Roles 100% customizables** — No hay roles fijos. Cada empresa define sus propios roles con los permisos exactos que necesita. Los roles predefinidos son solo templates de arranque.
5. **Aislamiento de memoria** — La memoria es del proyecto/equipo, no del individuo. Pero el acceso está controlado por roles.
5. **Audit obligatorio** — Cada decisión de auth se registra. Cada acceso a memoria se registra. Quién, cuándo, desde dónde, qué vió.
6. **BYO IdP** — No construimos otro identity provider. Integramos con los existentes (SSO, SAML, OIDC, SCIM).

---

## 2. Arquitectura de Identidad

```
                    ┌──────────────────────────────────┐
                    │   EXTERNAL IDENTITY PROVIDERS     │
                    │  ┌────────┐ ┌────────┐ ┌──────┐ │
                    │  │ Okta   │ │ Azure  │ │Google│ │
                    │  │        │ │ AD     │ │      │ │
                    │  └───┬────┘ └───┬────┘ └──┬───┘ │
                    │      │          │         │      │
                    └──────┼──────────┼─────────┼──────┘
                           │          │         │
                    ┌──────┴──────────┴─────────┴──────┐
                    │   NEXUSMIND IDENTITY LAYER        │
                    │                                   │
                    │  ┌─────────────────────────────┐  │
                    │  │  Auth Gateway               │  │
                    │  │  (OIDC RP + SAML SP +       │  │
                    │  │   API Key Verifier +         │  │
                    │  │   MFA Evaluator)            │  │
                    │  └─────────────┬───────────────┘  │
                    │                │                  │
                    │  ┌─────────────┴───────────────┐  │
                    │  │  Session Manager            │  │
                    │  │  (JWT + Device Fingerprint  │  │
                    │  │   + IP Reputation +         │  │
                    │  │   Tool Identity)            │  │
                    │  └─────────────┬───────────────┘  │
                    │                │                  │
                    │  ┌─────────────┴───────────────┐  │
                    │  │  Policy Evaluation Engine   │  │
                    │  │  (RBAC + ABAC + Rego)       │  │
                    │  └─────────────────────────────┘  │
                    └───────────────────────────────────┘
                                      │
                    ┌─────────────────┴─────────────────┐
                    │   RESOURCES                        │
                    │  ┌────────┐ ┌────────┐ ┌──────┐   │
                    │  │Memory  │ │Policies│ │Audit │   │
                    │  │Store   │ │ Engine │ │Store │   │
                    │  └────────┘ └────────┘ └──────┘   │
                    └────────────────────────────────────┘
```

### 2.1 Identity Sources (Who can connect?)

NexusMind acepta identidades de tres fuentes:

| Fuente | Mecanismo | Uso típico |
|---|---|---|
| **SSO Enterprise** | SAML 2.0 / OIDC / SCIM | Okta, Azure AD, Google Workspace, OneLogin |
| **API Keys** | Bearer token con scopes | Herramientas CI/CD, agentes autónomos, MCP servers |
| **Device Auth** | Device Code Flow + MFA | Developers conectando desde CLI/IDE |

### 2.2 Tool Identity (What tool is connecting?)

No solo autenticamos al **usuario**. También autenticamos a la **herramienta**. Esto es crítico para el control plane:

```
Request: "Ana escribe código con Cursor"

Identity Layer verifica:
1. ¿Quién es Ana? → SSO (Okta) → User: ana@acme.com
2. ¿Qué herramienta usa? → Plugin Cursor envía tool_id + tool_secret
3. ¿El dispositivo es conocido? → Device fingerprint match
4. ¿La ubicación es esperada? → IP en rango corporativo
5. ¿La herramienta tiene permiso para este proyecto? → Policy check
```

Cada herramienta que se integra con NexusMind recibe un **Tool ID** y **Tool Secret** único. Esto permite:
- Revocar acceso de una herramienta específica sin afectar al usuario
- Auditar por herramienta ("Cursor tuvo X accesos a memoria sensible")
- Diferenciar entre "Ana desde Cursor" y "Ana desde Claude Code"

### 2.3 Device Fingerprinting

Para sesiones desde CLI/IDE, NexusMind genera un device fingerprint basado en:

```
fingerprint = SHA256(
  hostname +
  OS version +
  tool version +
  workspace path hash +
  SSH key fingerprint (si aplica)
)
```

Esto permite detectar:
- Un mismo usuario desde dos máquinas distintas
- Una máquina comprometida intentando acceder
- Un developer que cambió de equipo sin reportarlo

---

## 3. Autenticación (Authentication)

### 3.1 SSO Enterprise (SAML 2.0 / OIDC)

Flujo primario para empresas con identity provider existente.

```
1. User abre Cursor plugin de NexusMind
2. Plugin redirige a NexusMind Auth Gateway
3. Auth Gateway inicia SSO flow con Okta/Azure AD:
   a. Genera AuthnRequest (SAML) o Authorization Request (OIDC)
   b. Redirige al IdP corporativo
4. User se autentica en su IdP (password + MFA)
5. IdP envía assertion/ID token a NexusMind callback
6. NexusMind valida:
   - Firma de la aserción
   - Tiempo de vida (no expirado)
   - Audiencia (destinado a NexusMind)
   - Atributos requeridos (email, groups, roles)
7. NexusMind:
   a. Crea/actualiza identidad del usuario
   b. Sincroniza grupos/roles vía SCIM
   c. Genera JWT de sesión
   d. Redirige al plugin con el JWT
8. Plugin usa JWT para todas las requests posteriores
```

**SCIM Sync**: Cuando el IdP soporta SCIM, NexusMind sincroniza automáticamente:
- Nuevos usuarios → acceso automático (si aplica política)
- Usuarios desactivados → revocación inmediata
- Cambios de rol → permisos actualizados sin intervención

### 3.2 API Keys (Machine-to-Machine)

Para agentes autónomos, CI/CD pipelines y MCP servers.

```
Key Format: nmk_prod_abc123def456... (prefijo por entorno)
           nmk_dev_...  (development)
           nmk_stg_...  (staging)

Cada key tiene:
- Scopes: [memory:read, memory:write, policy:evaluate, audit:write]
- Tool binding: vinculada a una herramienta específica
- Rate limits: por key
- Expiration: opcional (recomendado rotar cada 90 días)
- IP whitelist: opcional (solo desde IPs autorizadas)
```

**Revocación inmediata**: Las keys se pueden revocar desde Admin Console. La revocación es en caliente — no requiere redeploy.

### 3.3 Device Auth + MFA

Para developers que conectan su CLI/IDE directamente.

```
1. Developer ejecuta: nexusmind auth login
2. CLI muestra código de dispositivo: ABCD-1234
3. Developer abre nexusmind.ai/activate en el browser
4. Autentica con SSO corporativo
5. Confirma MFA (TOTP, Push, o WebAuthn)
6. CLI recibe token de sesión (validez: 8h)
7. Session se renueva automáticamente (refresh token)
```

**MFA es obligatorio** para acceso a memoria con datos sensibles (PII, PHI, secretos).

---

## 4. Autorización (Authorization)

### 4.1 Modelo: RBAC + ABAC + Custom Roles

NexusMind usa un modelo híbrido donde **los roles son 100% customizables** por cada empresa:

- **Custom RBAC**: Cada empresa crea los roles que necesita. No hay roles fijos ni obligatorios.
- **ABAC** (Attribute-Based Access Control): Políticas finas basadas en atributos del usuario, herramienta, proyecto, ubicación, etc.
- **Herencia opcional**: Los roles pueden heredar permisos de otros roles (ej: "senior-dev hereda de junior-dev más estos 3 permisos extra").

### 4.2 Roles: 100% Customizables

Los roles que aparecen en este documento son **templates de arranque** — ejemplos que se instalan por defecto pero que cada empresa puede modificar, eliminar o ignorar por completo.

**Cada empresa puede:**
- Crear roles con nombres propios (ej: "Ingeniero ML", "DevOps Lead", "Contractor", "Intern")
- Asignar permisos exactos a cada rol (selección granular de cada permiso)
- Heredar permisos de otro rol y extenderlos
- Clonar un rol existente y ajustarlo
- Importar/exportar roles vía YAML (git-ops friendly)
- Versionar roles (cada cambio queda registrado en audit trail)
- Tener roles distintos por proyecto (un rol en proyecto A puede tener distintos permisos que el mismo rol en proyecto B)

### 4.3 Definición de un Rol (YAML)

Cada rol se define como un recurso versionado en YAML:

```yaml
apiVersion: nexusmind.io/v1
kind: Role
metadata:
  name: ml-engineer
  displayName: "ML Engineer"
  description: "Ingeniero de machine learning con acceso a datos de entrenamiento"
  color: "#8b5cf6"   # Color en Admin Console
  icon: "🧠"
  extends: []         # Puede heredar de otro rol
  enterprise: true    # Solo disponible en plan Enterprise
spec:
  permissions:
    # Memoria
    - resource: memory
      actions: [read, write, search]
      constraints:
        max_sensitivity: sensitive    # No puede ver 'critical'
        projects: ["ml-models", "research", "inference"]
    
    # Políticas
    - resource: policy
      actions: [read]
      constraints:
        policies: ["ml-training-rules", "data-governance"]
    
    # Audit — solo sus propias acciones
    - resource: audit
      actions: [read]
      constraints:
        scope: self_only
    
    # Admin — ninguno
    
  abac_overrides: []  # ABAC rules específicas para este rol
```

### 4.4 Catálogo de Permisos Disponibles

Cada permiso se define como: `resource:action`. Una empresa puede asignar **cualquier combinación** de estos a un rol:

| Permiso | Descripción |
|---|---|
| `memory:read` | Leer entradas de memoria |
| `memory:write` | Escribir en memoria |
| `memory:delete` | Eliminar entradas de memoria |
| `memory:search` | Buscar en memoria |
| `memory:export` | Exportar memoria a JSON/CSV |
| `policy:read` | Ver políticas existentes |
| `policy:create` | Crear nuevas políticas |
| `policy:update` | Modificar políticas existentes |
| `policy:delete` | Eliminar políticas |
| `policy:evaluate` | Evaluar una request contra políticas |
| `policy:export` | Exportar políticas a YAML |
| `audit:read` | Ver audit trails |
| `audit:export` | Exportar audit trails a CSV/PDF |
| `audit:delete` | Forzar eliminación de registros (solo compliance officer) |
| `role:read` | Ver roles definidos |
| `role:create` | Crear nuevos roles |
| `role:update` | Modificar roles existentes |
| `role:delete` | Eliminar roles |
| `admin:users` | Gestionar usuarios (invitar, suspender, eliminar) |
| `admin:tools` | Registrar/revocar herramientas del ecosistema |
| `admin:projects` | Crear/eliminar proyectos |
| `admin:settings` | Configuración global de la instancia |
| `admin:billing` | Ver y gestionar facturación |
| `admin:audit` | Configurar políticas de retención de audit |

### 4.5 Restricciones por Permiso (Constraints)

Cada permiso puede tener restricciones adicionales:

```yaml
# Ejemplo: Solo lectura de memoria hasta nivel 'internal', solo en proyecto X
memory:read:
  max_sensitivity: internal    # No puede leer sensitive o critical
  projects: ["acme-webapp"]   # Solo este proyecto
  tools: ["cursor"]            # Solo desde esta herramienta
  time_window:                 # Solo en horario laboral
    from: "09:00"
    to: "18:00"
    timezone: "Europe/Madrid"
  rate_limit: 100              # Máximo 100 requests/minuto
  require_mfa: true            # Requiere MFA para usar este permiso
```

### 4.6 Herencia de Roles

Los roles pueden heredar permisos de otros roles, permitiendo crear jerarquías sin duplicar definiciones:

```yaml
apiVersion: nexusmind.io/v1
kind: Role
metadata:
  name: senior-dev
  extends: ["junior-dev"]
spec:
  permissions:
    # Hereda todos los permisos de junior-dev automáticamente
    # Solo añade los extra:
    - resource: memory
      actions: [delete]
      constraints:
        max_sensitivity: sensitive
    - resource: policy
      actions: [read, evaluate]
```

La herencia es **transitiva** y **aditiva**: senior-dev tiene todo lo de junior-dev + lo suyo. No se puede quitar un permiso heredado (para eso se crea un rol desde cero).

### 4.7 Roles por Proyecto

Un mismo rol puede tener **permisos distintos en proyectos distintos**:

```yaml
# Ana tiene rol "developer" en dos proyectos, pero con distinto acceso
ana@acme.com:
  project_memberships:
    - project: acme-payments
      role: developer
      # En payments, developer puede leer hasta 'sensitive'
      permissions_overrides:
        memory:read:
          max_sensitivity: sensitive
    
    - project: acme-webapp
      role: developer
      # En webapp, developer solo ve 'public'
      permissions_overrides:
        memory:read:
          max_sensitivity: public
```

### 4.8 Roles vs. ABAC

Los roles definen **qué puede hacer** un usuario. ABAC define **bajo qué condiciones**. Ambos se evalúan en conjunto:

```
Ejemplo:
- Ana tiene rol "developer" → puede escribir memoria hasta 'sensitive'
- ABAC: request proviene de Nigeria (location no esperada)
- Resultado: DENIED (ABAC override)
```

Los ABAC overrides se pueden definir globalmente (para toda la empresa) o por rol.

### 4.9 Templates de Arranque

NexusMind incluye estos templates que las empresas pueden usar como base o ignorar:

| Template | Permisos incluidos | Uso típico |
|---|---|---|
| **Super Admin** | Todos los permisos | Administración global |
| **Security Officer** | audit:*, policy:*, memory:read (todo), admin:audit | Compliance y seguridad |
| **Project Admin** | memory:*, policy:read, audit:read (su proyecto), admin:users | Líder técnico de proyecto |
| **Developer** | memory:read/write/search (hasta sensitive), policy:read | Ingeniero senior |
| **Junior Developer** | memory:read/search (hasta internal) | Ingeniero en formación |
| **Viewer** | memory:read/search (hasta public) | Stakeholders, PMs |
| **Tool / Agent** | memory:read/write (según key scope), policy:evaluate | Agentes autónomos |
| **Auditor** | audit:read/export (cross-project) | Auditores externos |

**Las empresas pueden:**
- Usarlos tal cual
- Modificarlos (añadir/remover permisos)
- Borrarlos y crear los suyos desde cero
- Tener múltiples roles con el mismo nombre pero distinta configuración

### 4.10 Gestión de Roles (Admin Console)

Desde la Admin Console, un usuario con permiso `role:create` puede:

1. **Crear rol**: Nombre + descripción + selección granular de permisos
2. **Clonar rol**: Duplicar un rol existente como punto de partida
3. **Editar rol**: Añadir/remover permisos. Audit trail registra cada cambio.
4. **Versionar rol**: Cada edición crea una nueva versión. Se puede revertir.
5. **Exportar rol**: Descargar como YAML (git-ops)
6. **Importar rol**: Subir YAML (ideal para infraestructura como código)
7. **Simular rol**: Ver qué permisos tendría un usuario antes de asignárselo
7. **Previsualizar impacto**: "¿A quiénes afecta si cambio este permiso?" — muestra lista de usuarios que perderían/ganarían acceso

### 4.11 API de Roles

```
GET    /v1/roles                    → Listar roles definidos
POST   /v1/roles                    → Crear nuevo rol
GET    /v1/roles/:id                → Ver detalle de rol
PUT    /v1/roles/:id                → Actualizar rol
DELETE /v1/roles/:id                → Eliminar rol (solo si no tiene miembros)
POST   /v1/roles/:id/clone         → Clonar rol
POST   /v1/roles/:id/simulate      → Simular qué permisos otorga
GET    /v1/roles/:id/members       → Usuarios con este rol
GET    /v1/roles/:id/versions      → Historial de versiones
POST   /v1/roles/:id/rollback      → Revertir a versión anterior

GET    /v1/permissions             → Catálogo completo de permisos disponibles
```

### 4.12 Base de Datos (esquema roles)

```sql
CREATE TABLE roles (
  id TEXT PRIMARY KEY,           -- UUID
  org_id TEXT NOT NULL,           -- Organización a la que pertenece
  name TEXT NOT NULL,             -- Nombre interno (ej: "ml-engineer")
  display_name TEXT NOT NULL,     -- Nombre visible (ej: "ML Engineer")
  description TEXT,
  extends TEXT[],                 -- IDs de roles de los que hereda
  color TEXT,                     -- Color para UI
  icon TEXT,                      -- Emoji o icon path
  yaml_definition TEXT NOT NULL,  -- Definición completa en YAML
  version INT NOT NULL DEFAULT 1,
  enabled BOOLEAN NOT NULL DEFAULT true,
  is_template BOOLEAN NOT NULL DEFAULT false, -- Template de arranque
  created_by TEXT REFERENCES users(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE role_versions (
  id TEXT PRIMARY KEY,
  role_id TEXT REFERENCES roles(id),
  version INT NOT NULL,
  yaml_definition TEXT NOT NULL,
  change_summary TEXT,
  changed_by TEXT REFERENCES users(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE project_role_overrides (
  project_id TEXT REFERENCES projects(id),
  role_id TEXT REFERENCES roles(id),
  permission_overrides JSONB,  -- Permisos que se modifican para este proyecto
  PRIMARY KEY (project_id, role_id)
);
```

```yaml
# Ejemplo 2: Herramientas no aprobadas no pueden leer memoria
apiVersion: nexusmind.io/v1
kind: Policy
metadata:
  name: tool-whitelist-memory-read
spec:
  match:
    action: memory:read
  rules:
    - allow:
        tools: ["cursor", "claude-code", "copilot", "open-code"]
      deny:
        tools: ["*"]
  on_violation: block_and_log
  audit: always
```

```yaml
# Ejemplo 3: Data residency — solo Europa
apiVersion: nexusmind.io/v1
kind: Policy
metadata:
  name: data-residency-eu
spec:
  match:
    project: ["client-confidential", "eu-customers"]
  rules:
    - allow:
        request.ip_country: ["ES", "FR", "DE", "NL", "IE"]
      deny:
        request.ip_country: ["*"]
  on_violation: block_and_alert_security
  audit: always
```

---

## 5. Aislamiento de Memoria (Memory Isolation)

Esta es una de las preguntas más críticas para las empresas: **¿qué contenido de la memoria centralizada puede leer cada quién?**

### 5.1 Principio de Mínimo Privilegio

Por defecto, un usuario/agente solo puede leer memoria que cumpla **todas** estas condiciones:

1. **Pertenezca a su proyecto** (o proyectos donde tenga acceso multi-proyecto)
2. **No esté etiquetada con un nivel de sensibilidad superior a su rol**
3. **No haya sido escrita por una herramienta que explícitamente marcó la entrada como privada**
4. **No infrinja políticas activas de data residency o compliance**

### 5.2 Etiquetas de Sensibilidad

Cada entrada de memoria lleva una etiqueta de sensibilidad:

```
mem_9a8b7c:
  content: "La API key de Stripe es sk_live_..."
  sensitivity: critical   ← Solo roles seniors pueden leer esto
  tags: ["credentials", "production"]
  written_by: "cursor"
  project: "acme-payments"
  created_at: "2026-05-10T19:00:00Z"
```

| Nivel | Quién puede leer | Ejemplos |
|---|---|---|
| `public` | Todos en el proyecto | Decisiones de arquitectura, tech stack |
| `internal` | Todos en el proyecto | Issues conocidos, workarounds |
| `sensitive` | Developer Senior+ | Datos de clientes, configs internas |
| `critical` | Project Admin+ | Credenciales, secretos, PII, PHI |
| `audit_only` | Security Officer + Auditor | Logs de acceso, decisiones bloqueadas |

### 5.3 Memoria por Proyecto

La memoria está organizada por **proyecto**. Cada proyecto tiene:

- Su propio namespace de memoria
- Su propio conjunto de políticas
- Su propio equipo con roles asignados
- Opcional: su propia base de datos SQLite (aislamiento físico)

```
/acme-payments/        ← Proyecto 1
  ├── memoria/         ← Solo equipo de payments
  ├── policies/
  └── audit/
  
/acme-webapp/          ← Proyecto 2
  ├── memoria/         ← Solo equipo de webapp
  ├── policies/
  └── audit/
  
/acme-ai-governance/   ← Proyecto de Security (cross-project)
  ├── memoria/         ← Security Officers + Super Admins
  ├── policies/
  └── audit/
```

### 5.4 Memoria Cross-Project

Un usuario puede tener acceso a múltiples proyectos. Sus roles se evalúan **por proyecto**:

```
Ana:
  - Proyecto "payments": Developer Senior (read/write sensitive)
  - Proyecto "webapp": Viewer (read-only public)
  - Proyecto "ai-governance": Sin acceso

→ Ana puede buscar en memoria de payments y webapp
→ En payments ve hasta sensitive; en webapp solo public
→ No ve nada de ai-governance
```

### 5.5 Tool-Level Memory Isolation

No solo el usuario importa. La **herramienta** también puede tener restricciones:

```
Cursor plugin de Ana:
  - Scopes: memory:read, memory:write
  - Memoria permitida: public, internal → no puede leer sensitive+

Claude Code plugin (misma Ana):
  - Scopes: memory:read, memory:write
  - Memoria permitida: public, internal, sensitive → puede leer más

→ Ana desde Cursor ve menos que Ana desde Claude Code
→ La herramienta también dicta el nivel de acceso
```

Esto permite políticas como:
- "Los agentes CI/CD solo ven memoria `public`"
- "Los agentes no-approbados (BYOT) solo ven `public` e `internal`"
- "Solo herramientas con certificación SOC2 pueden leer `critical`"

### 5.6 Memory Redaction Automática

Cuando un usuario/agente lee memoria que contiene datos sensibles para los que no tiene permiso, esos datos se redactan automáticamente:

```
Input original:
  "La API key de producción es sk_live_abc123 y el password del admin es Passw0rd!"

Output para Developer Junior:
  "La API key de producción es [REDACTED] y el password del admin es [REDACTED]!"

Output para Developer Senior:
  "La API key de producción es sk_live_abc123 y el password del admin es [REDACTED]!"
  (PII email sigue redactado porque no necesita saberlo)
```

Esto se logra con el **Policy Engine** que escanea contenido al vuelo y aplica reglas de redacción antes de devolverlo.

---

## 6. Session Management

### 6.1 Token Types

| Token | Vida útil | Propósito |
|---|---|---|
| **ID Token** | 1h | Identidad del usuario (JWT) |
| **Access Token** | 8h | Acceso a APIs (JWT con scopes) |
| **Refresh Token** | 30 días | Renovar sin re-autenticar |
| **Tool Token** | 90 días | Identidad de herramienta |
| **API Key** | Configurable (recomendado 90d) | Machine-to-machine |

### 6.2 Token Revocación

Escenarios que disparan revocación inmediata:
- Usuario desactivado en IdP (SCIM sync)
- Rol cambiado
- Dispositivo reportado como perdido/robado
- API key rotada manualmente
- Política nueva que afecta al usuario/herramienta
- Sesión desde ubicación no esperada (detección de anomalía)

### 6.3 Session Dashboard

La Admin Console muestra en tiempo real:

```
Session activas: 47
  ├── Ana García (cursor) → proyecto payments → desde Madrid, IP:...
  ├── Ana García (claude-code) → proyecto payments → desde Madrid, IP:...
  ├── Carlos Ruiz (cursor) → proyecto webapp → desde Barcelona, IP:...
  ├── CI/CD Pipeline (tool: github-actions) → proyecto payments → desde GitHub, IP:...
  └── ...

Alertas activas: 2
  ├── 🚨 Ana García desde ubicación no habitual (Lagos, Nigeria)
  └── 🚨 API key "nmk_dev_..." usada desde IP no whitelisted
```

---

## 7. Integración con Identity Providers

### 7.1 Proveedores Soportados (V1)

| Proveedor | Protocolo | Features |
|---|---|---|
| **Okta** | OIDC + SCIM | SSO, MFA, sync grupos, deprovisioning |
| **Azure AD** | OIDC + SCIM | SSO, MFA, sync grupos, Conditional Access |
| **Google Workspace** | OIDC + SCIM | SSO, sync grupos |
| **OneLogin** | SAML 2.0 + SCIM | SSO, MFA, sync roles |
| **Generic SAML 2.0** | SAML 2.0 | Compatible con cualquier IdP SAML |
| **Generic OIDC** | OIDC Discovery | Compatible con cualquier IdP OIDC |

### 7.2 SCIM 2.0 (System for Cross-domain Identity Management)

SCIM permite sincronización automática de usuarios y grupos:

```
Eventos SCIM que NexusMind procesa:
├── User.created → Crear usuario, asignar rol por defecto
├── User.updated → Actualizar atributos (department, manager)
├── User.deactivated → Revocar sesiones activas inmediatamente
├── User.deleted → Marcar como eliminado, retener audit trails
├── Group.created → Crear rol映射 en NexusMind
├── Group.updated → Re-evaluar permisos de todos los miembros
└── Group.deleted → Re-asignar o desactivar miembros
```

Mapeo típico IdP → NexusMind:
```
Okta Group "Engineering" → NexusMind Role "Developer Senior"
Okta Group "Security Team" → NexusMind Role "Security Officer"
Okta Group "Contractors" → NexusMind Role "Viewer"
```

### 7.3 Just-In-Time (JIT) Provisioning

Si un usuario no existe en NexusMind pero se autentica vía SSO, se crea automáticamente con rol por defecto. Esto evita tener que pre-crear usuarios.

El rol por defecto es configurable por empresa. Recomendado: **Viewer** (mínimo privilegio hasta que un admin lo suba de rol).

---

## 8. Flujo Completo: Ana desde Cursor

```
1. Ana abre Cursor y usa el plugin NexusMind

2. Plugin llama a NexusMind Auth Gateway:
   POST /v1/auth/check
   Headers: Authorization: Bearer <cursor_tool_token>
   Body: { user_id: "ana@acme.com", tool: "cursor" }

3. Auth Gateway verifica:
   ✓ Tool token válido (Cursor está registrado en la empresa)
   ✓ User existe en NexusMind (sincronizado vía SCIM desde Okta)
   ✓ User tiene rol "Developer Senior" en proyecto "payments"
   ✓ Dispositivo es conocido (fingerprint match)
   ✓ IP está en rango corporativo (España)

4. Plugin envía request a Memory API:
   POST /v1/memory/search
   Headers: Authorization: Bearer <session_jwt>
   Body: { query: "API keys de Stripe", project: "payments" }

5. Policy Evaluation Engine:
   ✓ Ana es Developer Senior → puede leer hasta "sensitive"
   ✗ "API keys de Stripe" está etiquetado "critical"
   → Búsqueda filtrada: resultados critical son excluidos

6. Memory devuelve resultados filtrados:
   - "Decidimos usar Stripe como payment processor" (public) ✓
   - "Problema con webhooks de Stripe" (internal) ✓
   - "Config de Stripe: endpoints" (sensitive) ✓
   - [API key de Stripe oculta] (critical, redactado)

7. Audit Trail registra:
   timestamp: "..."
   user: "ana@acme.com"
   tool: "cursor"
   action: "memory:search"
   query_hash: "..."
   results_count: 4
   filtered_count: 1 (critical)
   policy_decisions: ["P-001:passed", "P-015:redacted"]
   ip: "xxx.xxx.xxx.xxx"
   device_fingerprint: "..."
```

---

## 9. Cumplimiento y Certificaciones

| Requisito | Cómo lo cumple NexusMind |
|---|---|
| **SOC2** | Audit trails inmutables, RBAC granular, access reviews |
| **GDPR** | Data residency, right to deletion, exportabilidad |
| **EU AI Act** | Trazabilidad de decisiones AI, registro de modelos usados |
| **HIPAA** | PHI redaction automática, audit trails, BAA-ready |
| **SOX** | Audit trails financieros, separation of duties |
| **ISO 27001** | Access control policy, asset management |

---

## 10. Implementación Técnica

### 10.1 Stack de Auth

| Componente | Tecnología |
|---|---|
| **OIDC / SAML RP** | Go + zitadel/oidc + crewjam/saml |
| **JWT** | golang-jwt (RS256 o Ed25519) |
| **Session Store** | SQLite (MVP) → Redis (scale) |
| **Policy Engine** | Rego (OPA) + Custom Go evaluator |
| **SCIM Server** | Go + custom SCIM 2.0 implementation |
| **Device Fingerprint** | SHA256 de host+OS+tool+workspace |
| **Rate Limiter** | Token bucket (en memoria, distribuido con Redis) |

### 10.2 Base de Datos (esquema auth)

```sql
-- Users
CREATE TABLE users (
  id TEXT PRIMARY KEY,           -- UUID
  email TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  idp_subject TEXT,              -- Subject del IdP (SAML/OIDC)
  idp_provider TEXT,             -- "okta", "azure-ad", "google"
  status TEXT NOT NULL DEFAULT 'active',  -- active, suspended, deleted
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_login TIMESTAMPTZ
);

-- Tools registrados
CREATE TABLE tools (
  id TEXT PRIMARY KEY,           -- "cursor", "claude-code", etc.
  name TEXT NOT NULL,
  tool_secret_hash TEXT NOT NULL,
  tool_type TEXT NOT NULL,       -- "plugin", "mcp", "cli", "agent"
  status TEXT NOT NULL DEFAULT 'active',
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- API Keys
CREATE TABLE api_keys (
  id TEXT PRIMARY KEY,
  key_prefix TEXT NOT NULL,       -- "nmk_prod_", "nmk_dev_"
  key_hash TEXT NOT NULL,
  user_id TEXT REFERENCES users(id),
  tool_id TEXT REFERENCES tools(id),
  scopes TEXT[] NOT NULL,         -- ["memory:read", "policy:evaluate"]
  ip_whitelist TEXT[],            -- CIDR ranges
  expires_at TIMESTAMPTZ,
  last_used_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Proyectos
CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  sensitivity_default TEXT NOT NULL DEFAULT 'internal',
  data_residency TEXT[],           -- ["EU", "US"]
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Miembros de proyecto con roles
CREATE TABLE project_members (
  user_id TEXT REFERENCES users(id),
  project_id TEXT REFERENCES projects(id),
  role TEXT NOT NULL,              -- "admin", "developer-senior", etc.
  granted_by TEXT,
  granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (user_id, project_id)
);

-- Policy definitions
CREATE TABLE policies (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  policy_yaml TEXT NOT NULL,       -- YAML con reglas
  enabled BOOLEAN NOT NULL DEFAULT true,
  version INT NOT NULL DEFAULT 1,  -- Versionado git-ops
  created_by TEXT REFERENCES users(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Sessions activas
CREATE TABLE sessions (
  id TEXT PRIMARY KEY,
  user_id TEXT REFERENCES users(id),
  tool_id TEXT REFERENCES tools(id),
  device_fingerprint TEXT,
  ip_address INET,
  jwt_jti TEXT NOT NULL UNIQUE,    -- JWT ID
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

## 11. API Endpoints de Auth

```
POST   /v1/auth/login            → SSO redirect / Device code
POST   /v1/auth/token            → Exchange auth code por JWT
POST   /v1/auth/refresh          → Refresh token
POST   /v1/auth/revoke           → Revocar sesión
GET    /v1/auth/session          → Información de sesión actual
POST   /v1/auth/check            → Verificar acceso (para plugins)

GET    /v1/keys                  → Listar API keys
POST   /v1/keys                  → Crear API key
DELETE /v1/keys/:id              → Revocar API key

GET    /v1/users                 → Listar usuarios
POST   /v1/users                 → Crear usuario manual
PUT    /v1/users/:id             → Actualizar rol/status
DELETE /v1/users/:id             → Desactivar usuario

GET    /v1/tools                 → Listar herramientas registradas
POST   /v1/tools                 → Registrar nueva herramienta
DELETE /v1/tools/:id             → Desregistrar herramienta
```

---

*Fin de AUTH_SPEC.md v1.0*
