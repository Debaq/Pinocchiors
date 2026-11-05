# Plan urgente del sketch (Diseñar)

Auditoría del 2026-11-05: modelo Rust, solver, lógica web, interfaz e integración del sketch.
Marcas: **[V]** verificado leyendo el código (o con script), **[S]** sospecha a confirmar.
El orden de las fases es el de urgencia: primero lo que pierde trabajo del usuario o tumba la app.

---

## U0 — No perder trabajo (crítico)

- [x] **Guardar con el sketch abierto** [V]. El borrador vive solo en `cadUi.session()`;
  `save_project`, el autosave y la recuperación (`App.tsx:8529`, `:8940`) guardan el documento
  del backend sin él. Ctrl+S o un cierre dejan afuera todo lo dibujado desde que se abrió.
  Arreglo: antes de guardar, commit silencioso del sketch (sin cerrar la sesión) o `sketchDraft`
  en el proyecto y en la recuperación; `hasUnsavedWork` (`App.tsx:8236/8969`) cuenta `!!session()`.
- [x] **Abrir/nuevo proyecto descarta el sketch sin preguntar** [V] (`App.tsx:8581`, `:8629`,
  `cancelSketch(false)`). ConfirmDialog «Terminar / Descartar / Cancelar».
- [x] **«Terminar sketch» puede borrar el dibujo** [V] (`cadUi.ts:1177`). Cierra la sesión y el
  historial antes de `store.commit`; `commit` (`cad.ts:4218`) no aplica nada si la IA está en turno
  o se ve una versión guardada. Arreglo: cerrar la sesión solo si el commit se aplicó.
- [x] **«Descartar» sin confirmación** [V] (`DesignStep.tsx:8919`, `CadView.tsx:4757`).
  ConfirmDialog si `sketchHistory().undo > 0`.
- [x] **Sketch abierto al salir de Diseñar** [S] (`App.tsx:586`): sigue vivo e invisible y bloquea
  a la IA. Terminar o avisar al cambiar de espacio.
- **Hecho 2026-11-05**: `save_project`/`project_changed` reciben `design` (el diseño con el sketch
  abierto, `cadUi.documentWithSketch()`), que reemplaza la pestaña activa solo en lo guardado
  (`capture_with`, test `open_sketch_goes_into_save_without_touching_the_document`); el aviso de
  cambios mira el historial del sketch, así que abrir/nuevo/cerrar ya preguntan por
  `confirmDiscard`. `finishSketch` devuelve false y deja el sketch abierto si el documento no se
  puede cambiar (también las pestañas). «Descartar» con cambios pregunta (ConfirmDialog en
  CadView). Salir de Diseñar termina el sketch. Falta probarlo en la app.

## U1 — Que la app no se caiga ni se congele (crítico)

- [x] **Mutex envenenados** [V]. `state.rs:10` usa `std::sync::Mutex` y `cad.rs` hace ~70
  `.lock().unwrap()`. Un panic con el lock tomado (solver, regiones, evaluate) deja todos los
  comandos CAD rotos hasta reiniciar. Arreglo: `parking_lot::Mutex` o `unwrap_or_else(into_inner)`
  + `catch_unwind` alrededor de evaluate/solve.
- [x] **Solve en el hilo principal** [V]. `cad_solve_sketch`, `cad_sketch_suggest`,
  `cad_sketch_define` (`cad.rs:941/1005/1012`) son comandos síncronos: en Tauri v2 corren en el
  hilo principal, y el solve se llama en cada movimiento al arrastrar (`cadUi.ts:518`). Arreglo:
  `async` + `spawn_blocking`; en el front un solo solve en vuelo, coalescer el último arrastre.
- [x] **`diagnose` en cada solve y arrastre** [V] (`sketch.rs:1148`): Jacobiano denso + SVD +
  nullspace O(n³). Con cientos de puntos, segundos por frame. Arreglo: saltarlo al arrastrar
  (o solo la componente tocada) y diagnosticar al soltar.
- [x] **Panics alcanzables** [V]:
  - `sketch.rs:1442`: `poles[1]` / `poles[n-2]` con B-spline de 1 polo.
  - `regions.rs:662`: `position(..).unwrap()` en el grafo de medias aristas [S].
  - `solver/bspline.rs:~127`: pesos 0 → NaN que llega al JSON (`null`).
