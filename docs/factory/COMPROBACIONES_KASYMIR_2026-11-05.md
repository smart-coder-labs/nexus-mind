# Comprobaciones Kasymir por SSH · 2026-11-05

## Estado real

Se auditaron **49 repositorios**, usando commits que coinciden con los 49 SHA del inventario confirmado del workspace `f0bec300-6964-4e81-aaa7-bed0653621e6` (revisión de inventario 1). Se guardaron **109 comprobaciones**, revisión de runtime **16**, sin sustituir agentes, modelos, perfil del revisor, servicios, permisos ni presupuestos del usuario.

| Tipo | Entradas |
|---|---:|
| Tests | 45 |
| Build | 44 |
| Typecheck | 1 |
| QA funcional | 16 |
| Diseño responsive | 1 |
| Regresiones de seguridad | 2 |

Hay alguna comprobación configurada para **46/49 repositorios**. ComputerVision, Flutter y WordPress necesitan sus SDK/recetas propios; no se les asignó falsamente Node, un build de sintaxis ni un test vacío.

**Aplicado no significa aprobado.** El runner aplica la configuración, pero `ready=false`: faltan servicios. No se lanzó el workflow completo ni se fabricaron resultados de QA. La ejecución global de todas las recetas también debe respetar el límite existente de 1800 segundos de cada fase; no se amplió ese presupuesto.

## Qué se corrigió

- AppUI: `npm run flb` se reemplazó por el build real `npm run build`; no se ejecutan los formateadores/autofixes de `flb` durante la verificación.
- AppUI: `npm run generate` no era un typecheck. Se configuró `node node_modules/typescript/bin/tsc --noEmit --incremental false`.
- Se preservan los hooks de los comandos de build. Si generan cambios en fuentes versionadas, la verificación continúa detectándolos; no se desactivó ese control.
- Jest/Vitest conservan sus scripts reales, con informes JSON y validación estricta de resultados. Se rechazan cero tests, omisiones y TODOs. Los `--passWithNoTests` existentes se anulan explícitamente. Jest usa `--runInBand`; Vitest un trabajador, sin paralelismo entre archivos.
- Node/tsx usa informe TAP y `--test-concurrency=1`: no se confía únicamente en el código de salida. Los contratos GraphQL tienen una comprobación separada.
- Playwright utiliza el CLI local, no el script AppUI que fija una caché de navegadores de macOS. Se exige Chromium, un trabajador, sin retries ni `.only`; se rechazan omisiones, fallos esperados, flaky tests y anotaciones `a11y-finding`.
- Los reportes JSON se resumen con cantidades y digest en los logs. Las capturas anidadas de Playwright se recogen con nombres sin colisiones.

## Entornos instalados por el operador

El usuario no tuvo que introducir imágenes. Se mantuvo el SDK Node 22 fijado y se construyeron SDK QA desde bases oficiales inmutables, sin contexto de repositorio ni credenciales:

| Entorno | ID local ARM64 |
|---|---|
| Node 22 | `node@sha256:18a6d90fd60542269850c80ca03b29b1d9b14d6b221ac7ee8e35b0d33ef904bd` |
| Playwright 1.61.1, versión exacta del lock AppUI | `sha256:d2dbe5b91759520bc1fae477d26e912e39a671f88da39cdeefc703d9b36712bf` |
| Playwright 1.62.0, herramientas QA de Backoffice; instalación pnpm del proyecto aún pendiente | `sha256:ba640fb60883fb4ae481f5140831c55a761cace2b7ba966b2d342f644b127412` |

Ambos navegadores arrancaron realmente en ARM64 con `network=none`, usuario sin privilegios y filesystem de solo lectura, contra una página fixture. El instalador npm offline del SDK QA pasó tres pruebas, incluyendo tarballs verificados y bloqueo de scripts de instalación. **Estas pruebas de herramientas no acreditan QA del producto.**

Los SDK adicionales están en la allowlist local y en `node_bootstrap_images`; ningún campo del repositorio puede autorizar otra imagen. Los tarballs siguen sujetos al lockfile/SHA512, registro permitido, límites de tamaño y preparación offline. No se habilitó acceso libre a Internet, Docker socket ni credenciales de producción.

