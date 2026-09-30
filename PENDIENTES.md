# Pendientes

Compilado del 2026-09-30. Reúne lo que seguía abierto en los planes y
roadmaps del repositorio (`PLAN_MEJORAS.md`, `todo_fix.md`,
`apps/PLAN_GUI.md`, `apps/ROADMAP_POSE.md` y los `ROADMAP.md` de `libs/`),
después de contrastar cada punto con el código. Lo que ya estaba hecho no se
repite; esos documentos siguen en la historia de git
(`git log --all -- '*ROADMAP*.md' '*PLAN*.md' todo_fix.md`).

Leyenda: 🐞 error comprobado · ⬜ falta · 🟨 a medias · 🧪 falta probar.

---

## 1. Errores comprobados

- 🐞 **Impresión 3D, vecinos de las piezas.** Después de `subdivide`, la app
  llama a `find_neighbors` (`apps/desktop/src/commands.rs`, `subdivide_mesh`),
  un stub que marca vecinas las piezas ±1 por índice, y pisa los vecinos que
  `subdivide` ya calculó bien por caja (`detect_neighbors` en
  `print3d/src/slicer.rs`).
- 🐞 **"Optimal (menos cortes)"** se ofrece en `Print3DPanel.tsx` pero
  `SubdivideStrategy::Optimal` usa la grilla (`slicer.rs`).
- 🐞 **Cierre de los cortes**: solo se cierra un contorno por corte
  (`slicer.rs`, "Por ahora solo manejamos un loop"); piezas con varias islas
  o agujeros en el corte quedan abiertas.
- 🟨 `SubdivideConfig::max_dimension` y `overlap` no se leen; `cut_faces`
  siempre vacío; `LabelingScheme::Coordinate` genera `P{n}`;
  `orient_flat_on_bed` está vacío; `calculate_connections` deja área, normal
  y centro en 0; `AssemblyInfo.total_volume` es 0.

## 2. Esqueleto

Hecho el 2026-09-30: editor de estructura (agregar, borrar y renombrar
huesos; JSON en el formato de `load_skeleton_json`; esqueletos propios
guardados), forma "árbol libre", cuernos, mandíbula, colmillos, tentáculos
largos y aletas del manto (calamar), caudal horizontal (delfín), largo de
patas por especie (elefante 0.8, jirafa 1.3).