- [x] **Un dato malo tumba el sketch entero** [V] (`sketch.rs:936-1022`, `:1387-1470`,
  `regions.rs:551`): una restricción a una entidad de otro tipo o una B-spline con nudos
  inconsistentes aborta `build()` → sin regiones → fallan todas las operaciones siguientes.
  Arreglo: saltar la restricción/entidad inválida, marcarla en `SolveReport` y avisar.
- [x] **Un subsistema que no converge congela todo el sketch** [V] (`solver.rs`
  `solve_decomposed`, `?` al primer error; `sketch.rs:1124` no escribe nada). Conservar las
  componentes que convergieron.
- [x] **`residual = INFINITY` rompe el reporte** [V] (`sketch.rs:~1155`): serde lo da `null` y no
  se vuelve a leer; además el estado sale `OverConstrained` en vez de `Failed`.
- [x] **Validar entradas**: cotas ≤ 0, NaN o inf (`sketch.rs:295` `set_value`, `lower`); MCP
  `draw` sin validar radio, lados (`sides: 1e9` → OOM) ni puntos (`mcp/tools/sketch.rs`).
- **Hecho 2026-11-05**: `crate::sync::Mutex` (no se envenena; `.lock().unwrap()` igual) y
  `AppState::after_panic` limpia los cachés del diseño cuando `in_background` atrapa un pánico.
  `cad_solve_sketch`/`cad_sketch_suggest`/`cad_sketch_define` son `async` fuera del hilo
  principal (el front ya juntaba los arrastres). Arrastre cumplido sin `diagnose`
  (`SolveReport.quick`; el front conserva lo libre y lo recalcula al soltar). Restricción
  rota o cota no válida (NaN, tamaño ≤ 0) queda afuera en `SolveReport.invalid` (y en
  `conflicting`) con aviso; `ease_large_changes` las salta. Curva que no se puede muestrear no
  tumba las regiones; sin pánico en `regions.rs` ni con B-spline de un polo; pesos ≤ 0
  rechazados. Sub-sistema que falla no frena a los demás. Residual `f64::MAX` y estado
  `Failed`. MCP `draw` valida tamaños, puntos distintos, lados ≤ 1000 y puntos ≤ 10 000.
  Pruebas: `model/tests/sketch_robust.rs` (5), `solver/tests/test_solver.rs` (2), `sync.rs` (1).
  Falta probarlo en la app. Queda de U1: `nullspace` con `max_niter = 0` [S].

## U2 — Integridad de datos del sketch (alto)

- [x] **`dropDegenerate` borra las restricciones de los patrones** [V] (`sketchEdit.ts:80`):
  `equal_offset`/`equal_rotation` repiten id a propósito (patrones de ≥3 copias); cualquier
  `mergePoint`, unir, reparar o quitar duplicados las borra y el patrón se suelta sin aviso.
  Filtrar solo los tipos donde repetir es degenerado.
- [x] **Bloque con una curva que toca el origen** [V] (`sketchBlocks.ts:46,69`): el origen sale de
  `pts` y de `map`, la definición queda con `start: undefined`. Mapear el origen o rechazarlo.
- [x] **Desarmar un bloque escalado** [V] (`sketchBlocks.ts:168`): copia las cotas a escala 1
  (largo 4 sobre línea de 8). Escalar por `t.size` u omitirlas. Además empareja por índice [S].
- [x] **Centro de patrón colgando** [V] (`sketchPatterns.ts:67`, `cad.ts:3101`): borrar la
  entidad del centro o fusionarlo deja `spec.center` inválido → `TypeError` sin capturar.
- [x] **Recortar/partir pierden marcas** [V] (`cad.ts` `trimLine`/`trimCurve`,
  `sketchTransform.ts:737`, `sketchTrim.ts:140`, `insertClip`): el tramo nuevo pierde
  `construction`, `layer`, `axis`, `infinite` → aparece geometría sólida donde había construcción.
- [x] **Partir un arco deja `arc_length` del arco entero** [V] (`sketchTransform.ts:740`) y el
  tramo nuevo sin `radius`/`equal`.
- [x] **Escalar ignora lo bloqueado** [V] (`sketchTransform.ts:591`): el radio de un círculo con
  `lock` cambia igual.
