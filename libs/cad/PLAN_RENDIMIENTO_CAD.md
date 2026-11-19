# Plan: CAD más ágil

2026-11-18. Auditoría de código (sin perfilar) del motor (`libs/cad`), el
puente (`apps/desktop/src/cad.rs`, `apps/web/src/lib/cad.ts`) y la interfaz
(`CadView.tsx`, `CadViewer.ts`, `DesignStep.tsx`). Sigue a
`apps/PLAN_RENDIMIENTO.md` (R1–R12), que ya arregló el grueso del puente.

Lo que ya estaba bien: caché por operación encadenada (cambiar la N no rehace
0..N-1), comandos pesados fuera del hilo principal, mallas en binario,
booleanas en paralelo, teselado en paralelo, arrastre de puntos con deltas.

## Fase 1 — cambios chicos que se notan mucho

- **F1.1 Caché de operaciones que no se vacía sola.** `EvalCache` guardaba 128
  pasos para todas las pestañas y botaba la cuarta parte menos usada: con más
  de 128 operaciones (un diseño, o sumando las pestañas de un ensamble) se
  botaba el comienzo de la cadena y cada cambio recalculaba todo. Ahora: más
  capacidad y nunca se bota lo que usa la evaluación en curso.
- **F1.2 Caja en vez de masa.** ~30 lugares de `eval.rs`/`assembly.rs` usaban
  `mass()` (integrales de volumen, segundos en superficies libres) solo para la
  caja: pasan a `bounds()`.
- **F1.3 Revisión de sólido en paralelo.** `wrap_checked` (25 operaciones) usa
  `BRepCheck_Analyzer` con hilos.
- **F1.4 Visor: no rearmar el cuerpo si no cambió.** El efecto de piezas de
  `CadView` llamaba `setParts` en cada cambio del documento; `setParts` ahora
  compara. Vértices sin repetir con tabla hash (era O(V²)) y aristas en
  `Float32Array` sin `push(...slice)`.
- **F1.5 BVH en el cuerpo del CAD** (`ensureBvh`): pasar el mouse por piezas
  grandes.
- **F1.6 Pestañas sin congelar:** `cad_tab_*`, `cad_tabs_sync/merge`,
  `cad_tab_document/raw` fuera del hilo principal.
- **F1.7 Sin versiones guardadas en cada envío:** la vista previa y el
  deshacer no copian `versions`.
- **F1.8 Hover de caras sin pedidos repetidos:** `cad_face_info` en vuelo se
  recuerda.

## Fase 2 — respuesta

- Recálculo sin bloquear consultas (soltar `cad_cache` al evaluar/teselar) y
  cancelar el recálculo viejo si llega otro cambio (generación).
- Contador de versión del documento en vez de serializar para la huella;
  `design_tabs` sin clonar todo en ensambles; `prepared()`/`exact_scan_mesh`
  con caché.
- Sketch: hover cambia colores sin rehacer el sketch; imán una vez por cuadro;
  polilíneas de splines/elipses por versión; mover una entidad entera por el
  camino rápido; sketches iguales no se vuelven a resolver (caché propia).
- Consultas de caras/aristas en lote en C++ (`resolve_face`, `edge_ref`,
  `part_of_face` sin rearmar mapas); `cad_face_infos` en lote.
- `fingerprint` sin JSON; `diag()` recordado.

## Fase 3 — grande

- Piezas repetidas (instancias, patrones) comparten forma y malla.
- Malla por pieza: rehacer solo las piezas que cambiaron; detalle según el
  tamaño.
- Arrastre de gizmo/cota con malla gruesa, fina al soltar.
- Solver disperso y grados de libertad una vez por arrastre.
- Planos: mover una vista sin regenerar todo el SVG.
- Colaboración: envío con pausa (último gana), sin `versions`.
- Autosave: saltar si nada cambió antes de capturar.

## Avance

- 2026-11-18 Fase 1 escrita:
  - F1.1 `EvalCache`: capacidad 512 y `epoch` (lo tocado por el recálculo
    en curso se bota al último).
  - F1.2 `BoxOf::bbox()` en `geom.rs` (sobre `Shape::bounds`); 25 usos en
    `eval.rs` y 2 en `assembly.rs`. Los que leen volumen siguen con `mass()`.
  - F1.3 `BRepCheck_Analyzer(s, true, true)` (paralelo) en `wrap_checked`,
    `cad_is_valid` y la superficie recortada.
  - F1.4 `setParts` compara (`samePartStyles`); vértices con `Set` de claves;
    `edgeSegments` con `Float32Array` justo; `paintBody` con color por cara
    (era triángulos × piezas en cada cambio de preselección).
  - F1.5 `hitBody` con `ensureBvh` (se arma al primer pick tras cada
    rearmado; desde 20 k triángulos).
  - F1.6 comandos de pestañas `async` con `in_background`.
  - F1.7 solo la vista previa va sin `versions` (en `cad_set_document` el
    backend guarda lo que llega: quitarlas ahí las perdería).
  - F1.8 `planarFaces` marca la cara en vuelo.
  - Centro de gravedad: no se calcula con la opción apagada.
- 2026-11-18 Fase 2 (parte escrita):
  - Sketch en la interfaz: `setSketchHighlight` (hover/selección rehacen solo
    líneas y puntos, no regiones/calcos/rejilla; curvas por sketch en
    `shapeOf`); el efecto base de `CadView` lee lo resaltado con `untrack`.
    Imán una vez por cuadro (`sketchMove` con rAF; `flushSketchMove` en
    pointerdown/up). `farFromPoly` descarta polilíneas lejanas por su caja.
    `hitAll` con caché por sketch (WeakMap). Pick de vértices sin clones.
    `dropGroup` libera cualquier objeto.
  - Motor: `fingerprint` sin texto JSON (`HashSink`); `resolve()` copia sin
    `versions` (`without_versions`); `Shape::face_distances`/`edge_distances`
    por lote (`cad_sub_distances`, el mapa una vez) en `resolve_face` y
    `resolve_edge`; `Evaluation::edge_refs` por lote (usado en
    `cad_edge_refs` y proyectar cara).
  - `Sketch::solve_quick` (sin diagnóstico si se cumple) en el arrastre de
    una entidad entera; `solve_drag` igual que antes.
  - Pendiente de la fase 2: caché de sketches iguales, contador de versión /
    `design_tabs` sin clonar, recálculo sin bloquear y cancelable, `diag()`
    recordado (falta una identidad de forma barata en `cad_occt`).