- ⬜ Limitación de la cadera en el ajuste (#19 de la auditoría): en el
  humanoide sintético queda en y≈0.42 (real 0.47); no hay señal geométrica.
- 🧪 Las variantes nuevas no se probaron ajustándolas a modelos reales.

## 3. Pose y animación

Principios que siguen valiendo: los formatos de salida solo entienden FK
(todo se hornea al exportar); hueso de deformación ≠ control; una sola pila
por cuadro (keys FK → restricciones → IK → resortes → límites); todo se
deshace y se guarda en el `.pinocchio`; genérico por especie.

Hecho el 2026-09-30:
- Pose de prueba encadenada en el paso Esqueleto.
- Aviso en la barra de estado cuando la animación pasa el límite.
- Restricciones (F3): copiar giro/posición, hijo de, seguir, estirar hacia,
  mapeo de giro, driver y reparto de giro, con influencia animable, panel
  en la pestaña IK, filas en la línea de tiempo y aristas en el grafo de
  relaciones (arrastrar un hueso sobre otro crea una).
- Importar BVH y retargeting (F7) con mapeo automático por nombre y por
  cadenas, tabla editable y plantillas guardadas; detecta esqueletos
  espejados (las plantillas propias ponen `_l` en −X).
- Captura desde video con MediaPipe (F6): filtro 1€, huecos, raíz desde la
  imagen, contactos de los pies como fijado del IK.
- Resortes y seguimiento (F8) con suelo, resortes automáticos.
- Piel con cuaterniones duales en el visor (F9) y rigidez del límite como
  degradado.
- Exportación (F10): la mezcla de capas se hornea; límites y datos del rig
  en `extras` de glTF.

Pendiente:
- ⬜ Captura: webcam en vivo (en Linux WebKitGTK hay que habilitar
  `getUserMedia`), detección en un worker, cámara del Revopoint, varias
  cámaras, cara y manos (Face/Hand Landmarker), recortar el tramo útil antes
  de convertir. Los "títeres" funcionan con la tabla de mapeo del
  retargeting (brazos del actor → patas delanteras, etc.), sin plantillas
  por BodyPlan.
- ⬜ Estirar hacia: sin preservación de volumen (el hueso no tiene escala).
- ⬜ Ragdoll, colisiones con el propio cuerpo, terreno irregular, parpadeo
  (no hay huesos de párpados).
- ⬜ Correctivos por ángulo, shape keys (la escena no las modela: hay que
  agregarlas a `converter-scene`, glTF y USD), huesos segmentados,
  preservación de volumen.
- ⬜ Exportar los controles como nodos vacíos (hoy van como datos en
  `extras` de la raíz, no como nodos).
- 🧪 El modelo de MediaPipe no se pudo bajar en este entorno: la detección
  real sobre un video no se probó (sí la tubería con puntos sintéticos).
- 🧪 Panel de articulación sin probar con la diseñadora.

## 4. Interfaz

- ✅ **Outliner** (2026-09-30): los nodos del archivo se ocultan, se eligen
  (también con doble clic sobre la malla), muestran su transformación local
  en la pestaña Objeto y se borran (sale su geometría de la malla y de lo
  exportado). Queda: editar la transformación (hoy solo lectura), ocultar
  también en el alambre y en la vista de pesos, deshacer el borrado.
- ⬜ Arrastrar y soltar para importar; archivos recientes; vista previa de la
  información antes de importar.
- ⬜ Exportar USDA en texto; opciones USD en la interfaz (escala, eje arriba,
  separar ORM).
- ⬜ Impresión 3D: cortes manuales, conectores (la lib genera dowel,
  dovetail y pirámide; puzzle y terraza no; aplicarlos requiere CSG), vista
  de piezas y planos en el visor, candado de proporción por eje.
- ⬜ Cuatro vistas, ajuste a grilla/vértices, atajos configurables, menús
  Editar/Vista/Malla.
- ⬜ Tags y categorías en la biblioteca de poses.
- ⬜ Overlay lado a lado original/retopología.
- ⬜ Rendimiento: decimar la vista previa de mallas > 100k, workers, LOD.
- ⬜ Tests de integración y E2E (Playwright).
- ⬜ Avisos de estilo de clippy (~30; se necesita red hacia crates.io para
  compilar). CI no corre clippy.
- 🧪 Nada de la app se probó en el escritorio real en las últimas rondas:
  abrir, guardar, cerrar con cambios, importar, indicador de cambios sin
  guardar, escáner, panel de retopología.

## 5. Librerías

### Retopología (`libs/quadriflow`)
- ⬜ Probar personajes reales (humanoide con manos) en el banco antes de
  decidir 2A y 4.
- ⬜ Propuestas, en este orden:
  1. Encarecer en `integer.rs` que las cargas de posición crucen zonas con dos
     aristas vivas a menos de dos quads (causa la mayoría de los plegados en
     CAD). Esfuerzo medio, riesgo bajo.
  2. (2A) Simplificación de pares 3-5 junto a singularidades de orientación
     (Bozzo y Tarini). Esfera: 14 irregulares, mínimo 8.
  3. (4) Densidad adaptativa por niveles (1×, ½×, ¼×) con transiciones 2:1;
     hoy la adaptativa continua sube los irregulares y es opcional.
  4. (3) Dual contouring con QEF en `rebuild.rs` para recuperar aristas vivas
     al reconstruir STL rotas.
  5. (5) Chaflanes angostos en el criterio de tamaño (después de 4).
- ⬜ Costuras de UV: islas angostas (unir a la vecina) y suavizar la
  dirección de la restricción.
- 🟨 "Determinista con cualquier número de hilos": el test no varía los hilos.
- Límites matemáticos (no son errores): una esfera necesita 8 irregulares;
  un rasgo más angosto que un quad no se representa sin quads más chicos;
  toda transición de tamaño necesita irregulares.

### UV (`libs/uv`)
- 🧪 La distribución por partes solo se midió en personajes sintéticos.
- ⬜ Lado escondido sin frente conocido: la cabeza se abre por abajo; usar el
  frente del esqueleto.
- ⬜ Los nombres de las guías pierden las tildes (`converter/layers/raster.rs`).

### Conversor (`libs/converter`)
- ⬜ FBX; morph targets reales (hoy solo canales `weights` sin targets);
  emulación de doble cara en USD por duplicado; exportar PLY en ASCII.

### Reparación (`libs/pinocchio/repair`)
- ⬜ Resolver cuerpos solapados y auto-intersecciones (hoy solo se detectan).
- ⬜ Uniones en T entre piezas.
- 🟨 Parches de agujeros con UV extrapolada; se pierden skin y jerarquía.
- ⬜ Feature `parallel` declarada sin código; índices `u32`.

### Auto-rigging (`libs/pinocchio`)
- ⬜ Validación: comparar con el Pinocchio C++, suite estándar de mallas,
  métricas de skinning, benchmarks con `criterion`.
- ⬜ API WASM propia (la feature `wasm` no tiene código), GPU (`wgpu`), FBX.
- ⬜ Tests de volumen, área y centro de masa en `print3d`; `prepare_for_printing`.

---

## Rust sin compilar

En esta sesión crates.io estaba bloqueado: el Rust nuevo solo se revisó con
`rustfmt` (sintaxis) y a mano. Correr `cargo test --workspace` antes de
integrar. Toca: `apps/desktop/src/commands.rs` (`set_skeleton_bones`,
`write_text_file`, `remove_scene_node`, `node_extras`, nodos por grupo en
`get_mesh_data`, campos nuevos de `BodyPlanDto`), `apps/desktop/src/lib.rs`,
`libs/pinocchio/skeleton/src/body.rs` (formas y apéndices nuevos, con
tests), `libs/converter/gltf-io` (`node_extras`, con test) y
`libs/converter/core/src/options.rs`.
