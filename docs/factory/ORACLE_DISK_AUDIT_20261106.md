# Barrido de disco Oracle — 2026-11-06

Auditoría de solo lectura solicitada por el usuario. No se borraron imágenes,
volúmenes, backups, cachés, logs, checkouts ni ramas. Los valores son aproximados
salvo los recuentos/sumas indicados. Las imágenes k3s variaron por GC automático.

## Estado

- Raíz ext4: 189 GiB. Al comenzar: 175 GiB usados, 15 GiB disponibles (93%).
- Durante el barrido: 176 GiB usados/14 GiB disponibles; al cierre, tras GC
  automático: 161 GiB usados/29 GiB disponibles (85%).
- Inodos: 4,2 millones usados de 25 millones (18%); no es agotamiento de inodos.
- Al cierre: `DiskPressure=True`, transición 22:47:14 UTC; API HTTP 503.
- Kubernetes volvió a expulsar backend/worker y MCP por ephemeral-storage.
  Pod nuevo del backend `nexusmind-backend-6f4c94b646-kcrc4` pendiente. No se
  alteraron taints, umbrales ni despliegues para forzar scheduling.
- Los cambios de espacio libre no fueron limpiezas nuestras: se observaron
  después de expulsiones/GC de Kubernetes. El margen puede volver a consumirse
  al descargar/descomprimir imágenes.

## Consumo principal

| Categoría | Tamaño medido | Ubicación / detalle |
|---|---:|---|
| Almacén Docker | ~46 GiB físicos | `/var/lib/containerd`: snapshots ~36 GiB + blobs 9,5 GiB; `/var/lib/docker` solo 336 MiB |
| Almacén k3s/containerd | ~31 GiB | `/var/lib/rancher/k3s/agent/containerd`: snapshots 25 GiB + blobs 6,3 GiB; variable por GC |
| Backups completos SQLite en PVC | **43,68 GiB / 28 archivos** | 23 copias en raíz: 34,76 GiB; 5 en `maintenance-backups`: 8,92 GiB |
| Checkouts/estado del runner | ~16 GiB | `/home/ubuntu/.local/state/nexusmind/kasymir-runner` |
| Caché npm del backend | 6,4 GiB | `$NEXUS_DATA/claude-home/.npm` |
| Prometheus | 6,1 GiB | PVC `monitoring_prometheus-server` |
| Journal systemd | ~4 GiB | `/var/log/journal`; `/var/log` total 4,2 GiB |
| Otras copias SQLite en home | ~3,54 GiB / 3 archivos | `/home/ubuntu/nexusmind-backups`; inodos distintos de las copias del PVC, no hardlinks |
| Base SQLite activa | **1,83 GiB** | `$NEXUS_DATA/nexusmind.db`; WAL ~13 MiB, no es la causa principal |
| Caché npm Ubuntu | ~920 MiB | `/home/ubuntu/.npm` |

`NEXUS_DATA=/var/lib/rancher/k3s/storage/pvc-b127b392-6dc7-4091-a325-2290e8522baf_nexusmind_nexusmind-data`.

El PVC NexusMind suma ~53 GiB e incluye backups, base activa, `claude-home`
(~6,8 GiB) y embeddings (~523 MiB). El conjunto de PVC de k3s suma ~60 GiB,
incluido Prometheus. Estos totales **no se suman otra vez** a sus componentes.
Docker y k3s son dos almacenes distintos; no se debe sumar además el tamaño
virtual de cada imagen, porque comparten capas dentro de su almacén.

## Recuperable declarado por Docker

- 140 imágenes, 48,65 GB lógicos; Docker estima 27,49 GB recuperables.
- Una imagen/contenedor en uso al primer muestreo; posteriormente ninguno.
  Esto NO demuestra que las otras imágenes no estén referenciadas por perfiles,
  configuración de SDK o despliegues futuros.
- Build cache: 59 entradas, 5,28 GB totales; **3,174 GB recuperables**.
  Parte comparte capas con imágenes; ya está incluida en el almacén, no es
  espacio adicional para sumar.
- Tres volúmenes Docker: ~89,95 MB; no son el problema y no se tocaron.

## Backups y ejecuciones

En total hay **31 copias SQLite completas, ~47,22 GiB**, frente a una base activa
que ocupa 1,83 GiB. Son copias de distintas intervenciones entre el 2 y el 6 de
noviembre, sin una retención acotada evidente. Parte de esta acumulación viene
de nuestras copias preventivas en despliegues y reparaciones. No se han comparado
hashes de todas las copias: no se afirma que sean idénticas ni prescindibles.

Entre los checkouts del runner hay ejecuciones de 3,6 GiB, 3,5 GiB, 2,0 GiB,
2,0 GiB y 1,8 GiB. La caché `.node-cache` de ese directorio es solo 169 MiB:
el volumen principal no es esa caché. La consulta final de estados de las
 ejecuciones falló porque el pod volvió a ser expulsado; no se clasifican sus
checkouts como seguros de eliminar sin reconciliar estados/evidencias.

## Orden propuesto, sin ejecutar

1. Caché de build no usada: ~3,17 GB, con autorización explícita.
2. Cachés npm: ~7,3 GiB entre backend y Ubuntu; comprobar consumidores activos.
3. Retención del journal: potencial de varios GiB conservando una ventana útil.
4. Mayor ahorro: política de backups, verificar/restaurabilidad y trasladar
   copias seleccionadas fuera del nodo antes de eliminar excedentes.
5. Revisar imágenes Docker por referencias reales; no hacer prune general.
6. Retención de checkouts solo tras revisar estado y necesidades de reintento.

No borrar directorios `containerd` a mano, ni PVC, base activa, WAL, imágenes
referenciadas, backups o evidencias como si todo fuera caché.

## Persistencia de sesión

Goal: explicar el consumo con un barrido de solo lectura.
Accomplished: tamaños físicos y lógicos separados, 31 backups completos,
checkouts, npm, Prometheus, journal y estado de presión identificados.
Discoveries: Docker usa snapshots en `/var/lib/containerd`, no principalmente
`/var/lib/docker`; la mayor acumulación persistente evitable son backups completos.
Next Steps: autorizar una limpieza acotada y establecer retenciones; recuperar
estabilidad antes de nuevos clones. No se ejecutó ninguna limpieza.
Relevant Files: este informe; rutas de datos y cachés enumeradas arriba.
La API NexusMind estaba en 503 al cierre; persistir esta sesión en memoria remota
cuando recupere disponibilidad.