- [x] **`remove_entity` deja referencias** [V] (`sketch.rs:701`, `cad.ts:3101`): `texts`, `uses`,
  `layers`, `patterns`, `style.path`.
- [x] **`hold_text_points` pierde puntos con un error en medio** [V] (`sketch.rs:881`).
- **Hecho 2026-11-05**: `dropDegenerate` deja las cadenas de patrones (solo quita pares sin
  largo, pares iguales o centro dentro de un par). `createBlock` con el origen adentro: el
  origen del bloque es el del sketch (elegir otro da mensaje). `explodeBlock` multiplica las
  cotas por la escala, invierte ángulos con reflejo, salta horizontales/verticales si la copia
  está girada y empareja por tipo y distancia al ancla si a la copia le borraron algo.
  Centro de patrón circular: `removeEntity` no lo borra, `mergePoint` lo sigue y
  `circularPattern` da mensaje si falta (también el camino de un patrón en curva y
  `style.path` siguen al fusionar duplicados). `inheritMarks` (construcción, capa, eje,
  infinita) en recortar línea/arco/curva, partir y pegar. Partir un arco pasa `arc_length` a
  `curve_length` de los dos tramos (se implica el mismo radio por el punto común). Escalar no
  toca el radio de lo bloqueado. `removeEntity` (TS) limpia `style.path` y olvida patrones
  sin nada; `remove_entity` (Rust) limpia textos, `uses` y `style.path` y no borra anclas.
  `hold_text_points`: el solve va en `catch_unwind` y los puntos vuelven antes de seguir el
  pánico. Pruebas: `e2e/sketchIntegrity.test.mjs` (10, fallan sin el arreglo) y
  `sketch_robust.rs` (1). Falta probarlo en la app.

## U3 — Interfaz que se traba (alto)

- [x] **Arrastres sin `setPointerCapture`** [V] (`CadView.tsx:2680/2740/2745`, `onPointerUp`
  `:3094`): soltar fuera del visor deja punto/lazo/caja/recorte pegados y funde el siguiente paso
  de deshacer (`dragSaved`). Capturar el puntero y limpiar en `pointercancel`/`lostpointercapture`.
- [x] **Esc no cancela lazo, caja, trazo de recorte ni campo escrito** [V] (`resetTool`,
  `CadView.tsx:717`).
- [x] **Teclas del sketch se comen menús y botones** [S fuerte] (`CadView.tsx:3300`): Esc/Enter/
  Supr dentro de la sesión no excluyen select, menús ni diálogos (la rama del borrador sí).
- [x] **Tecla «3» (Arco 3 p.) nunca funciona** [V]: `viewForKey` (`CadView.tsx:3273`) la toma para
  la vista antes. Cambiar la tecla por defecto y reservar 1/3/7 en `SKETCH_RESERVED`.
- [x] **Teclas sin herramienta pasan a los atajos del modelo** [S] (`CadView.tsx:3275`): «S» sin
  asignar en el sketch activa Escalar del modelo. `stopPropagation` de toda tecla suelta.
- [x] **«A» (línea↔arco) fija y no reservada** [V].
- [x] **Vista previa congelada al salir el mouse** [V] (`CadView.tsx:3817`: falta `setCursor(undefined)`).
- [x] **`cycle` (Tab entre encimadas) con ids viejos** [V] (`CadView.tsx:3302`, `:1579`).
- **Hecho 2026-11-05**: el sketch captura el puntero al apretar (`setPointerCapture`) y
  `pointercancel`/`lostpointercapture` sueltan sin aplicar (`abortGesture`). Esc suelta caja
  (devuelve lo elegido), lazo, trazo de recortar y arrastres antes que lo demás; el campo
  escrito ya cancelaba por su lado. Las teclas del sketch no actúan dentro de select, menús,
  listas ni diálogos, ni con un ConfirmDialog abierto. Arco 3 p. pasa a «B»; 1/3/7 reservadas
  (vistas). Una tecla suelta sin herramienta no sigue a los atajos del modelo. El alterno
  línea↔arco usa la tecla del Arco (si se reasigna, la sigue). Fuera del visor no hay vista
  previa (salvo con el campo escrito). Tab entre encimadas descarta ids que ya no están.
  Escenario e2e escrito: «sketch que no se traba» en `e2e/cad/sketch-edicion.mjs` (sin
  correr: el e2e va cuando lo pida el usuario). Falta probarlo en la app.

