# Pi administrado: runtime automático y OAuth solo en el backend

Estado: desplegado el 2026-11-05, SHA `ea81f936853f4979abbc9704650c7e0668698a60`, [release 42006713831](https://github.com/smart-coder-labs/nexus-mind/actions/runs/42006713831). No equivale a aceptación del workflow completo. Evidencia y pendientes: [PUBLICACION_PI_2026-11-05.md](PUBLICACION_PI_2026-11-05.md).

## Uso

En **Workflows → Configuración → Agentes**, «Usar Pi del backend en las cuatro fases» configura el SDK administrado y conserva los checks, servicios y modelos ya seleccionados para Pi. Sustituye los harnesses incompatibles; muestra los modelos resultantes antes de guardar. No inicia tareas ni publica código.

No hace falta escribir un digest, una conexión de modelo ni una red. Los adaptadores personalizados conservan la configuración avanzada y su autorización local. No existe fallback entre Pi y Claude.

## Frontera de seguridad

- El runner construye una vez la receta SDK revisada e instalada por el operador, con base y versión Pi fijadas. El contexto de construcción está vacío: nunca se construye un Dockerfile del repositorio ni se incluyen secretos.
- El ID inmutable resultante se anuncia como `managed_images.agent`. Cada ejecución congela ese ID en su snapshot. Una receta nueva no cambia un snapshot existente.
- Pi y sus herramientas se ejecutan en el contenedor, con `network=none`, sin Docker socket y con los montajes y límites habituales. Investigador y revisor tienen herramientas y código de solo lectura.
- Un endpoint de loopback dentro del sandbox intercambia solicitudes y SSE acotados por un socket Unix exclusivo del intento. Solo el proceso del runner puede hablar HTTPS con el backend. El sandbox no recibe la credencial del runner, el lease ni OAuth; su clave aparente es un marcador sin autoridad.
- El backend comprueba organización habilitada explícitamente, runner, lease activo, intento, fase y modelo del snapshot. Nunca acepta un URL de proveedor ni elige el modelo a petición del repositorio.
- OAuth se resuelve y refresca con `ModelRuntime.getAuth('openai-codex')` en el backend. No se copia `auth.json`, una conversación, una sesión del navegador ni el directorio Pi al runner.
- No se habilitan herramientas remotas del proveedor, respuestas almacenadas, trabajos en background ni escalamiento de tier. Pausa, cancelación, revocación y cambio de fence interrumpen el acceso.

## Habilitación del operador, una vez

1. Desplegar backend, admin y los archivos coordinados del runner: `runner.py`, `execution.py`, `delivery.py`, `container_entry.py`, `runtime_settings.py`, `node_environment.py`, `model_relay.py`, `managed_agent.py`, `Dockerfile.agent`.
2. Autorizar únicamente la organización aprobada mediante `FACTORY_BACKEND_MODEL_ORG_ID` en el contenedor backend. Sin esta variable, ninguna organización puede usar su cuenta.
3. Conservar el JSON privado del runner y activar `allow_backend_models: true` y `auto_provision_agent_runtime: true`. Mantener su permiso de configuración remota y sus allowlists existentes. No añadir archivos de autenticación ni claves personales.
4. Reiniciar el servicio y esperar la construcción/inspección del SDK y el reporte del runner. El digest lo obtiene el programa; no se escribe manualmente.

La receta necesita acceso a los repositorios públicos de paquetes durante la preparación del SDK. Las ejecuciones posteriores no tienen esa salida. Un fallo de preparación mantiene el runtime bloqueado; no autoriza una imagen distinta.

## Límites y evidencia

Hay como máximo cuatro streams de modelo simultáneos en el backend y 32 solicitudes por intento, contabilizadas antes del envío en eventos persistentes `model.requested`. Cada solicitud tiene límites de cuerpo, respuesta y tiempo. Las solicitudes fallidas también consumen el límite. Los reintentos automáticos, el cache warming y los recursos del proyecto están desactivados. Cada fase abre una conversación nueva; el workflow conserva sus intentos y leases.

El consumo de una suscripción no es una factura API. La salida lleva `billing=subscription_not_invoice` y tokens autoritativos; no se debe interpretar el campo de compatibilidad `cost_usd=0` como ausencia de consumo o como coste de la suscripción.

**Guardado**, **aplicado por el runner** y **QA ejecutado con éxito sobre el SHA final** son estados diferentes. Instalar Pi no crea tests, servicios, credenciales de paquetes privados ni resultados funcionales, visuales o de seguridad.

En el diagnóstico de Kasymir (revisión 8), además del runtime faltaban tests, QA funcional/diseño/seguridad y servicios; AppUI necesita acceso autorizado a una dependencia privada. Nada de esto se omite para permitir el arranque.

## Comprobaciones reproducibles

```sh
cargo test --manifest-path apps/backend/Cargo.toml --lib api::workflow_model
cargo test --manifest-path apps/backend/Cargo.toml --lib managed_pi_resolves
python3 -m unittest discover -s scripts/factory -p test_model_relay.py -v
# Prueba del CLI real con proveedor simulado, sin consumo OAuth:
FACTORY_PI_BIN=/ruta/pi python3 -m unittest discover -s scripts/factory -p test_model_relay.py -v
# Prueba real del SDK y del aislamiento Docker, también con proveedor simulado:
docker build -t nexusmind-managed-runtime:test -f scripts/factory/Dockerfile.agent /ruta/contexto-vacio
FACTORY_MANAGED_IMAGE=$(docker image inspect --format '{{.Id}}' nexusmind-managed-runtime:test) python3 -m unittest discover -s scripts/factory -p test_model_relay.py -v
```

Estas pruebas comprueban transporte, parser, aislamiento y cercas. Un proveedor simulado no acredita inferencia real ni aceptación pagada ni QA del producto.
