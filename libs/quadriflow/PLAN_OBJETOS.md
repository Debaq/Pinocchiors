# Plan: retopología por objeto + menús laterales con globitos

Pedido (2026-10-12): el cráneo desarticulado (86 piezas, 16 materiales) pierde
piezas al retopologizar entero (quedan ~29) y las caras se reparten solo por
área. Además los menús laterales derechos tienen demasiado texto.

Un objeto = un material de origen (mismo criterio que el desplegado UV).

## A. Retopología por objeto (en este orden)

1. [x] `quadriflow-core`: `remesh_groups(mesh, face_group, config, progress)`:
   parte la malla por grupo, reparte las caras objetivo por área con un piso
   por objeto, retopologiza cada uno y junta los quads (vértices y caras en el
   orden de los grupos). Un objeto que falla o queda vacío se devuelve en el
   informe, no se descarta en silencio.
2. [x] Tests en `core/tests`: dos objetos separados conservan los dos; el
   reparto suma el objetivo (±); el piso evita objetos vacíos.
3. [x] App: `run_retopology` calcula el grupo de cada cara con
   `uv_core::face_groups` (≥ 2 grupos → por objeto), progreso "Objeto i de n".
   `RetopologyConfig.per_object` (por defecto sí).
4. [x] UI: casilla "Por objeto" en el panel de Retopología; tras correr, tabla
   corta de quads por objeto en el informe.
5. [x] Prueba real con el cráneo (30 000 quads): piezas de salida = piezas de
   entrada, tiempo, calidad. Comparar contra la corrida entera (54 s, 29 de 86
   piezas).
6. [x] Commit.

Resultado cráneo (30 000 quads, 16 materiales): por objeto 36 s, los 16
objetos presentes, 29 318 quads; entero 160 s y ~29 de 86 piezas.

## B. Menús laterales: menos texto, más globitos (en este orden)

Patrón: los párrafos `text-xs text-text-muted` pasan a `Hint` (ícono ⓘ con
globito), las etiquetas se acortan, los botones llevan `hint` (globito con lo
que hacen). Cada cambio de texto actualiza el catálogo `en.*.ts`.

1. [x] `Button` y `Checkbox`/`Slider`: prop `hint` que envuelve en `Tooltip`.
2. [x] Retopología (`RetopologyStep`, `RetopologyPanel`) — se ve en A.4.
3. [x] UV (`UvStep`, 7 párrafos).
4. [x] Remallar (`RemeshStep`, `SimplifyPanel`, `SmoothPanel`, `IsotropicPanel`,
   `VoxelPanel`, `QuadsPanel`).
5. [x] Reparar, Escanear, Fabricar, Materiales, Vista.
6. [x] Esqueleto, Animar, Pose, Luces e Historial hechos (2026-10-16).
7. [x] Revisión (2026-10-16; quedan avisos de estado, «sin modelo» y los de diálogos, Escáner y Diseñar): ningún párrafo `leading-relaxed` suelto en los paneles
   laterales; `tsc` y pruebas de i18n. Hoy quedan `leading-relaxed` en `UvStep`, `RemeshStep`,
   `SkeletonStep`, `AnimateStep`, `RigPanel` (3), `IkPanel`, `ObjectTab` (2), `LibraryPanel`,
   `MaterialPanel`, `LightsPanel` e `HistoryPanel` (algunos son avisos de «sin modelo» y pueden quedarse).

## C. Los objetos siguen existiendo después de la retopología (en este orden)

Reporte (2026-10-12): tras retopologizar un modelo de varias piezas ya no se
eligen sus piezas por separado, Materiales muestra un solo objeto, el Outliner
no despliega y la malla original no aparece tachada.

1. [x] Backend: los datos de la malla de quads llevan el objeto (material de la
   piel) de cada grupo (`groupNodes`), como la malla original lleva su nodo.
2. [x] Outliner: «Malla» tachada cuando hay retopología; «Quad Mesh» se
   despliega con un hijo por objeto (nombre y quads).
3. [x] App: en el visor de quads se eligen objetos (clic en el Outliner,
   doble clic y recuadro en el visor); se resaltan; ocultar/mostrar por objeto.
4. [x] Materiales: con objetos de quads elegidos muestra sus materiales (editar
   color, textura…); asignar/quitar material no aplica a la piel.
5. [ ] (falta probar en la app real, sin e2e hasta que se pida) Probar con el cráneo y con un modelo de una sola pieza (no cambia nada).
6. [x] Commit (`b394b72`, 2026-10-12).