## U4 — Rendimiento con sketches grandes (alto)

- [x] **Overlay entero rehecho en cada movimiento** [V] (`CadView.tsx:1096` → `CadViewer.ts:1222`):
  separar capa estática (sketch, regiones) de la vista previa; throttle a rAF de `hitTest`,
  `trimRemoved`, `extendAny` en `onPointerMove`.
- [ ] **El sketch entero por IPC en cada arrastre** [V] (`cadUi.ts:518`). Medido: 1,3 MB de
  JSON con 1125 entidades, ~3 ms en Rust; falta medir el lado de WebKit antes de cambiarlo.
- [x] **`find_regions` cuadrático** [V] (`regions.rs:586`, `:682`, `cut_crossings ~217`;
  `system.rs:219` `decompose_indexed`): hash espacial, `probe` una vez por lazo, rejilla para cruces.
- [x] **`sketch_assist` O(n²) memoria / O(n³) tiempo** [V] (`sketch_assist.rs:34,61`): por
  componente, con tope de candidatas.
- [x] Banco: sketch con 1000+ entidades (DXF, texto largo) con tiempos de solve, regiones y arrastre.
- **Hecho 2026-11-05**: banco `model/tests/sketch_bench.rs` (`--release -- --ignored`).
  Causa principal: `diagnose` armaba jacobiano denso + SVD + espacio nulo de todo el sketch
  (texto sin restricciones: identidad 12000×12000). Ahora va por grupos de puntos unidos por
  restricciones, con el espacio nulo por bloques (`cad_solver::Nullspace`); los puntos sueltos
  no pasan por SVD. Además el arrastre rápido exigía `Converged` y un sketch sin definir da
  `UnderConstrained`: nunca era rápido. Regiones: muestreo con índice de puntos, nodos en
  rejilla, cruces con barrido por x y cajas de segmentos, gemelas por hash, poda con cola,
  contención ordenada por área. Ayudas: `Freedom` por bloques que se juntan al unir grupos,
  `gain` sin copiar bases (Gram-Schmidt sobre Bᵀ·fila), grados libres contados, relación que
  ya se cumple se suma sin resolver; iguales/paralelas/radios contra la primera de su grupo,
  vecinos por rejilla y cajas. Visor: líneas del sketch en lotes (`LineSegments2` con color
  por vértice, antes 2 objetos por entidad) y vista previa en `setSketchPreview` (su propio
  grupo y efecto: el mouse ya no rehace el sketch); preselección una vez por cuadro.

  | caso (release) | antes | después |
  |---|---|---|
  | placa 1125 ent.: resolver / arrastrar | 44 303 / 45 063 ms | 14 / 12 ms |
  | placa: regiones | 130 ms | 4 ms |
  | rejilla 60+60 (3481 regiones) | 254 ms | 19 ms |
  | texto 6000 ent.: resolver | 9 213 ms | 5 ms |
  | placa 80 ent.: sugerir / definir | 395 / 432 ms | 0,7 / 4,5 ms |
  | placa 320 ent.: sugerir / definir | > 60 s | 2 / 485 ms |

  Pruebas: `sketch_robust.rs` (arrastre rápido con algo libre), `sketch_solver2.rs` (definir
  con piezas sueltas casi iguales). Falta probarlo en la app (colores y grosores del visor).
  Queda: definir con relaciones en 1125 entidades iguales tarda 24 s (las iguales juntan todo
  en un bloque de ~2700 variables: costo cúbico; haría falta QR ralo); IPC del arrastre.

## U5 — Geometría y regiones correctas (medio)

- [x] Regiones de área ≈ 0 (círculo r 1e-9, elipse con semieje 0) entran a «All» y rompen la
  extrusión (`regions.rs:~600`, `eval.rs:4898`).
- [x] Puente entre contorno e isla: arista duplicada y la isla sale como región aparte (`regions.rs:~620`).
- [x] Curva que se cruza a sí misma (spline en 8) no se parte (`cut_crossings` solo i<j).
- [x] Cruces con elipse/spline quedan en la cuerda, no en la curva (`regions.rs:~230` `refine`);
  lo mismo en «Reparar» (`sketchEdit.ts:849`, punto movido a 0,12 % de R fuera del círculo).