Se registró `kasymir-graphql-unit`: 33 parámetros requeridos por el módulo GraphQL, con endpoints de loopback inaccesibles y un remitente `.invalid`, sin credenciales. Es **solo para unit tests con mocks**, no sustituye servicios reales. `NODE_ENV=test` se fija en el comando, no como valor de referencia privada: el redactor trata conservadoramente los valores del archivo como secretos y una cadena pública corta podría alterar nombres de checks en los resultados.

## Matriz de los 49 repositorios

Todos los nombres siguientes tienen prefijo `kasymir-`. `Build` significa el script versionado `npm run build`, salvo donde se indica que falta. `F` = suite Jest E2E existente después de levantar servicios; no acredita por sí sola cobertura completa de negocio.

| Repositorio | Tests | Build | QA / observaciones |
|---|---|---|---|
| accounting-service | Jest estricto | Sí | F; lockfile no portable, 809 rutas temporales externas de macOS: bloqueado sin regenerarlo silenciosamente. |
| analytics-service-stack | Jest estricto | Sí | F. |
| api-nest-public-service | Jest estricto | Sí | F. |
| app-ui | Jest estricto | Sí | Playwright funcional, responsive y regresión de cookies; paquete privado y cuenta de pruebas pendientes. |
| audit-service | Jest estricto | Sí | Sin tests versionados: la suite vacía debe fallar. |
| backoffice-dashboard | Vitest estricto | Sí | Lock pnpm: preparación aislada pendiente; E2E tiene casos TODO omitidos y no se presenta como QA completo. |
| backoffice-service | Jest estricto | Sí | Sin tests versionados. |
| boilerplate-serverless-nest | Jest estricto | Sí | F; paquete privado. |
| cameras-streaming-service | `bash infra/tests/verify-infra-safety.sh` | Sin receta | Check estático real. El test de arranque requiere un entorno/VM específico y no se registró como si funcionara en el SDK restringido. |
| checkout-ui | No existe suite | Sí | No se inventó un `node --test` vacío ni QA visual. Paquete privado. |
| common-domain | Jest estricto | Sí | Se anula el éxito con cero tests. |
| computervision | Pendiente SDK Python/dependencias | Pendiente receta | Tiene pytest e integración Mongo. No se sustituyó por un supuesto build Node. |
| customers-service | Jest estricto | Sí | F. |
| dbmigration-service-stack | Jest estricto | Sí | E2E declarado sin suite localizada: no se inventó cobertura. |
| design-canvas | No existe suite | Sí | No se inventó QA visual. |
| discounts-service | Jest estricto | Sí | E2E declarado sin suite localizada. |
| ecommerce-integrations-service | Jest estricto | Sí | Sin E2E localizado. |
| generate-pdf-service-stack | Jest estricto | Sí | Paquete privado. |
| goals-service | Jest estricto | Sí | Sin E2E localizado. |
| graphql-service | TAP estricto + contratos | Sí | Regresiones de autenticación, tickets WS y aislamiento de organización; no es DAST. |
| iam-service | Jest estricto | Sí | F. |
| import-files-service-stack | Jest estricto | Sí | F; paquete privado. |
| imports-service | Jest estricto | Sí | Sin tests versionados; paquete privado. |
| infraestructure-service | `bash kubernetes/services/vend-payment-service/configuration.test.sh` | Sin receta | Falla sobre el commit por `configmap.yaml` ausente; no se copió configuración productiva. |
| invoice-service | Jest estricto | Sí | Sin tests versionados. |
| localization-service | Jest estricto | Sí | Sin tests versionados; paquete privado. |
| locations-service-stack | Jest estricto | Sí | Se conserva la copia PEM del build versionado. |
| media-service | Jest estricto | Sí | Sin tests versionados. |
| method-payment-service | Jest estricto | Sí | Se anula el éxito con cero tests. |
| mobile-app | Pendiente SDK Flutter/Dart/Android | Pendiente receta | Existen tests Dart/Kotlin; iOS/dispositivo requiere otro runner, no una simulación en Linux. |
| notification-service | Jest estricto | Sí | Se anula el éxito con cero tests. |
| organization-setup-service-stack | Jest estricto | Sí | F. |
| outbox-relay | Jest estricto | Sí | Sin tests versionados; se anula el éxito con cero tests. |
| payment-gateway-service | Jest estricto | Sí | F. |
| payment-service | Jest estricto | Sí | F; engines heredado restringe Node 18, requiere revisión del proyecto. |
| products-service | Jest estricto | Sí | Sin E2E localizado para registrar como cobertura real. |
| purchases-service | Jest estricto | Sí | Sin E2E localizado. |
| sales-service | Jest estricto | Sí | F. |
| search-service | Jest estricto | Sí | Sin tests versionados. |
| send-message-notification | Jest estricto | Sí | F. |
| shipping-service | Jest estricto | Sí | Sin tests versionados; paquete privado. |
| storage-service-stack | Jest estricto | Sí | Se conserva la copia PEM del build versionado. |
| terminals-service | Jest estricto | Sí | F. |
| transaction-service | Jest estricto | Sí | F. |
| ui-common | Vitest estricto | Sí | Se conserva el build real, incluido el guard de tipos; no se sustituyó por uno parcial. |
| url-shorten-service | Jest estricto | Sí | F. |
| vision-insights-service | Vitest estricto | Sí | Sin tests versionados; se anula el éxito con cero tests. |
| webhooks-service | Jest estricto | Sí | Sin E2E localizado. |
| wordpress-plugin | Pendiente SDK/receta PHP | No hay build declarado | Sin suite ni Composer; no se creó una aprobación ficticia. |

