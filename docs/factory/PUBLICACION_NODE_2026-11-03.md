# Corrección publicada: entorno Node automático

- Producción: SHA **`f2673a99c396b03d89c5ba2eb5e0f42effd4ce15`**.
- [Release 41739545278](https://github.com/smart-coder-labs/nexus-mind/actions/runs/41739545278): todos los jobs correctos. Backend, panel y runner actualizados; schema continúa en 97.
- CI: 1862 tests de biblioteca pasados (4 ignorados), 26 integraciones, 59 Python y 127 UI; TypeScript/build correctos. Incluye instalación npm real offline en contenedor.
- Runner Oracle: `/home/ubuntu/.local/lib/nexusmind/runner-f2673a9/`, activo, sin reinicios por fallo. Se registró el SDK Node público inmutable y se habilitó la configuración remota para que el workflow lo prepare; credenciales, estado y límites anteriores conservados.
- Descarga automática del SDK registrada verificada en Oracle usando la misma función de inventario del runner. Prueba offline repetida en ARM64: tres tests correctos, importación del paquete fixture y bloqueo del `preinstall` demostrados, sin modificar clones reales.
- Catálogo autenticado confirma `managed_images.node`, imagen disponible y 49 repositorios autorizados. La revisión del usuario permanece en **1**, con **0 comprobaciones guardadas**: no se sustituyó su configuración ni se inventaron comandos para recuperar un borrador que había fallado.
- El workflow completo sigue `ready=false`: no se han configurado por esta corrección los harnesses/modelos, servicios o QA pendientes. No se llamaron modelos pagados ni se iniciaron trabajos Kasymir.
- El JS productivo coincide con el artefacto de CI: `assets/index-DMAjILmX.js`, SHA-256 `94c4e534f0c28b9d8cad2ade82a4dc66f4d9c3bb1de42c12a3532d35913f1c9f`.
- Backup previo del release: `/data/pre-workflows-20261103T234033Z.db`, verificado por SQLite antes del rollout. Copias privadas del archivo de configuración y unidad anteriores al cambio Node: `~/nexusmind-backups/runner-config-before-node-f2673a9.json` y `runner-service-before-node-f2673a9.service`.

## Fallos corregidos

1. Cambiar el entorno común ya actualiza filas existentes que heredaban el anterior, incluidas las vacías; conserva overrides explícitos.
2. La selección Node no requiere pegar un digest o instalar una imagen manualmente: el runner prepara el SDK registrado y dependencias verificadas.
3. El error de guardado identifica imagen, repositorio, comando o duplicado. Se permiten varias comprobaciones del mismo tipo/repositorio si sus comandos son distintos.
4. El paquete privado de `kasymir-app-ui` se bloquea con un motivo específico: requiere conexión autorizada a su registro. No se reemplaza ese bloqueo por una falsa aprobación o una petición genérica de imagen.

Uso y límites: [Entorno Node](ENTORNO_NODE.md). Refresca el panel, elige **Node.js 22 · preparación automática**, añade los comandos y guarda. Los borradores previos que no llegaron al servidor no se han reconstruido automáticamente.
