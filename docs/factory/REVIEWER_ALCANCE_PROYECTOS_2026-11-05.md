# Reviewer: alcance del proyecto padre y sus hijos

## Objetivo y causa

Corregir `valid_repository_or_project_required` del Kasymir AppUI Reviewer sin
cambiar su configuración. La revisión 8 seleccionaba el proyecto `kasymir`.
El resolver consultaba únicamente `code_projects.project_id` del padre:
no recorría los hijos ni reconocía su vinculación canónica histórica por
nombre exacto dentro de la misma organización. La conclusión inicial de
que faltaban repositorios era incompleta: los hijos sí tenían índices.

## Corrección

`apps/backend/src/db/autonomous_scope.rs` usa un CTE recursivo con `UNION`:

- Incluye padre, hijos y descendientes; termina incluso si hay ciclos.
- Exige la misma organización y proyectos activos en cada nivel; poda ramas archivadas.
- Admite el vínculo explícito y, únicamente cuando `project_id IS NULL`, la
  asociación canónica exacta `projects.name = code_projects.name` existente en Nexus.
- Un vínculo explícito conflictivo nunca se sustituye por nombre. No se infieren
  asociaciones por nombre del repositorio, similitud, inventario del workspace o entrada del run.
- Excluye índices archivados/no GitHub y deduplica repositorios normalizados.

Sin migración, backfill de vínculos, reindexación ni cambio de permisos, modelos o schedules.

## Comprobaciones y despliegue

- Implementación desplegada: `1a9b4603f6feee69139f45a01e62b98959bcddac`.
- Release: https://github.com/smart-coder-labs/nexus-mind/actions/runs/42065054822
- Siete jobs exitosos; 1869 tests de biblioteca aprobados y 4 ignorados,
  26 de integración, 70 Python y 134 UI; check all-targets y builds aprobados.
- Las dos regresiones fallaban antes del arreglo y pasan después. Cubren
  autorización, validación, reconciliación y enqueue desde hijos/nietos,
  además de organización, archivos, ciclos y precedencia del vínculo explícito.
- Backend/worker: `sha256:ff7136ec26ffb1c1e9da7c62d7ef0c44986aab7b115c140b7f62112e39480336`.
- Backup SQLite verificado por el release: `/data/pre-workflows-20261105T164928Z.db`.
- Backend y egress desplegados; health público devuelve `status=ok`, `db=ok`.

## Estado real y siguientes pasos

La validación por API de la revisión **8** devuelve **valid**. El padre resuelve
`kasymir/kasymir-app-ui` y `kasymir/kasymir-discounts-service`, coincidiendo con
los índices activos de sus hijos. El estado **disabled** se conservó: no se
habilitó automatización ni se encoló una revisión. Las claves temporales del
operador se revocaron en `finally`.

La habilitación queda como decisión separada. Este arreglo no demuestra una
revisión/modelo live ni repara el CI fallido de `9580cf0`; los servicios integrados
pendientes de Factory tampoco forman parte de este cambio.

Aprendizaje: cero vínculos por FK directo no significa cero repositorios en la
jerarquía. Revisar el árbol y el contrato de asociación existente antes de
atribuir el fallo a la configuración del usuario.
