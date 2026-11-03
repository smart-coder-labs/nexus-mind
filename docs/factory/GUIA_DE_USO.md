# Guía de uso: Kasymir, runner Oracle y reviews de GitHub

Panel: https://admin.nexusmind.smartcoderlabs.com/factory/workflows

API pública: `https://api.nexusmind.smartcoderlabs.com`

La nueva configuración guiada está **en vista previa local, no desplegada**. Recorrido de formularios y requisitos: [Configuración del admin](CONFIGURACION_ADMIN.md).

## 1. Reviews automáticos de GitHub

Este flujo no necesita el runner de workspaces.

1. Crea un agente de plantilla **GitHub PR Reviewer**, configura su repositorio, valídalo y habilítalo.
2. En **GitHub requested reviews**, selecciona ese agente.
3. Conserva la URL pública del API indicada arriba.
4. Pulsa **Connect GitHub webhook**.
5. Abre un PR normal del mismo repositorio, desde una cuenta distinta del bot, y solicita revisión a **CesarRuizBotAgents**.
6. Consulta el resultado en GitHub y la ejecución en **Factory → Runs**.

**Autor y reviewer no pueden ser la misma cuenta de GitHub.** Si el resolver abrió el PR como `CesarRuizBotAgents`, no podrás solicitar revisión a ese mismo bot, aunque uses otra cuenta en el navegador o inicies otro agente/modelo. Además, un draft debe marcarse **Ready for review** antes de solicitar revisión. Para probar el webhook, usa un PR no-draft creado por `cr8297408` u otra cuenta. Para una revisión formal de GitHub de PRs generados por el bot, necesitas otra identidad de reviewer o de publicación. La fase interna `review` del workflow sí usa una invocación independiente con contexto limpio, aunque publique el mismo bot; no equivale a una aprobación formal de GitHub. El webhook y reviewer autónomo mantienen su exclusión de PRs propios.

Los nuevos commits se revisan con contexto nuevo. Los forks y PRs propios del bot se excluyen. La cuenta revisora es la autenticada en el `gh` del backend, no tu sesión del navegador.

### Superusuario de NexusMind frente a permisos de GitHub

Son permisos independientes:

- **Superusuario de NexusMind:** permite administrar la integración dentro de esta aplicación.
- **Administración de webhooks en GitHub:** permite crear/modificar el hook del repositorio; requiere autorización real de GitHub y los permisos adecuados en el token.

Si el bot no puede administrar hooks, puedes usar **Webhook administration token (optional)** con un token de una cuenta administradora, limitado al repositorio y a sus webhooks. Se utiliza únicamente durante esa petición: no se guarda ni se entrega a los agentes. La cuenta que revisa sigue siendo el bot; no es necesario convertirlo en administrador del repositorio.

Un hook instalado por un administrador puede reutilizarse sin proporcionar otra vez el token. Para cambiar su URL o volver a crearlo, vuelve a proporcionar autorización administrativa.

Para `kasymir/kasymir-app-ui`, el hook se configuró con la cuenta administradora `cr8297408`, conservando a `CesarRuizBotAgents` como reviewer. Se comprobó la firma con un evento que no inicia una revisión ni consume un modelo. No hace falta volver a crear ese hook para probar un PR.

Nunca pegues tokens en chats, issues, código o archivos versionados.

## 2. Runner de Kasymir en Oracle

**Estado instalado (2026-11-03):** runner `Oracle Kasymir` activo y persistente, workspace `Kasymir · Oracle`, 49 repositorios descubiertos y confirmados, todos con escritura habilitada por petición explícita del usuario. La prueba `manual:oracle-kasymir-runner-bootstrap-v1` terminó en `succeeded`, sin modelos ni cambios de código. Los adaptadores de modelos, QA y entornos de pruebas aún no están configurados: el runner no se presenta como listo para resolver tareas end-to-end hasta completar esos perfiles.

- Runner ID: `607a6a06-9b74-4fdb-b83a-caa4879567f2`.
- Workspace ID: `f0bec300-6964-4e81-aaa7-bed0653621e6`.
- Definición de prueba disponible: `Kasymir Oracle - preparation check`.
- Ejecución de prueba: `6dae93e5-2a1e-4944-a6b7-89208b36ea8b`.

El runner es el proceso que descubre los clones y ejecuta los pasos autorizados. No se ejecuta dentro del backend HTTP.

Rutas instaladas del runner administrado:

| Elemento | Ruta en Oracle |
|---|---|
| Clones separados | `/home/ubuntu/nexusmind-workspaces/kasymir/` |
| Programa del runner | `/home/ubuntu/.local/lib/nexusmind/runner-v95/` |
| Configuración privada | `/home/ubuntu/.config/nexusmind/kasymir-runner.json` |
| Estado y checkouts temporales | `/home/ubuntu/.local/state/nexusmind/kasymir-runner/` |
| Servicio de usuario | `nexusmind-kasymir-runner.service` |

