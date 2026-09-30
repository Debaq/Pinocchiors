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

- ⬜ Editor de esqueleto: añadir hijo, borrar hueso, guardar presets propios,
  importar/exportar el esqueleto en JSON (la lib ya tiene
  `load_skeleton_json`/`save_skeleton_json`).
- ⬜ Formas de cuerpo: "árbol libre" (cadenas ramificadas sin anatomía) y
  apéndices cuernos (fijos) y mandíbula. Variantes incompletas: elefante sin
  colmillos, calamar sin los 2 tentáculos largos ni aletas del manto, delfín
  sin caudal horizontal.
- ⬜ Proporciones por especie (patas cortas y gruesas del elefante: la rodilla
  de la plantilla cae a la altura de la panza).
- ⬜ Limitación de la cadera en el ajuste (#19 de la auditoría): en el
  humanoide sintético queda en y≈0.42 (real 0.47); no hay señal geométrica.

## 3. Pose y animación

Principios que siguen valiendo: los formatos de salida solo entienden FK
(todo se hornea al exportar); hueso de deformación ≠ control; una sola pila
por cuadro (keys FK → restricciones → IK → límites); todo se deshace y se
guarda en el `.pinocchio`; genérico por especie.

- ⬜ Pose de prueba encadenada en el paso Esqueleto (hoy "Probar la pose"
  guarda una sola articulación).
- ⬜ La barra de estado no avisa cuando la articulación viola su límite.
- ⬜ **Restricciones (F3)**: la etapa existe vacía en la pila (`rig.ts`).
  Copiar giro/posición/escala, hijo de (cambiar de padre en el tiempo),
  seguir (damped track), estirar hacia, mapeo de transformación, drivers
  (un deslizador "cerrar mano" dobla los dedos), reparto de giro en la
  columna. Cada una con influencia animable. El grafo de relaciones del
  panel de articulación debería mostrarlas y crearlas al conectar.
- ⬜ **Captura (F6)**: MediaPipe (`@mediapipe/tasks-vision`) desde archivo de
  video y webcam, en un worker; filtro 1€, huecos, cambios de lado,
  calibración, contactos → keys de fijado, puntos → giros con IK, límites,
  reducción de keys; UI de grabación; títeres para otras especies. Pregunta
  abierta: modelos incluidos (offline) o descargados la primera vez.
- ⬜ **Importar BVH** (exportar ya existe).
- ⬜ **Retargeting (F7)**: pasar un clip entre esqueletos con mapeo automático
  por `ChainKind`, lados y nombres, tabla editable, compensar proporciones,
  plantillas guardadas.
- ⬜ **Movimiento secundario (F8)**: huesos resorte, seguimiento con retardo,
  ragdoll, colisiones con el suelo y el cuerpo, terreno irregular.
  Procedurales: el preset "Reposo" respira y mira alrededor; falta parpadeo.
- ⬜ **Deformación (F9)**: dual quaternion, correctivos por ángulo, shape keys
  (la escena no las modela), huesos segmentados, preservación de volumen.
- ⬜ **Exportación (F10)**: la mezcla de capas no se hornea al exportar (solo
  con el botón); los controles se descartan (opción de exportarlos como
  nodos vacíos); límites y metadatos en `extras` de glTF y atributos USD.
- 🟨 Rigidez del límite dibujada como degradado (hoy es una línea interior).
- 🧪 Panel de articulación sin probar con la diseñadora.

## 4. Interfaz

- ⬜ **Outliner**: los nodos del archivo son de solo lectura porque al
  importar todas las primitivas se unen en una malla.
  - Guardar por triángulo el nodo de origen (rangos por primitiva en
    `AppState`).
  - Ocultar/mostrar un nodo y sus hijos en el visor.
  - Seleccionar un nodo desde el outliner y resaltarlo (y al revés).
  - Ver la transformación del nodo en la pestaña Objeto.
  - Borrar un nodo (de la malla unida y de la escena exportada).
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

## Notas del README a corregir
Se corrigen junto con este compilado: MSRV 1.88, comandos de UV que ya no
existen y enlaces a los roadmaps borrados.