- [x] Arco con inicio ≈ fin desaparece sin aviso (`regions.rs:575`).
- [x] Tolerancias absolutas: `regions.rs:537` usa la coordenada máxima de todos los puntos (una
  construcción lejos fusiona nodos chicos); `Parallel`/`Perpendicular` con residual ∝ L²;
  `sketchTrim.ts:73` `den < 1e-9` sin normalizar; `sketch3d.rs:~290` `nn < 1e-18`.
- **Hecho 2026-11-05**: un lazo con área ≤ tolerancia × perímetro no es región (ni suelto ni
  cara), así que «Todas» no los toma; la revisión lo avisa («no tiene tamaño», también al
  extruir sin regiones). Arista con sus dos medias en la misma cara = puente: se saca y se
  rearman las caras (el contorno queda con su agujero y la isla adentro). Auto-cruces de
  splines y B-splines; los tramos que vuelven a su nodo son lazos de una arista (también el
  arco de vuelta entera con inicio y fin distintos). Cruces llevados a la curva: forma cerrada
  entre rectas y círculos, Newton con la curva evaluada (`curve_eval`) si hay elipse o spline;
  en TS la revisión y «Reparar» usan el cruce exacto entre rectas, arcos y círculos. Escala de
  tolerancias solo con lo que forma regiones (Rust y `checkSketch`); paralela y perpendicular
  divididas por √(L1·L2) con su jacobiano; paralelos relativos a los largos en `sketchTrim` y
  `checkSketch`; `circumcenter` relativo a los lados. Pruebas: `regions_geometry.rs` (8, fallan
  sin el arreglo), `test_solver.rs` (jacobiano numérico, paralela en 1 µm), `sketch3d.rs`,
  `sketchCheck.test.mjs` y `sketchEdit.test.mjs`. Falta probarlo en la app. Queda: «Reparar»
  no parte cruces con elipses ni splines (como antes).

## U6 — Importar/exportar y fórmulas (medio)

- [x] DXF: tolerancia `size*1e-7` con coordenadas georreferenciadas fusiona/borra líneas
  (`sketchFiles.ts:55`); extrusión −Z ignorada (arcos espejados, `:181`); SPLINE cerrado sale
  abierto y extremos en los polos (`:214`); INSERT/bloques, TEXT y HATCH se ignoran en silencio
  (`:143`: mostrar `skipped` y expandir INSERT); unidades 3/7/… caen a 1 (`:133`); NaN y radio 0
  sin descartar (`:165`).
- [x] DXF exportado: arco de vuelta entera como `ARC 0..0` (`:329`); capas ocultas y nombres no ASCII en R12.
- [x] Fórmulas: `sqrt(2,5)` da `sqrt(2)` sin error (aridad sin validar, `sketchFormula.ts:114`);
  parámetros con acento o ñ no se entienden (`:41`, usar `\p{L}`); `1/0` aborta toda la curva
  (`:182`); `constructor` como variable (usar `Object.hasOwn`).
- [x] CSV/coordenadas: «1,5 2,5» se lee como (1, 5) (`sketchFiles.ts:21`); detectar separador.
- [x] `expr.rs:270`: `e`/`pi` pisan parámetros del usuario; sin tope de profundidad del parser.
- **Hecho 2026-11-05**: DXF leído con transformaciones: bloques (INSERT girado, escalado,
  anidado hasta 16 niveles y en filas/columnas; con escala despareja el círculo pasa a elipse
  con sus ejes principales), extrusión −Z en CIRCLE/ARC/polilíneas/INSERT (espejo en x) y en
  ELLIPSE (semieje menor y sentido), las 25 unidades de $INSUNITS, formas con NaN o sin tamaño
  saltadas y contadas. SPLINE sujeta a su dominio por inserción de nudos (`between` de
  `sketchTrim`): sin sujetar empieza sobre la curva; periódica con un período de nudos o con
  polos repetidos queda cerrada (o B-spline periódica si no cierra). Tolerancia de `addShapes`
  por el tamaño del dibujo (piso 1e-12 × lejanía). INSERT/TEXT/HATCH ya salían en el mensaje
  de salteadas; los ATTRIB del INSERT también. Exportar: vuelta entera como CIRCLE, capas en
  ASCII (sin acentos, mayúsculas, sin repetir) y las ocultas con color negativo (apagadas).
  Fórmulas TS: aridad validada, nombres con `\p{L}`, solo propiedades propias, lo que no da
  número corta la curva en tramos (una spline por tramo, con aviso). CSV: coma decimal con
  espacios. `expr.rs`: el parámetro manda sobre `e`/`pi` y anidamiento máximo 200. Pruebas:
  `sketchFiles.test.mjs` (+7), `sketchFormula.test.mjs` (+3), `expr.rs` (+2). Falta probarlo en
  la app con DXF reales (AutoCAD, LibreCAD, QCAD, Inkscape).

