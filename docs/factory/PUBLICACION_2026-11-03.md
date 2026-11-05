# Publicación verificada: Factory guiada y conectada

Autorizada por el propietario después de revisar la vista previa. Publicada el **3 de noviembre de 2026**.

## Resultado

- Panel: <https://admin.nexusmind.smartcoderlabs.com/factory>
- API: <https://api.nexusmind.smartcoderlabs.com>
- SHA desplegado: **`63125d281263a6c333495ffb02c836d4e3e5fcac`**.
- Release: [41666357163](https://github.com/smart-coder-labs/nexus-mind/actions/runs/41666357163), **todos los jobs correctos**.
- Rama de publicación: `deploy/factory-workflows-v95` (nombre histórico, ahora schema **97**).
- PR #410 sigue draft y sin merge. No se fusionaron ramas ni se desplegaron aplicaciones de Kasymir.
- Cloudflare: <https://5b02de43.nexusmind-admin.pages.dev>.

## Comprobaciones

El CI repitió **1861** tests backend (4 ignorados explícitamente), **26** integraciones de workflows/API/backups, **55** tests Python y **125** del admin; TypeScript, build del panel e imágenes correctos. Backend y proxy se actualizaron antes de publicar el panel.

Después del despliegue:

- Salud HTTPS: API y base de datos `ok`; schema **97**, `quick_check` correcto.
- Catálogo autenticado: pausa común, pendientes, hallazgos y proyección mínima de perfiles responden correctamente.
- Kasymir conserva **49 repositorios, todos con autorización de escritura**. Permanecen dos runs históricos `succeeded`; no se inició un nuevo trabajo.
- Runner activo, sin reinicios por fallo, usando `/home/ubuntu/.local/lib/nexusmind/runner-v97-96e7405/`.
- Su configuración privada, credencial, estado y límites de systemd se conservaron. La unidad anterior está en `~/nexusmind-backups/runner-service-before-schema97.service`.
- `runtime.revision=0`, `ready=false`, `remote_enabled=false`: se mantienen los bloqueos reales; no se han inventado adaptadores o QA instalados.
- Los accesos de comprobación usaron claves temporales revocadas en `finally`; quedan **0** claves activas de ese procedimiento.
- El JavaScript servido por el dominio productivo coincide byte a byte con el artefacto de este run: `assets/index-IayPP37c.js`, SHA-256 `da34f0333e317d375231750f43928d9a33a580a6ff994f322bf1dc49e8c50ab0`.

## Artefactos inmutables

- Backend/worker/proxy ARM64: `ghcr.io/smart-coder-labs/nexusmind-backend@sha256:8d2575ff511003b71c0173e27c5daaa31bc08aa0c1a5aa22daf6dba5c37fe427`.
- Comandos ARM64: `ghcr.io/smart-coder-labs/nexusmind-factory-commands@sha256:918297de3fc13949e3dc3b893a0819288d50a721fc842c98c0411ad37880bcc8`.
- Imagen de agente multiarch publicada: `ghcr.io/smart-coder-labs/nexusmind-workflow-agent@sha256:9c03d15f88502b1bf704d4071df5030fedd7d47350766e4c7ed8e12b5bbc7b66`. Publicarla no significa instalarla o autorizarla en el runner.

## Backups y reversión

Antes de migrar:

1. `/data/pre-factory-guided-schema95.db`, **1347731456 bytes**, SHA-256 `80a1bc818a9e71a7e59c827c57369c876618e8d7d96268c38f37cf288c29fc75`. Copias verificadas con schema 95 y `quick_check` en Oracle `~/nexusmind-backups/` y en el equipo del operador `~/.local/state/nexusmind/deploy-backups/`.
2. El release generó después otra copia, inmediatamente antes del rollout: `/data/pre-workflows-20261103T155147Z.db`, con `integrity_check` correcto. Copia en `~/nexusmind-backups/`; SHA-256 `02c6bcd12cd611d207ac38654e539a9b6aca1ca96d7edb22d2416bb28c8c5d31`.

Las copias son privadas (0600), no están en repositorios ni se montan en agentes. La copia posterior debe preferirse para reducir la ventana de datos perdidos.

Imagen anterior: `ghcr.io/smart-coder-labs/nexusmind-backend@sha256:fc1f851f7d93ccc47e44b6eb7288071b0de0a348a427a918c20b121924dc1334`.

**No basta con bajar la imagen después de una migración de schema.** Una reversión necesita ventana de mantenimiento, parar escritores y runner, copia del estado actual y plan explícito para restaurar schema 95; restaurarlo elimina escrituras posteriores al backup. No se realizó ninguna reversión.

## Qué no se ha activado

No se copiaron secretos productivos, cuentas OAuth, claves maestras ni permisos administrativos de GitHub a agentes. No se llamaron modelos pagados, hicieron merges ni desplegaron aplicaciones Kasymir. Las preferencias globales históricas no reactivan agentes bloqueados automáticamente.

Para ejecutar un workflow completo siguen haciendo falta wrappers locales revisados para harnesses adicionales, proxies/auth autorizados, imágenes SDK, entornos de desarrollo, servicios y verificaciones reales. La publicación del panel **no constituye aceptación end-to-end** de esos recursos ni de dispositivos móviles.

Uso y límites: [Factory conectada](FACTORY_CONNECTED.md), [Configuración](CONFIGURACION_ADMIN.md) y [Guía](GUIA_DE_USO.md).
