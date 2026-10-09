# Plan: una sola escena con objetos (2026-10-07)

Pedido del usuario: que Diseñar, Preparar, Rig y Fabricar no se sientan como
programas separados en la misma ventana. Nada de botones para "iniciar un CAD"
ni para "usar el diseño como modelo": pasar de un espacio a otro es
transparente. Si se modifica la malla de una pieza (reparar, escalar, cortar…),
eso queda colgando del objeto en el Outliner, y si después cambia el diseño, las
modificaciones **se rehacen solas** (como los modificadores de Blender). Las
herramientas actúan sobre **el objeto elegido en el Outliner**.

## Cómo está hoy

- El backend tiene un único modelo activo (`AppState`: escena, malla, esqueleto,
  quads, pesos, piezas de impresión…) y aparte el documento del CAD.
- Deshacer guarda copias completas del estado (`project::capture`/`restore`) y
  de la interfaz (`projectUi`/`restoreProjectUi`), y las intercambia.
- "Usar como modelo" convierte el sólido entero en el modelo y reemplaza lo que
  hubiera; no queda vínculo.

## Diseño

- **Objeto** = un modelo completo (lo mismo que hoy es el estado activo) con un
  origen: importado o pieza del CAD (`PartId`). El activo vive en `AppState`
  como hasta ahora; los demás se guardan como `ProjectState` en `AppState.objects`.
  Cambiar de objeto = guardar el activo en su casillero y restaurar el otro (el
  mismo intercambio que deshacer). La interfaz guarda por objeto su `projectUi`
  y su historial.
- **Pieza del CAD → objeto**: cada pieza del diseño aparece como objeto. Su
  malla se genera sola (sin aviso) cuando se la elige fuera de Diseñar o se
  entra con ella elegida a Preparar, Rig o Fabricar. Se guarda una huella de la
  pieza para saber si quedó vieja.
- **Modificaciones que se rehacen**: cada paso deshacible que cambia la geometría
  guarda en el historial los comandos del backend que corrió y sus parámetros.
  Si la pieza cambió en el diseño, se regenera la malla y se vuelven a correr
  los pasos activos del historial (camino de la raíz al actual). El esqueleto se
  conserva; los pesos se recalculan si los había.
- **Outliner**: debajo de cada objeto, la malla generada y sus modificaciones.

## Fases (todas hechas el 2026-10-07)

1. Diseñar sin "Nuevo diseño": entrar a Diseñar crea el documento vacío.
2. Objetos múltiples: `project::ObjectSlots` (casilleros con el `ProjectState`
   de cada objeto inactivo, ya en MessagePack), `object_activate/remove/adopt`.
   Las copias para deshacer y los casilleros guardan solo el modelo
   (`capture_model`/`restore_model`); el proyecto lleva todo. En la interfaz,
   `lib/objects.ts` y `activateObject` en App (guarda `projectUi(false)` y el
   historial del que sale; `restoreProjectUi(…, true)` del que entra). Importar
   o escanear crea un objeto nuevo, salvo que el activo no tenga malla (un
   esqueleto solo: sus animaciones pasan al modelo).
3. Piezas del CAD como objetos: `syncCadObjects` agrega uno por pieza;
   `cad_part_to_model(part, known)` genera la malla soldada (cerrada, como un
   STL) y devuelve su huella; no recarga si no cambió. Pasa sola al entrar a
   Preparar, Rig o Fabricar con la pieza elegida (o la abierta en la lista de
   piezas, o la primera). Se fue "Usar como modelo" (`cad_to_model`).
4. Rehacer: `undoable` graba los comandos de `REPLAYABLE` (reparar,
   retopología, UV, escalar, dividir…) en su paso del historial; los pasos
   `placement` llevan su matriz. Si la pieza cambió, `replayModifications`
   recorre `history.trail()` y los vuelve a mandar; recalcula los pesos si
   había. Las copias para deshacer de esos pasos tenían la malla vieja: quedan
   como hitos.
5. Outliner: Planos, Operaciones, Objetos; del objeto activo cuelgan su malla,
   lo hecho sobre ella (pasos de geometría del historial) y quads, esqueleto,
   pesos. Los inactivos muestran sus modificaciones del historial guardado.
6. Los objetos inactivos se ven en gris (`object_mesh_data`, en las unidades
   del activo; `Viewer3D.setGhosts`); el ojo del Outliner los oculta.

## Pendiente

- Las ediciones de esqueleto no se rehacen: el esqueleto se conserva tal cual
  sobre la malla nueva (los pesos sí se recalculan).
- El orden de los objetos es el de creación; no se pueden reordenar ni agrupar.
- ~~Las normales de la malla soldada se promedian (franjas en cilindros)~~:
  hecho 2026-10-08 con suavizado automático en el shader (`lib/autoSmooth.ts`,
  define `AUTO_SMOOTH`): donde la normal del vértice se aparta más de 10–18° de
  la de la cara se usa la de la cara. Solo en mallas con normales calculadas por
  el programa (`MeshData.group_computed_normals`); las del archivo se respetan.
- Elegir varios objetos a la vez (Fabricar con varias piezas juntas).
- Diseñar sigue con su propio visor; el plan del rediseño de la interfaz de
  Diseñar (ver ROADMAP) puede unificarlo.
