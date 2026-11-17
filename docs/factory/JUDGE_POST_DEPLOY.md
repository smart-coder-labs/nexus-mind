# Judge después del deploy: conexión de GitHub y Slack

El Judge con `trigger_workflow` corre cuando ese workflow termina en la rama principal tras un merge. Verifica los PRs mergeados y las issues que cierran, y si algo falla avisa por Slack a los responsables. Para que funcione hacen falta tres conexiones únicas.

## 1. GitHub: evento "Workflow runs"

En el repo, en **Settings → Webhooks** (o en la GitHub App), edita el webhook de NexusMind y marca **Workflow runs**, además de los eventos que ya tiene. Sin este evento el backend nunca se entera de que terminó el deploy.

## 2. Slack: login único en el worker

La imagen ya trae el servidor `slack` en `/app/qa-mcp.json`: el MCP alojado de Slack con el mismo cliente OAuth que el plugin de Slack de Claude. Falta autorizarlo una vez dentro del contenedor `autonomous-worker`. El token queda en su `HOME` persistente (`/claude-home`) y sobrevive a los redeploys.

> No copies la sesión de tu Claude local. Los tokens de refresco rotan, así que la copia invalida una de las dos sesiones. Además, todo lo que se publique saldría a nombre de esa persona. Lo ideal es iniciar sesión con un usuario de servicio de Slack. Los mensajes salen como el usuario que autoriza.

Desde tu máquina, en una terminal, abre el túnel del callback OAuth (puerto 3118) hasta el pod:

```bash
ssh -L 3118:localhost:3118 oracle \
  'kubectl -n nexusmind port-forward deploy/nexusmind-backend 3118:3118'
```

En otra terminal, abre Claude dentro del worker con esa config:

```bash
ssh -t oracle \
  'kubectl -n nexusmind exec -it deploy/nexusmind-backend -c autonomous-worker -- \
     claude --mcp-config /app/qa-mcp.json'
```

Dentro de Claude: `/mcp`, luego **slack**, luego **Authenticate**. Abre en tu navegador la URL que muestra e inicia sesión en Slack. El callback a `localhost:3118` vuelve por el túnel. Cuando `/mcp` muestre `slack · connected`, sal con `/exit` y cierra el túnel.

Para comprobarlo:

```bash
ssh oracle 'kubectl -n nexusmind exec deploy/nexusmind-backend -c autonomous-worker -- \
  claude -p "Use the slack MCP to search the user named <tu nombre>; reply only with the user id" \
  --mcp-config /app/qa-mcp.json --allowedTools "mcp__slack__*"'
```

Si el token caduca o se revoca, repite este paso.

## 3. Configurar el Judge

En el admin, en **Factory → Agentes**, crea o edita el Judge:

- **Repositories**: el repo, por ejemplo `smart-coder-labs/nexus-mind`.
- **Run after this workflow on the default branch**: el nombre exacto del workflow, por ejemplo `Deploy`.
- **Notify the people responsible on Slack when a claim fails**: activado, y el canal si quieres (por ejemplo `#deploys`). Sin canal, envía un DM a cada responsable.
- **Target**: la URL de producción de la app.

## Prueba

Mergea un PR pequeño que cierre una issue. Cuando termine el workflow `Deploy`, en **Ejecuciones** debe aparecer una ejecución del Judge (`github_webhook`) con el PR y la issue como objetivos. Si todo se cumple no llega nada a Slack. Si algo falla, llega un solo mensaje que menciona al autor del PR y a los asignados de la issue.
