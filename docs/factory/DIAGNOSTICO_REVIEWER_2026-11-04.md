# Diagnóstico: bot solicitado como reviewer, sin revisión

Comprobación productiva de solo lectura, 4 de noviembre de 2026. No se modificó la configuración, validó/reactivó un agente, reenviaron eventos ni llamaron modelos.

**Rectificación posterior:** el ledger `autonomous_agent_validations` sí contenía validaciones válidas, también para la revisión 4. La columna inmutable pending estaba desactualizada por diseño y el filtro de GitHub la consultaba erróneamente. No faltaba una acción del usuario. [Causa final y corrección](GITHUB_PUBLICACION_CUENTA_BACKEND.md). El diagnóstico inicial siguiente se conserva como histórico, no como estado final.

## Diagnóstico inicial (rectificado)

- PR: `kasymir/kasymir-app-ui#691`, autor `xtraix20`, abierto, no draft, SHA `000376b6739e59d969d48beff6949b767f79616e`.
- Reviewer solicitado: `CesarRuizBotAgents`, coincide con la identidad del CLI del backend. No es un caso de autorrevisión.
- El webhook **sí llegó**: `review_requested`, delivery `703dd8a0-d7e5-11f1-9c61-1e07cc9840db`, registrado `2026-11-04 00:19:48` UTC. La suscripción al PR quedó activa.
- Agente: `Kasymir AppUI Reviewer`, ID `c5d239b3-21a2-487d-88d9-2df86c7bfbfd`, revisión **4**. Figura `enabled`, pero `validation_status=pending`, sin `validated_at`.
- No se creó work item para el PR 691. `enqueue_github_webhook_agents` exige definición habilitada **y revisión actual validada**; por eso el evento se registró pero no se encoló la ejecución.
- Pi en el backend devuelve `status=ready`; la falta de autenticación Pi no es el bloqueo observado. El runner de workspaces y la imagen Node no participan en esta ruta.

## Otros dos problemas observados

1. Tiene un schedule cada 120 minutos sin número de PR. Sus últimos runs son de `trigger_kind=schedule` y terminan `blocked_runtime`, `pull_request_number_missing`. Esos runs no son las solicitudes de review de GitHub.
2. Su revisión contiene `auto_merge=true`. No se ha validado ni reproducido el evento para evitar convertir el diagnóstico en una aprobación o merge no autorizado.

## Defecto de interfaz

`AgentCard.agentStatus` prioriza `enabled` antes que `validation_status`; presenta verde «Enabled» aunque falte validar. Su acción principal es Run y no Validate en ese estado. La habilitación histórica no demuestra elegibilidad actual para el webhook.

## Reparación propuesta, pendiente de confirmación

- Dejar el reviewer en **solo revisión, sin auto-merge**.
- Quitar la ejecución periódica sin PR (destino por eventos de GitHub).
- Validar la nueva revisión y habilitarla explícitamente.
- Reconciliar la solicitud vigente para el SHA actual del PR 691; no tratar un delivery ya registrado como si fuera nuevo ni saltarse deduplicación/filtros.
- Corregir la tarjeta para mostrar «Necesita validación» y la acción correspondiente aun en el estado histórico enabled/pending.

La aprobación de esos cambios no autoriza merges. No se deben copiar credenciales del backend al runner ni mezclar esta ruta con el workflow de workspaces.
