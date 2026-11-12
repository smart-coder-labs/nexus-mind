# Origen verificable de cada ejecución

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

Esta corrección no consolida/cancela la cola, cambia temporizadores o autenticaciones, ni lanza nuevos modelos de prueba. Los pendientes de configuración/proyectos/sandboxes permanecen en [PENDIENTES_OPERADOR.md](PENDIENTES_OPERADOR.md).
