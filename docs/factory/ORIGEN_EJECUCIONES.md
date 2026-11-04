# Origen verificable de cada ejecución

**Publicado y verificado:** SHA `4d2ef3e89b3db9ea1fdf3d92b6f3a06abbee4234`, [release 41785347496](https://github.com/smart-coder-labs/nexus-mind/actions/runs/41785347496), todos los jobs correctos. Schema 97 sin migración.

El historial ya no usa «GitHub event» para mezclar notificaciones y consultas del worker.

| Etiqueta | Evidencia requerida |
|---|---|
| Webhook recibido de GitHub | `trigger_kind=github_webhook`, origen registrado y delivery ID asociado a una entrega recibida para la misma organización, repositorio y hash de payload |
| Reconciliación de GitHub (sondeo) | `trigger_kind=reconcile`, origen explícito del polling y scan ID |
| Encadenado de agente | `trigger_kind=reconcile`, origen del encadenado y run ID del agente anterior |
| Temporizador | `trigger_kind=schedule` persistido por el scheduler |
| GitHub · origen histórico no registrado | Run antiguo guardado con `github_webhook` pero sin evidencia detallada del origen |
| Reconciliación o encadenado · histórico | Run antiguo con `reconcile` sin procedencia detallada |

La lista y el detalle consumen la misma función. El detalle permite desplegar **Evidencia del origen** y consultar el identificador registrado. El API expone `trigger_origin` como metadata; no permite al contenido de un issue elegir esa procedencia.

La creación de runs usa una variante tipada Webhook/Reconciliation, no deduce el origen de prefijos de strings ni del momento del despliegue. Un supuesto webhook sin delivery registrado falla cerrado. Se conservan la firma del endpoint, deduplicación, permisos, cancelación por SHA obsoleto y comprobación de solicitud de revisión para los runs de polling.

No se reescribe el historial ni se adivina el origen de runs antiguos por fecha, nombre o cercanía a un evento. Una reconciliación posterior no cambia el origen inicial de un run ya deduplicado. No se requiere migración SQLite: se conserva el CHECK existente, y el origen se registra en el input persistido del control plane, fuera del contexto configurable del modelo.

Verificación: CI con 1863 tests backend pasados (4 ignorados), 26 integraciones, 59 Python y 132 UI, TypeScript/build correctos. El primer intento local de backend tuvo un fallo en el test de checkpoints; no se contó como aprobado. El CI del SHA final repitió la suite completa y pasó. El API productivo devuelve los runs históricos sin `trigger_origin` inventado. El JS servido coincide con el artefacto CI: `assets/index-DmHmpL6s.js`, SHA-256 `fc68d75c71dcb3f632eb8a906c1903ed78ccb624b1bcb353af94df4888566300`. Salud y `quick_check` correctos; backup previo `/data/pre-workflows-20261104T063524Z.db`.

Comprobación adicional: el reviewer solicitado del PR #691 terminó correctamente y publicó [una revisión COMMENTED](https://github.com/kasymir/kasymir-app-ui/pull/691#pullrequestreview-5763593520) sobre SHA `000376b6739e59d969d48beff6949b767f79616e`. No se confundió con una aprobación formal ni un merge.

Esta corrección no consolida/cancela la cola, cambia temporizadores o autenticaciones, ni lanza nuevos modelos de prueba. Los pendientes de configuración/proyectos/sandboxes permanecen en [PENDIENTES_OPERADOR.md](PENDIENTES_OPERADOR.md).