## Alcance exacto de QA y pendientes

- Funcional AppUI: suites existentes salvo los dos archivos de login/diagnóstico excluidos por credenciales fijas o trazas sensibles. Se requieren explícitamente `E2E_EMAIL` y `E2E_PASSWORD` de una cuenta **de pruebas**; no se usan fallbacks. No se crearon esas credenciales ni se contactó producción.
- Diseño AppUI: cinco suites con comprobaciones reales de overflow móvil y capturas. No es una auditoría exhaustiva de accesibilidad ni comparación completa de píxeles. Un hallazgo de accesibilidad anotado por el test impide aceptar el resultado.
- Seguridad: regresiones existentes de cookies AppUI y autorización/aislamiento GraphQL. No se anuncia como pentest ni escaneo DAST de los 49 repos.
- Ocho lockfiles requieren lectura autorizada de paquetes GitHub. El token de publicación de PRs no se reutilizó ni se copió al sandbox para resolver esto.
- Diez repos declaran tests sin archivos de pruebas. Dos frontends no declaran tests. Las recetas no convierten esos huecos en verde.
- Siguen faltando servicios integrados y fixtures/cuentas de QA. El DAG existente conserva `verify → environment → functional/design/security`. No se levantaron aplicaciones, bases productivas ni servicios externos.

## Evidencia, despliegue y recuperación

- Límite corregido de 64 a **256** checks: 49 repos × tests/build ya supera 64. [Release dee35b1 / 42018354851](https://github.com/smart-coder-labs/nexus-mind/actions/runs/42018354851), todos los jobs correctos. Se partió del despliegue concurrente `77b112f`, conservando su alcance por proyectos.
- SDK QA, soporte de preparación Node autorizado localmente, wrappers estrictos y capturas anidadas: [release a4806e5 / 42021616827](https://github.com/smart-coder-labs/nexus-mind/actions/runs/42021616827), todos los jobs correctos. No hay migración nueva.
- Runner: `/home/ubuntu/.local/lib/nexusmind/runner-a4806e5/`; backup privado anterior al cambio en `/home/ubuntu/.local/state/nexusmind/runtime-backups/20261105T120506Z/`.
- Auditoría, plan aplicado, imágenes y validaciones manuales: `/home/ubuntu/.local/state/nexusmind/kasymir-checks-audit/`, privado. No publicar los dumps crudos de tests: se encontraron credenciales fijas en fuentes E2E.
- El generador `scripts/factory/kasymir_checks_plan.py` produce las recetas desde los manifests auditados; sus tests de reportes falsos solo verifican el gate, no acreditan QA del producto.
- Validación manual aislada, sin leases ficticios productivos ni modificación de fuentes: build GraphQL pasó; sus contratos pasaron con configuración de unit tests, mientras la suite general sigue fallando. El check estático de cámaras pasó. Accounting se bloqueó antes de instalar por el lockfile no portable; el test de configuración de infraestructura falló por un archivo ausente.

Las escrituras de configuración usaron la API con revisión esperada y claves temporales revocadas en `finally`. No se modificaron repositorios Kasymir, PRs, schedules, permisos ni despliegues de aplicaciones. No se marcaron aprobados los 109 checks ni el workflow completo.