No se reutilizan ni modifican los clones de despliegue ni `kasymir-app-ui-dev`. Los clones nuevos no reciben archivos `.env` de producción. El usuario autorizó explícitamente escritura en los 49 repositorios actuales. Esto no autoriza merges, despliegues ni acceso a secretos de producción. Los snapshots de ejecuciones anteriores no se modifican; los workflows y runs nuevos utilizarán el inventario confirmado actualizado.

### Consultar el servicio

Una vez finalizada la instalación:

```sh
ssh oracle 'systemctl --user status nexusmind-kasymir-runner --no-pager'
ssh oracle 'journalctl --user -u nexusmind-kasymir-runner -n 50 --no-pager'
ssh oracle 'systemctl --user restart nexusmind-kasymir-runner'
```

La configuración contiene un token: debe tener modo `0600`. No imprimas ni compartas el archivo completo. El estado y las credenciales están fuera de las carpetas de código.

## 3. Probar primero la preparación

1. Abre **Workspaces & workflows** y busca el runner **Oracle Kasymir**.
2. Comprueba que actualiza **Last seen**.
3. Abre el workspace **Kasymir · Oracle**.
4. Revisa el inventario y pulsa **Confirm selected repositories** si está pendiente.
5. Usa **Prepare workspace workflow** y **Save definition**.
6. En **Start a workflow**, selecciona esa definición:
   - **Source key:** `manual:kasymir-preparacion-001`.
   - **Task title:** `Comprobar preparación de los repositorios de Kasymir`.
7. Pulsa **Start execution** y revisa la ejecución en **Executions**.

Esta prueba solo crea copias de los commits. No llama a modelos, no cambia código, no abre PRs y no despliega Kasymir.

## 4. Resolver una tarea de extremo a extremo

La conexión del runner no equivale a tener configurados todos los modelos y pruebas de cada proyecto.

Antes del flujo completo hay que instalar/configurar:

- Adaptadores de investigación, autor de tests, implementación y reviewer independiente.
- Imágenes revisadas por digest, con los SDK y dependencias necesarios.
- Acceso al modelo mediante un proxy autorizado y limitado; sin copiar credenciales de GitHub ni producción a agentes.
- Comandos reales de tests, build, lint/typecheck y QA funcional, diseño y seguridad.
- Servicios y datos desechables de pruebas. Nunca usar la base de producción.
- Para móviles, SDK/emulador/dispositivo o runner de plataforma apropiado. La ausencia de esa capacidad bloquea QA; no se considera aprobado.

Después:

1. Autoriza escritura solo en los repositorios necesarios del workspace.
2. Pulsa **New end-to-end workflow**.
3. Selecciona los adaptadores disponibles por fase, configura el conector de publicación y los presupuestos; guarda la definición.
4. En **Start a workflow**, introduce una Source key única y una tarea con problema, pasos de reproducción y criterios de aceptación.
5. Sigue las fases en **Executions**: investigar → tests de regresión → implementar → tests/build → servicios integrados → QA → draft PRs → review independiente.

Los adaptadores ausentes esperan; las comprobaciones fallidas bloquean o generan reparación acotada. Los logs y artefactos permiten ver qué se ejecutó. Una nueva versión del código invalida las comprobaciones correspondientes. No se fusionan PRs ni se despliega automáticamente.

## 5. Entornos de desarrollo

Los `.env` deben ser de desarrollo, propiedad del usuario del runner, con modo `0600`, y estar fuera de los clones. Por ejemplo: `/home/ubuntu/.config/nexusmind/env/kasymir-backend.env`.

No se copian automáticamente los `.env` de tus clones ni de los despliegues existentes. Cada adaptador/servicio debe solicitar explícitamente su referencia de entorno.

## 6. Linear, Slack y Sentry

Una vez probado el workflow manual, conecta sus entradas desde **Connected intakes** usando conectores cifrados y limitados a la fuente. La selección del workflow es común; no necesitas crear un issue de GitHub intermedio. Las entradas respetan deduplicación, políticas y aprobaciones.

## 7. Qué revisar si se detiene

- **Not connected:** servicio del runner, conectividad HTTPS y credencial.
- **Inventory confirmation required:** confirmar el inventario actualizado.
- **Missing adapter/capability:** instalar/configurar ese adaptador; no es un fallo de código del proyecto.
- **Approval required:** revisar y aprobar la acción y revisión correctas.
- **Tests/build/QA failed:** abrir los logs y evidencias; no asumir que pasaron.
- **GitHub webhook permissions:** revisar la cuenta/token de administración de GitHub, no cambiar el rol de NexusMind a ciegas.

Referencia técnica: [WORKSPACES_WORKFLOWS.md](./WORKSPACES_WORKFLOWS.md) y [GITHUB_REVIEW_WEBHOOKS.md](./GITHUB_REVIEW_WEBHOOKS.md).
