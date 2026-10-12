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
6. [~] Esqueleto, Animar, Pose hechos; faltan Luces e Historial (párrafos con piezas sueltas).
7. [ ] Revisión: ningún párrafo `leading-relaxed` suelto en los paneles
   laterales; `tsc` y pruebas de i18n.
