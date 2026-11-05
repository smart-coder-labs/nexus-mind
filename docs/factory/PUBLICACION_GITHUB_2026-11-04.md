# Publicación verificada: cuenta GitHub del backend y reviewer

- SHA productivo: **`0fdfb2e79625925dd167fcc074aea980470b13bb`**.
- [Release 41754968626](https://github.com/smart-coder-labs/nexus-mind/actions/runs/41754968626): todos los jobs correctos. Backend, worker, proxy y admin actualizados. Schema continúa en **97**.
- CI: **1862** tests de biblioteca pasados (4 ignorados explícitamente), **26** integraciones, **59** Python y **131** UI, TypeScript/build correctos. La prueba de publicación recorre tanto conector como cuenta backend, verifica SHA/CI/draft, idempotencia, cambio de cuenta y revocación.
- `GET /v1/factory/github/publication-identity` autenticado confirma `enabled=true`, login **CesarRuizBotAgents**, sin error. La autorización de organización proviene de la variable de release `FACTORY_GITHUB_PUBLISH_ORG_ID`, aplicada únicamente al backend.
- Salud HTTPS y base de datos correctas; `quick_check` ok.
- El JavaScript servido coincide byte a byte con el artefacto CI: `assets/index-BNdPluWt.js`, SHA-256 `a1857c47fe161c96a020bff2a54aa402506b677f8898865217bfd239353aa849`.
- Backup anterior al rollout verificado: `/data/pre-workflows-20261104T014626Z.db`. No se restauró ninguna base ni se borró historial.

## Reviewer solicitado

La validación vigente se consulta ahora en el ledger append-only y no en la columna inmutable inicial. Tras desplegar se reconciliaron las solicitudes vigentes sin modificar o falsificar revisiones:

- PR **691**, SHA `000376b6739e59d969d48beff6949b767f79616e`: run **`a8c59d65-dfb1-43b3-aff8-256338c94eca`**, encolado.
- PR **649**, SHA `73aaf2b87ded587ad934930982d2712e1797e170`: run **`e50a3074-d664-46ac-a281-914ae47f58f9`**, encolado.

Al comprobar después del despliegue permanecían en cola; el worker estaba ejecutando un trabajo de `Kasymir Issues Resolver`. **Encolado no significa revisión publicada.** No se cancelaron trabajos ajenos ni se afirmó un resultado de modelo inexistente.

El reviewer de AppUI está habilitado, revisión 5 validada; `auto_merge=false`, schedule manual desactivado. La cuenta, harness y demás opciones se conservaron. El mensaje de PR ausente ya no lo atribuye a la infraestructura ni promete reintento automático.

No se hicieron merges, se desplegaron aplicaciones de Kasymir ni se publicaron cambios de repositorio para probar la cuenta. Las pruebas de publicación reales contra GitHub se limitan a consulta de identidad; el flujo de escritura fue probado contra el servidor fixture, preservando todos los gates. Los tokens no se enviaron a runners o agentes.

## Alcance de esta entrega

Incluye [publicación con cuenta backend](GITHUB_PUBLICACION_CUENTA_BACKEND.md) y corrección del disparo/mensajes del reviewer. El alcance de agentes por proyecto y los presets de sandbox componibles, compartidos entre repos o integrados siguen pendientes; no se presentan como resueltos en esta publicación.