## U7 — Pulido de interfaz e i18n (bajo)

- [ ] Textos sin `t()`: `PointFields` «Inicio/Fin/Centro» (`DesignStep.tsx:9100-9119`),
  `Entidades (n)` (`:9038`), `Elegido (n)` (`:8267`), `Piezas (n)` (`:7309`), `Exportado (… KB)`
  (`:9249`), `Calco escalado ×` (`:9287`).
- [ ] Errores del backend sin `tBackend` (`cadUi.ts:528/1123/1136`, `CadView.tsx:870/1670/2654`).
- [ ] Error de un solve viejo se muestra tras deshacer (`cadUi.ts:528`, falta chequear `generation`).
- [ ] Fallos del sketch solo como mensaje efímero: estado «sin resolver» persistente en avisos.
- [ ] `CadViewer.dispose()`: el último lote `retired` y las texturas de calcos no se liberan; falta
  `forceContextLoss()`.
- [ ] Listeners de `window` del diálogo y del texto de cota sin `onCleanup` (`CadView.tsx:3684/3721`).

## U8 — Huecos de producto (después de U0–U4)

Imprescindible:
- [ ] **Reparar referencias rotas** (`SketchUse`): hoy solo avisa (`eval.rs:1922-1970`) y hay que
  romper el vínculo y volver a usar. Elegir la arista nueva en el visor (como «resolver arista colgante»).
- [ ] **MCP del sketch para editar**: hoy solo `sketch_create` y `sketch_add`; faltan borrar/mover
  entidades, recortar, usar arista/corte, spline, elipse, texto, offset, espejo, patrones.

Importante:
- [ ] Importar SVG a sketch (solo existe exportar).
- [ ] «Detectar relaciones» al importar DXF (coincidencias, horizontales, verticales) con
  `sketchSuggest`/`auto_define`.
- [ ] Tangencia y curvatura con la spline interpolada (`Geometry::Spline`): hoy solo B-spline
  (`sketch.rs:1552`, `:1604` → `WrongKind`).
- [ ] Offset de spline y elipse [S] (`sketchEdit.ts:416-520`).
- [ ] Cambiar una cota de radio a diámetro y al revés [S].

Deseable:
- [ ] Simetría respecto de un punto.
- [ ] Variante `Unknown` en serde para restricciones/geometrías de versiones nuevas.

## U9 — Pruebas que faltan

- Rust: entradas degeneradas (r 0, cotas ≤ 0/NaN, B-spline de 1 polo, pesos ≤ 0, restricción a
  entidad de otro tipo) sin que caiga el sketch; topología (puente, auto-cruce, arco start≈end);
  escala (1e-3 y 1e5 mm); banco de 1000+ entidades; `remove_entity` con texts/uses/patterns.
- Node: `dropDegenerate` con patrón de ≥3 copias, `createBlock` con origen, `explodeBlock`
  escalado, `editPattern` con centro borrado, recortar conservando `construction`/`layer`;
  `removeEntity`, `trimLine`, `trimCurve`, `extendArc`, `mirrorEntities`, `offsetEntities` (sin
  test directo); DXF (extrusión −1, SPLINE cerrado, coordenadas grandes, INSERT); fórmulas
  (aridad, Unicode).
- App: guardar con sketch abierto; pánico en un comando sin dejar la app inutilizable; MCP con
  entradas inválidas. Nada del sketch está probado en Tauri real (solo arnés HTTP).

## Orden sugerido de trabajo

1. U0 completo (un commit): es pérdida de datos.
2. U1: mutex + async del solve + `diagnose` fuera del arrastre + panics (un commit cada uno).
3. U2 y U3 juntos por archivo (`sketchEdit`/`sketchBlocks`/`sketchPatterns`; `CadView`).
4. U4 con banco antes y después.
5. U5–U7 por tandas; U8 según prioridad del usuario.
