# CAD paramétrico en Pinocchiors

Traer a Pinocchiors el CAD de `cad-blender` (addon de Blender) sin Blender: núcleo
B-Rep con OpenCASCADE desde el inicio (sin etapa intermedia de mallas) y el flujo
nuevo **escaneo → CAD** que conecta orizon3d con sólidos editables.

Decisiones (2026-10-05):

- **Motor**: OpenCASCADE (OCCT 7.9) desde el día uno. Redondeos, chaflanes, STEP real
  y caras exactas. Se descartó empezar con manifold3d y migrar: las referencias a caras
  cambian de motor a motor y el trabajo se haría dos veces. manifold puede volver
  más adelante solo para booleanas sobre mallas orgánicas (`print3d`), que OCCT hace mal.
- **El árbol guarda recetas, no resultados**: cada operación guarda parámetros y se
  recalcula. El sólido nunca se serializa (solo los STEP importados, como bytes).
- **Referencias por geometría, no por índice**: "esta cara" = punto + normal (+ tipo de
  superficie); "esta arista" = punto medio + dirección. Se resuelven buscando la cara/arista
  más parecida en el sólido recalculado. Evita el problema de nombres topológicos.
- **Unidades**: milímetros. **Ejes**: Z arriba dentro del CAD (como cualquier CAD y como
  la UI de la app); al pasar al visor se convierte a Y arriba (`converter_scene::z_up_to_y_up`).
- **Sin OCCT compila igual**: `cad-occt` detecta OpenCASCADE en `build.rs`; si no está,
  compila un stub (todo devuelve "OpenCASCADE no disponible") y `cad_occt::available()`
  lo dice. Así no hacen falta features en cadena. `CAD_REQUIRE_OCCT=1` lo vuelve error
  (releases), `CAD_OCCT_STUB=1` fuerza el stub (probar).

## Crates

| Crate | Ruta | Qué hace | OCCT |
|---|---|---|---|
| `cad-solver` | `libs/cad/solver` | Solver de restricciones 2D (Newton + LM, dispersas, diagnóstico), ajuste de primitivas (plano/esfera/cilindro, RANSAC), segmentación por normales, simplificación | no |
| `cad-occt` | `libs/cad/occt` | Puente C++ propio a OCCT: `Shape` con RAII, perfiles con líneas/arcos/círculos, operaciones, topología consultable, teselado con id de cara, STEP | sí |
| `cad-scan` | `libs/cad/scan` | Escaneo → CAD: elegir plano/cilindro con un clic, cortes, contornos a sketch, profundidad por rayos, detección automática | no |
| `cad-model` | `libs/cad/model` | Documento: sketches + árbol de operaciones, recálculo, referencias geométricas, serde | vía cad-occt |

## Fases

- [x] **F0 — Plan** (este archivo).
- [x] **F1 — `cad-solver`**: portar desde `cad-blender/crates/cadblender_solver` sin pyo3 ni
      marca de licencia. Tests del original pasando. `constraint3d` (código muerto allá) no se trae.
- [x] **F2 — `cad-occt`**: puente C++ nuevo (el de cad-blender solo hacía polilíneas y no
      exponía redondeo ni booleanas).
  - build.rs: OCCT del sistema (`OCCT_INCLUDE_DIR`/`OCCT_LIB_DIR`, rutas comunes, pkg-config),
    nombres de librerías de 7.9 (TKDESTEP) y anteriores.
  - Errores: cada llamada devuelve el mensaje de `Standard_Failure` (sin `nullptr` mudos).
  - Perfiles: segmentos, arcos por 3 puntos, círculos, B-splines; varios lazos (agujeros).
  - Primitivas; extruir (vector, simétrico), revolucionar, barrer, loft; booleanas;
    redondeo/chaflán de aristas elegidas; cáscara; transformar, espejar.
  - Topología: caras (tipo de superficie, normal, centro, área, eje/radio de cilindros) y aristas.
  - Teselado: vértices, normales, triángulos con id de cara, polilíneas de aristas (para dibujar
    y para elegir con el mouse).
  - Volumen/área/centro de masa/caja; validez (`BRepCheck`). STEP leer/escribir (bytes y archivo).
- [x] **F3 — `cad-model`**: documento con sketches en planos (XY/XZ/YZ, cara, plano libre),
      entidades + restricciones resueltas con `cad-solver`, detección de regiones cerradas,
      operaciones (sketch, extruir ciego/simétrico/pasante/hasta cara, revolucionar, redondeo,
      chaflán, cáscara, booleana, patrón lineal/circular, espejo, primitiva, STEP importado),
      recálculo con error por operación, retroceso (rollback), suprimir, serde.
- [x] **F4 — Escaneo → CAD** (`libs/cad/scan`, crate `cad-scan`): de una malla (`pinocchio_mesh::Mesh` o nube de orizon3d)
      segmentar + ajustar → primitivas detectadas (plano, cilindro, esfera) con su error;
      convertirlas en operaciones (plano de trabajo desde plano detectado, agujero/tetón desde
      cilindro, corte de la malla por plano → sketch). Banco con modelos de `~/Descargas`.
- [x] **F5 — App de escritorio** (`apps/desktop/src/cad.rs`): feature `cad` en `apps/desktop`, comandos Tauri (documento,
      agregar/editar/borrar operación, recalcular, teselado para el visor, exportar STEP/STL/3MF,
      convertir el sólido en el modelo de la app), guardado en `.pinocchio`.
- [x] **F6 — Espacio de trabajo "Diseñar"** en la web: árbol de operaciones, panel de parámetros,
      primitivas, sketch básico (línea, rectángulo, círculo, arco) sobre plano o cara, extruir,
      elegir caras/aristas en el visor, redondeo. Textos sin mencionar Blender.
- [~] **F7 — Distribución**: `scripts/build-occt.sh` + workflow manual `release-cad.yml` (no
      toca `release.yml`). Linux probado en local; Windows y macOS escritos pero sin correr.
- [ ] **F8 — Sketch completo**: portar herramientas de cad-blender. Hecho: polígono, ranura,
      cotas en el visor, enganche a puntos y curvas, horizontal/vertical automáticos, regiones
      elegidas con clic, puntos libres marcados, recortar (líneas, círculos y arcos), extender,
      equidistante, redondear esquinas, arcos tangentes encadenados, gestor de restricciones
      que resalta y elige lo que restringe.
- [ ] **F9 — Más adelante**: planos 2D (proyección + cotas), ensambles, chapa.

## Compilar

Arch: `pacman -S opencascade` (7.9.3). Otras: definir `OCCT_INCLUDE_DIR` y `OCCT_LIB_DIR`.

```
cargo test -p cad-solver
cargo test -p cad-occt
cargo test -p cad-model
```

## Bitácora

- **2026-10-05 F1** (c19087b): solver portado; 101 tests. Arreglado `solve_drag` cuando el
  destino choca con las restricciones (devolvía un compromiso que no cumplía ninguna).
- **2026-10-05 F2** (7047c85, ab9e38a): puente nuevo, 15 tests con volúmenes exactos.
  Hallazgos: (1) `ShapeFix_Face` no da vuelta agujeros mal orientados → se decide por área;
  (2) redondeos imposibles "funcionan" y dejan sólidos inválidos → redondeo, chaflán,
  cáscara, desmolde y booleanas validan con `BRepCheck_Analyzer`; (3) los traductores STEP
  imprimen estadísticas por stdout → se quitan los printers del messenger global.
- **2026-10-05 F3**: `cad-model` con 12 tests de flujo completo (placa paramétrica que cambia
  de ancho y su redondeo sigue la arista, bolsillo en cara pasante, tetón hasta cara,
  revolución con eje del sketch, patrón circular de agujeros, simetría, vaciado, corte,
  errores aislados por operación, retroceso, suprimir, serde, importar STEP).
  - Regiones: caras del grafo plano (media arista "anterior en orden antihorario"), lazos
    sueltos para círculos, contención por el lazo más chico; profundidad par = perfil.
  - Radio de círculos es dato en el solver (no variable): las cotas de radio se aplican
    antes de resolver. Tangencia línea-arco con extremo común = perpendicular al radio (exacta).
  - Referencias: cara = punto + normal (coseno ≥ 0,9), arista = punto + dirección; error si
    la mejor coincidencia está a más de media diagonal del cuerpo.
  - Límite conocido: dos aristas paralelas equidistantes del punto guardado (p. ej. tras
    duplicar una altura) pueden empatar. Arreglado el 2026-10-05 con orígenes de caras (abajo).
- **2026-10-05 F4**: `cad-scan`. Test de punta a punta: pieza CAD teselada con ruido ±0,02 →
  plano superior (z 20 ± 0,05, área ±1 %, 3 contornos), profundidad 20 ± 0,2 por rayos,
  agujero r 8 ± 0,05 (detectado como agujero), tetón r 10, corte a media altura → sketch
  con círculo reconocido y rectángulo enderezado → reconstrucción con volumen ±1 %.
  - Banco (`--example scan_report`): espéculo STL → esfera, cilindro r 8,999, planos, 99 %
    del área; cabeza de 126 k triángulos en 0,4 s.
  - Hallazgos: (1) los teselados de CAD traen triángulos largos y finos cuya normal el ruido
    vuelve cualquier cosa → cara de "normal poco confiable" si su altura < 2× tolerancia; se
    aceptan por banda de vértices y la segmentación las cruza con la normal de la última
    cara confiable. (2) La segmentación heredada compara contra la normal semilla y corta
    cilindros en sectores → segmentación suave cara-vecina, con la heredada solo para
    rescatar planos en zonas mixtas. (3) Los ajustes de cilindro heredados dependen del
    orden de los puntos y dieron ejes falsos → `fit_cylinder_normals` (autovector menor de
    Σ n·nᵀ + círculo de Kåsa en el corte), compite con RANSAC por menor error.
  - Pendiente: planos grandes con muchos triángulos finos se parten en 2–3 zonas en
    `detect_all` (no afecta `pick_plane`).
- **2026-10-05 F5**: comandos `cad_*` en la app. El frontend edita el documento entero y lo
  manda con `cad_set_document` (deshacer = copias del documento); el recálculo queda en
  caché por huella del JSON. `cad_mesh` devuelve binario (posiciones/normales Y arriba en
  unidades de la escena, índices, cara por triángulo, polilíneas de aristas) para que el
  sólido calce sobre el escaneo. `cad_scan_*` toma el modelo cargado (Z arriba, mm según
  `meters_per_unit`) con caché por huella de posiciones; el índice de triángulo del visor
  sirve de semilla. Exporta STEP exacto y STL/3MF/OBJ/PLY/GLB teselados; `cad_to_model`
  convierte el sólido en el modelo de la app. El diseño se guarda en `.pinocchio`
  (probado en MessagePack con enums etiquetados y bytes de STEP).
- **2026-10-05 F6**: espacio "Diseñar" (`CadView` + `CadViewer` + `DesignStep`, estado en
  `lib/cad.ts` y `lib/cadUi.ts`). Probado con el backend real sin Tauri (la sesión estaba
  bloqueada): `examples/cad_http.rs` sirve los comandos `cad_*` por HTTP y un HTML temporal
  reemplaza `__TAURI_INTERNALS__`; Chromium headless manejado por CDP desde Node 22
  (WebSocket nativo, sin puppeteer). Flujos verificados: caja + sketch con rectángulo
  dibujado + extrusión; triángulo con líneas cerrado con clics + arrastre de vértice +
  círculo como agujero; cota de largo editada (50 mm exactos); redondeo eligiendo aristas en
  el visor (volumen a 0,1 mm³ del teórico: el empalme del vértice común suma un poco) y cambio de radio; Ctrl+Z; sketch sobre una cara + agujero
  pasante; escaneo STL → cilindro elegido con un clic → cilindro CAD calzado sobre el escaneo.
  - Bugs encontrados en la prueba: polilínea cortada en cada clic (efecto dependía del
    objeto sesión entero), caída de la interfaz por carrera entre estado de herramienta y
    sketch, bolsillos sobre caras que apuntaban hacia afuera (ahora una extrusión que resta
    y no toca el sólido se da vuelta sola).
  - Pendiente de UI: cotas dibujadas en el visor (hoy se editan en el panel), encuadre del
    sketch al entrar, elegir regiones sueltas con clic, ejes de revolución con clic.
- **2026-10-05 F8 (parcial)**: polígono regular (círculo de construcción + lados iguales),
  ranura (arcos tangentes), cotas como etiquetas editables en el visor, puntos pegados a
  curvas, regiones sueltas elegidas con clic para extruir/revolucionar, zonas detectadas del
  escaneo agregables. Arreglado el diagnóstico de puntos libres (miraba solo las columnas
  del punto; ahora el espacio nulo del Jacobiano).
- **2026-10-05 F7**: OCCT 7.9.3 estático y mínimo (`scripts/build-occt.sh`, ~25 min con 16
  núcleos): modelado + TKDESTEP; CMake arrastra XCAF/V3d/LCAF como dependencias de TKDESTEP
  pero sin freetype/OpenGL/X11 compilan igual, y al enlazar estático no entra ningún objeto de
  ellos (0 símbolos XCAFDoc/V3d/Graphic3d en el binario). Tests de cad-occt y cad-model pasan
  enlazados estáticos; el binario solo depende de libstdc++/libc.
  - Tamaño de la app release (Linux): 62,7 MB con CAD contra 23,6 MB sin CAD; sin símbolos
    51,8 contra 19,5 MB (+32 MB); comprimido con xz 14,8 contra 5,4 MB (+9,4 MB). Lo grueso es
    el esquema STEP AP214 y los algoritmos booleanos.
  - Pendiente: correr el workflow (para `workflow_dispatch` el archivo tiene que estar en la
    rama por defecto); en Windows se define `OCCT_STATIC_BUILD` y se enlazan user32/advapi32/
    ws2_32/gdi32/shell32, en Linux dl/pthread; macOS sin extras. Podrían faltar libs del
    sistema que solo se ven al enlazar en esas plataformas.
- **Pruebas de punta a punta**: `apps/web/e2e/cad.mjs` (9 escenarios con volúmenes contra el
  teórico) con `examples/cad_http.rs` + vite + Chromium headless. Workspace completo: 775
  tests pasan.
- **2026-10-05 Referencias por origen**: cada cara del sólido lleva etiquetas de origen
  (`FaceTag { feature, name }`): extrusión "inicio"/"fin"/"lado:<entidad>", revolución
  "lado:<entidad>"/"inicio"/"fin", primitivas por eje ("+z", "lado", "arriba"), importado
  "cara:<i>", redondeo/chaflán "redondeo:<k>", corte "corte", copias de patrón "#k" y simetría
  "#espejo". Se calculan en cada recálculo y viajan con la historia de OCCT
  (`Modified/Generated/IsDeleted`, registrada en el puente por operación y leída con
  `occt::with_history`). `FaceRef.tags` y `EdgeRef.sides` (orígenes de las dos caras de la
  arista) se resuelven primero por origen y desempatan por distancia exacta al punto guardado;
  sin origen, la resolución geométrica de antes. Tests en `model/tests/naming.rs`: el redondeo
  de la arista superior sigue arriba al duplicar la altura (sin orígenes elegía la de abajo),
  caras partidas por booleanas conservan el origen, sketch sobre la tapa sigue a la tapa.
  - Sin origen todavía: caras nuevas de vaciado y desmolde (usan la geometría).
- **2026-10-05 Parámetros y fórmulas**: `Document.parameters` (nombre = expresión) y
  `Document.bindings` (ruta del campo → expresión, p. ej. `3.kind.extent.distance` o
  `0.kind.sketch.constraints.4.value`). `Document::resolve` evalúa los parámetros en orden de
  dependencia (ciclos, nombres repetidos o desconocidos dan error sin frenar el resto), aplica
  las fórmulas sobre el JSON del documento (enteros redondeados) y el recálculo usa ese
  documento; el guardado conserva números y fórmulas. Expresiones (`expr.rs`): + − × ÷ ^,
  paréntesis, coma o punto decimal, `pi`, trigonometría en grados, `sqrt abs min max round
  floor ceil`. En la interfaz cualquier campo numérico acepta fórmula (marca "fx"), las cotas
  del sketch también (en el panel y en la etiqueta del visor), sección Parámetros con renombre
  que actualiza las fórmulas que lo usan.
- **2026-10-05 F8 (resto)**: arco tangente (G) desde el extremo de una línea o arco, encadenable
  (tangencia arco-arco con extremo común = centros alineados con el contacto, nuevo en el
  solver); extender (E) hasta el primer cruce; recortar círculos (pasan a arco) y arcos (se
  acortan o se parten); gestor de restricciones: pasar el mouse resalta en el visor lo que
  nombra, clic lo elige. Bug encontrado: con dos cruces sobre el mismo lado, el segundo punto
  partía la línea original ya cortada (rompía las regiones); ahora se parte el tramo que lo
  contiene.
- **2026-10-05 Flujo tipo Onshape** (pedido del usuario): "Sketch" se pone en el plano o la
  cara elegida, o pide elegir uno (planos base planta/frente/lateral visibles en el visor, a
  escala del modelo). Al terminar cada forma se piden sus cotas en el visor (rectángulo:
  ancho y alto; círculo: diámetro; línea: largo, salvo el tramo que cierra; arco y arco
  tangente: radio; ranura: largo y radio; polígono: radio): quedan con lo dibujado
  (redondeado según el zoom), Enter/Tab aplica y pasa a la siguiente, Esc corta. Fuera del
  sketch se eligen caras, aristas, regiones de sketches visibles y planos (Mayús/Ctrl suma);
  Extrusión/Revolución usan las regiones elegidas, Redondeo/Chaflán las aristas,
  Vaciado/Desmolde las caras. Los planos solo se eligen si no hay caras ni regiones bajo el
  puntero. Bugs encontrados por las pruebas: el campo de cota se recreaba en cada cuadro y
  perdía el foco (For → Index), Esc en la cota no cortaba la polilínea, las etiquetas tapaban
  clics de dibujo (ahora sin clic mientras se dibuja y corridas fuera de la figura).
- **2026-10-05 Anclajes, fase 1** ([PLAN_ANCLAJES.md](PLAN_ANCLAJES.md)): `Sketch.origin`, punto
  fijo en (0, 0) con ecuación implícita en el solver, que no se borra al quitar entidades (los
  sketches viejos lo reciben al editarse). Restricciones nuevas `horizontal_points` /
  `vertical_points` (también en el panel con dos puntos elegidos). `lib/sketchSnap.ts` calcula
  el anclaje bajo el cursor con prioridad punto/centro > origen > medio > cuadrante > sobre la
  curva (pruebas en node: `e2e/sketchSnap.test.mjs`); `placeSnap` deja la restricción: medio =
  parte la línea con mitades iguales, cuadrante = punto en círculo alineado con el centro.
  Todas las herramientas lo usan (rectángulo, círculo, arco, polígono, ranura, arco tangente);
  entre dos puntos que ya estaban no se piden cotas (quedan definidas). Mayús dibuja libre;
  glifo junto al cursor. Bug encontrado: partir una línea inclinada dejaba las dos mitades
  sin alinear (ahora quedan paralelas) y borraba la cota de largo (ahora pasa a distancia
  entre los extremos, con su fórmula).
- **2026-10-05 Anclajes, fase 2**: intersecciones línea-línea, línea-círculo y círculo-círculo
  (los arcos recortados a su barrido), con prioridad entre el origen y el punto medio; solo se
  cruzan las curvas que pasan cerca del cursor. El punto queda sobre las dos (las líneas se
  parten ahí). E2E: centro de un círculo en el cruce de dos líneas → cuatro mitades.
- **2026-10-05 Anclajes, fase 3**: sin un punto cerca, al dibujar una línea desde un punto se
  ofrece salir paralela o perpendicular a otra línea, o tangente al arco del que sale (no las
  horizontales/verticales: esas las pone la restricción de eje); si no, el punto se alinea en
  horizontal y/o vertical con los puntos cercanos (nunca con el punto de salida ni con los de
  la forma en curso). Líneas guía punteadas y la entidad de referencia resaltada. Las pruebas
  e2e con clics en coordenadas fijas ahora tienen que quedar lejos (> 8 px) de alineaciones
  sin querer.
- **2026-10-05 Radio variable** ([PLAN_SKETCH.md](PLAN_SKETCH.md), fase 2): el radio de los
  círculos deja de ser dato. Cada círculo suma al solver un punto oculto en su borde (misma
  altura que el centro); radio, diámetro, igual, punto en círculo y tangencias se escriben
  sobre esa distancia, y después de resolver el radio vuelve a `Geometry::Circle`. Un círculo
  sin cota tiene su radio libre (un grado más, como Onshape). Solver: `TangentLineCircleVar`
  y `TangentCircles` (por fuera o por dentro según cómo estén al resolver). Modelo:
  `Concentric`, tangencia entre dos círculos/arcos sin extremo común. Panel: "Tangentes" y
  "Concéntricos" con dos curvas elegidas. Pruebas: círculo por tres puntos, tangente a una
  esquina en L, iguales y tangentes, concéntrico y tangencia interna, documento viejo con
  radio sin cota.
- **2026-10-05 Colores por entidad y cotas de referencia** ([PLAN_SKETCH.md](PLAN_SKETCH.md),
  fase 3): `SolveReport.free_entities` (alguno de sus puntos o su radio libre, según el espacio
  nulo) → azul en el visor (`--color-sketch-free`), en conflicto → rojo, definida → color del
  texto. Las cotas llevan `reference` (no entran al solver; al resolver se escribe lo que miden)
  y se muestran entre paréntesis; botón "Ref" en el panel. Una cota que repite lo que ya está
  definido también sobre-define (antes pasaba sin aviso porque no deja residuo): se marca la
  más nueva de las redundantes y el visor ofrece "Dejarla de referencia" o "Quitarla".
- **2026-10-05 Herramientas simples** ([PLAN_SKETCH.md](PLAN_SKETCH.md), fase 4): rectángulo
  por el centro (diagonal de construcción partida en el centro: mitades paralelas e iguales,
  así el centro puede ser el origen), arco por 3 puntos (inicio, fin y uno por donde pasa;
  se guarda antihorario, al revés si pasa por el otro lado; tecla 3), punto suelto
  (`Geometry::Point`, no forma regiones; tecla O) y Q para construcción sí/no en lo elegido.
  `geometryPoints` reemplaza las cadenas de ternarios que daban por hecho "si no, es arco".
- **2026-10-05 Simetría y patrones en el sketch** ([PLAN_SKETCH.md](PLAN_SKETCH.md), fase 5):
  simetría respecto de la primera línea elegida (cada punto copiado `symmetric` con su
  original, los que están sobre el eje se comparten, los arcos se invierten). Patrón lineal y
  circular con dos restricciones nuevas del solver, `EqualVector` (mismo desplazamiento) y
  `EqualRotation` (mismo giro alrededor de un centro): solo el primer par lleva cotas (distancia
  horizontal y vertical, o radios iguales y ángulo entre dos líneas de construcción) y cambiar
  esa cota mueve todas las copias. Con 2 en total el centro es punto medio (180° es inestable
  con `atan2`). Los círculos copiados llevan `equal` (el radio es incógnita).
- **2026-10-05 Elipse** ([PLAN_SKETCH.md](PLAN_SKETCH.md), fase 6, primera parte):
  `Geometry::Ellipse { center, major, minor }` con los extremos de los semiejes como puntos
  (perpendicular implícita en el solver), así los radios se acotan con distancias y se
  arrastran. En OCCT es exacta: `Curve::Ellipse` (tipo 4 del puente, `gp_Elips`; si el semieje
  b es el mayor se gira el eje por dentro). Herramienta Elipse (I): centro, extremo del eje
  mayor y ancho. Al dibujar, ahora solo los rectángulos evitan alinearse con su primer punto
  (la elipse, el arco, la ranura y el polígono sí se alinean con su centro). La barra del
  sketch pasa a dos filas si no entra.
- **2026-10-05 Spline con manijas** ([PLAN_SKETCH.md](PLAN_SKETCH.md), fase 6, segunda parte):
  herramienta Spline (N): clics por donde pasa, clic en el primero la cierra, Esc la termina
  abierta. `Geometry::Spline` suma `start_handle`/`end_handle` opcionales (puntos: la tangente de
  salida y la de llegada, en el sentido de avance); en OCCT, `Curve::SplineEnds` (tipo 5 del
  puente, `GeomAPI_Interpolate::Load` con escala: cuenta la dirección, no el largo). En el
  visor la spline se ve suave (Catmull-Rom con esas tangentes; antes era una polilínea) y las
  manijas con línea punteada; botón "Manijas en los extremos sí/no".
- **2026-10-05 Texto** ([PLAN_SKETCH.md](PLAN_SKETCH.md), fase 7; el usuario eligió opentype.js
  en vez de OCCT + freetype): herramienta Texto (X) con el texto, el tamaño (em, en mm) y la
  fuente (incluida Liberation Sans, SIL OFL, en `apps/web/public/fonts` con su licencia; o un
  .ttf/.otf elegido). `lib/sketchText.ts` pasa los contornos a líneas y splines: las Bézier
  seguidas que empalman suave (< 15°) van en una sola spline por puntos muestreados, cortando en
  las esquinas; un contorno todo curvo es una spline cerrada (la "o"). Pruebas en node con la
  fuente real (`e2e/sketchText.test.mjs`) y e2e "Hola" extruido. Pendiente: texto rígido y
  editable (hoy queda como curvas libres).
- **2026-10-05 Horizontal/vertical al dibujar e íconos** (pedido del usuario): al dibujar una
  línea desde un punto, a ≤ 8 px de la horizontal o la vertical se pega a ella (glifo y guía
  punteada), combinado con alinearse en el otro eje con otro punto; la restricción es la que se
  vio. Reemplaza a la regla vieja que agregaba horizontal/vertical después, si la línea quedaba
  a menos de 3° (sin aviso). La barra del sketch pasa a íconos propios (`icons/sketch.tsx`,
  trazo fino, puntos de clic rellenos, auxiliar punteado) en tres grupos (elegir · dibujar ·
  modificar); el nombre va en el tooltip y en un texto solo para lectores de pantalla.
- **2026-10-05 Diálogo de operación con vista previa** ([PLAN_EDICION.md](PLAN_EDICION.md),
  fase 1): crear o elegir una operación abre su diálogo (✓ / ✗, Enter / Esc). Mientras está
  abierto, `commit` cambia un borrador (`Draft` en `cad.ts`, `store.doc()` lo devuelve) y se
  manda con `cad_preview`, que el backend guarda aparte (`AppState::cad_preview`) y evalúa
  hasta la operación en edición, como Onshape; lo de abajo se ve atenuado. Aceptar deja un solo
  paso de deshacer; cancelar vuelve atrás y una operación nueva cancelada desaparece. Exportar y
  pasar a modelo usan el documento guardado (`evaluate_committed`). La caché guarda el recálculo
  anterior: cancelar o aceptar sin cambios no recalcula. Salir de Diseñar acepta el diálogo.
- **2026-10-05 Caja centrada y exportar desde el encabezado** (pedido del usuario): las cajas
  nuevas llevan `centered` (centradas en X e Y, base en Z = 0; las de documentos viejos siguen
  desde la esquina). El botón Exportar del encabezado se habilita con un diseño y, abierto desde
  Diseñar o sin modelo, exporta el sólido (STEP, STL, 3MF, OBJ, PLY, GLB) con el visor del
  diseño a la vista; `cad_export` también en el puente HTTP para el e2e.
- **2026-10-06 Cajas de selección** ([PLAN_EDICION.md](PLAN_EDICION.md), fase 2): los campos
  de referencia del diálogo (aristas del redondeo y el chaflán, caras del vaciado y el
  desmolde, cara de "hasta una cara", regiones) son cajas como en Onshape: con clic se activan
  (borde de acento) y lo que se elige en el visor entra o sale; la lista resalta cada ítem al
  pasar el mouse y lo quita con ✗. Activa, la vista previa se evalúa sin la operación
  (`Draft.selecting`), así se eligen las aristas originales y no las ya redondeadas. Para
  resaltar y alternar, `cad_resolve_refs` resuelve las referencias contra el sólido mostrado
  (`Evaluation::resolve_face`/`resolve_edge`, la misma resolución del recálculo); las que no
  aparecen salen en rojo. Una caja vacía se activa al abrir el diálogo, y los paneles sueltos
  de "Aplicar" se fueron. Redondeo, chaflán y desmolde sin nada elegido dicen qué falta. El
  editor del panel se monta por id y aceptar o cancelar cambian todo en un `batch` (antes,
  cerrar el diálogo podía leer una operación ya desmontada y cortar el guardado).
- **2026-10-06 Herramienta translúcida** ([PLAN_EDICION.md](PLAN_EDICION.md), fase 3): con el
  diálogo de una extrusión, revolución, primitiva o importación abierto, su herramienta se
  dibuja translúcida encima del resultado, como en Onshape: verde si suma, roja si resta, ámbar
  si interseca, con el contorno visible a través del sólido. `Evaluation::tool` da la forma y
  `cad_tool_mesh` la tesela en el formato de `cad_mesh` (vacía si la operación no tiene); el
  store la pide después de cada vista previa (`store.tool()`). No se dibuja mientras se elige
  en una caja (el sólido es el de antes) ni después de aceptar o cancelar.
- **2026-10-07 Referencias perdidas** ([PLAN_EDICION.md](PLAN_EDICION.md), fase 4): el
  recálculo anota qué referencia no encontró (`MissingRef { field, index }`, con el nombre del
  campo de la operación: `edges`, `faces`, `regions`, `plane`, `neutral`, `axis`, `extent`). En
  listas (aristas, caras, regiones) la operación sigue con las que están y queda en el estado
  nuevo `FeatureState::Warning` ("faltan 1 de 2 aristas"); si no queda ninguna, o la referencia
  es única (plano de un sketch, eje, "hasta una cara"), falla con `Error { missing }`. El árbol
  muestra la advertencia en ámbar y el error en rojo; en el diálogo las perdidas salen "no
  encontrada" aunque la caja no esté activa, la caja se activa sola al editar y lo que se elige
  ocupa el lugar de la primera perdida. Perfil y regiones se resuelven una sola vez
  (`selected_regions` ya no se repite en `profile_faces`).
- **2026-10-07 Árbol de operaciones** ([PLAN_EDICION.md](PLAN_EDICION.md), fase 5): barra de
  retroceso siempre visible y arrastrable entre filas (al soltarla al final se calcula todo);
  filas arrastrables para reordenar (`store.moveFeatureTo(id, hueco)`; subir y bajar usan lo
  mismo); clic derecho sin arrastrar sobre una cara abre "Editar «operación»" con las
  operaciones que la originaron según sus `FaceTag` y el sketch de extrusiones y revoluciones.
  El arrastre usa eventos de puntero (no HTML5 drag & drop, que WebKitGTK con Tauri maneja
  mal) y un umbral de 4 px para no confundirlo con el clic que abre el diálogo.
- **2026-10-07 Caja con el centro en el origen** (pedido del usuario: "centro con centro"): las
  cajas nuevas quedan centradas también en Z (`PrimitiveShape::Box { centered_z }`, campo nuevo
  con `serde(default)`: las viejas siguen con la base en el origen o en una esquina). El diálogo
  tiene "Origen en: el centro / el centro de la base / una esquina". El e2e se ajustó (la tapa
  de la caja de 20 queda en z = 10) y `sketchOn` pregunta al visor qué hay bajo el punto antes
  de hacer clic en la planta, porque ahora la caja la atraviesa.
- **2026-10-07 Medir** ([PLAN_INSPECCION.md](PLAN_INSPECCION.md), fase 1): con una o dos cosas
  elegidas en el sólido (caras, aristas y ahora vértices) aparece abajo a la derecha del visor
  el panel de medidas, como en Onshape: de una, tipo, área, largo, radio y diámetro o
  coordenadas; de dos, distancia mínima (dibujada punteada entre los puntos más cercanos),
  ΔX/ΔY/ΔZ, distancia entre centros y ángulo. El mensaje suelto de "Cara plana · área" se
  reemplazó por el panel. `cad_measure` en Tauri y en el puente HTTP.
- **2026-10-07 Masa y material** ([PLAN_INSPECCION.md](PLAN_INSPECCION.md), fase 2): material
  del documento (PLA, PETG, ABS, nailon, resina, aluminio, acero, inoxidable, latón, pino o
  densidad propia); la sección Sólido muestra masa, centro de masa e inercia principal, y "Ver
  el centro de masa" lo marca en el visor. Cambiar el material es un paso de deshacer y no
  recalcula el árbol (el hash del documento lo ignora). El puente devuelve los momentos
  principales con sus ejes.
- **2026-10-07 Vistas** ([PLAN_INSPECCION.md](PLAN_INSPECCION.md), fase 3): cubo de vistas en
  el visor del diseño, vistas estándar con Mayús+1…7 (Onshape) y 1/3/7 con o sin Ctrl (como el
  visor de mallas), F o «.» para acercar a lo elegido, y en el menú de la cara "Mirar de frente"
  y "Acercar a la cara". Los giros duran 0,3 s con frenado.
- **2026-10-07 Menú del clic derecho** ([PLAN_INSPECCION.md](PLAN_INSPECCION.md), fase 4): el
  menú cambia según lo que hay bajo el puntero (cara, arista o vacío) y crea operaciones con
  eso: sketch en la cara, vaciar, desmoldar, redondear, chaflán; más editar lo que la originó y
  las vistas. Mayús+S abre un sketch con lo elegido e Inicio encuadra todo.
- **2026-10-07 Filtros y caja de selección** ([PLAN_INSPECCION.md](PLAN_INSPECCION.md), fase 5):
  filtro de qué se elige en la barra del visor y selección por caja al arrastrar en vacío
  (ventana hacia la derecha, cruce hacia la izquierda, solo lo visible, Mayús suma).
- **2026-10-07 Vista de corte** ([PLAN_INSPECCION.md](PLAN_INSPECCION.md), fase 6): corte solo de
  vista por los planos base, con posición regulable, invertir y la sección rellena (tapa con
  stencil); lo cortado no se elige. Con esto el plan 3 queda hecho salvo el plano de corte por
  una cara y su manipulador.
- **2026-10-07 Carpetas** ([PLAN_EDICION.md](PLAN_EDICION.md), fase 6, con lo que el plan 2
  queda completo): tramos de operaciones agrupados, plegables y con nombre; solo presentación
  (no recalculan). Mayús+clic elige un tramo en el árbol.
- **2026-10-07 Recálculo incremental** ([PLAN_RECALCULO.md](PLAN_RECALCULO.md), fases 1 y 2):
  caché por operación con huellas encadenadas; cambiar algo al final de 30 operaciones pasa de
  506 ms a 10 ms. El árbol muestra el tiempo de las operaciones lentas. El driver del e2e ahora
  mata el grupo de procesos de Chromium y borra su perfil al cerrar (los `chrome-*` habían
  llenado `/tmp`).
- **2026-10-07 Cola de envíos y teselado medido** ([PLAN_RECALCULO.md](PLAN_RECALCULO.md),
  fases 3 y 5): escribir varios valores seguidos calcula solo el último pendiente; el banco
  ahora mide el teselado (40–55 ms, 4 ms de extracción).
- **2026-10-07 Varias piezas** ([PLAN_PIEZAS.md](PLAN_PIEZAS.md), fases 1 y 2): un diseño puede
  tener varias piezas ("Nueva pieza" en el selector de operación; unir algo que no toca nada
  también la crea). Cada operación actúa sobre las piezas que corresponden; la sección
  "Piezas" da color, nombre, visibilidad y exportación por pieza.
- **2026-10-07 Booleanas entre piezas** ([PLAN_PIEZAS.md](PLAN_PIEZAS.md), fases 3 y 4): "Con las
  piezas" en unir/restar/intersecar, operación Booleana (unir, restar conservando o no,
  intersecar), Separar sólidos sueltos en piezas y Borrar pieza. Las dependencias por pieza
  impiden reordenar una booleana antes de lo que creó sus piezas.
- **2026-10-07 Material por pieza** ([PLAN_PIEZAS.md](PLAN_PIEZAS.md), fase 5): cada pieza puede
  tener su material; la masa total y el centro de masa salen de las piezas.
- **2026-10-07 STEP con piezas** ([PLAN_PIEZAS.md](PLAN_PIEZAS.md), fase 6, con lo que el plan 5
  queda completo): el STEP lleva cada pieza con su nombre y su color; cuesta +6,7 MB en el
  binario estático (XCAF).
- **2026-10-07 Geometría de referencia** ([PLAN_CONSTRUCCION.md](PLAN_CONSTRUCCION.md), fase 1):
  planos, ejes y puntos de referencia como operaciones del historial, visibles y elegibles en
  el visor, usables como plano de sketch, eje de revolución o de patrón y plano de simetría.
- **2026-10-07 Extrusión completa** ([PLAN_CONSTRUCCION.md](PLAN_CONSTRUCCION.md), fase 2): dos
  direcciones, hasta la siguiente cara, desmolde de las paredes y extrusión delgada.
- **2026-10-07 Barrido y transición** ([PLAN_CONSTRUCCION.md](PLAN_CONSTRUCCION.md), fase 3):
  operaciones con diálogo; el barrido usa esquinas a inglete y el camino se encadena solo.
- **2026-10-07 Agujero** ([PLAN_CONSTRUCCION.md](PLAN_CONSTRUCCION.md), fase 4): simple, con
  caja o avellanado, pasante o ciego con punta, en los puntos de un sketch; tamaños métricos
  M2–M12 para pasar o roscar.
- **2026-10-07 Hélice y engrosar** ([PLAN_CONSTRUCCION.md](PLAN_CONSTRUCCION.md), fase 5 sin el
  nervio): hélice de referencia que sirve de camino de barrido (resortes) y engrosar caras.
- **2026-10-07 Mover cara, escala y patrón en curva** ([PLAN_CONSTRUCCION.md](PLAN_CONSTRUCCION.md),
  fase 6): se arregló también la transformación no uniforme del puente (perdía la escala).
- **2026-10-07 Plano 2D** ([PLAN_PLANOS_2D.md](PLAN_PLANOS_2D.md), fase 1 y parte de la 2):
  frente, planta, lateral e isométrica con líneas ocultas exactas, primer o tercer diedro,
  escala normalizada automática, cajetín y exportación a SVG.
- **2026-10-07 Plano a DXF** ([PLAN_PLANOS_2D.md](PLAN_PLANOS_2D.md), fase 5): la hoja o una
  vista sola a 1:1 para corte láser o CNC.
- **2026-10-07 Cotas generales en el plano** ([PLAN_PLANOS_2D.md](PLAN_PLANOS_2D.md), parte de la
  fase 3): ancho y alto de cada vista con flechas y texto, también en el DXF.
- **2026-10-07 Corte A-A en el plano** ([PLAN_PLANOS_2D.md](PLAN_PLANOS_2D.md), fase 4): el
  frente en corte por el plano medio, rayado, con la línea de corte en la planta.
- **2026-10-07 Plano a PDF** ([PLAN_PLANOS_2D.md](PLAN_PLANOS_2D.md), fase 2): con `svg2pdf` en
  Rust, a tamaño real de hoja; el SVG pasó a ser XML válido.
- **2026-10-07 Desviación escaneo ↔ diseño** ([PLAN_ESCANEO.md](PLAN_ESCANEO.md), fase 1): el
  escaneo se pinta según cuánto se aparta del sólido, con resumen (media, P95, máximo, % dentro
  de tolerancia).
- **2026-10-07 Contornos del escaneo con relaciones** ([PLAN_ESCANEO.md](PLAN_ESCANEO.md), fase 3):
  los sketches de corte llegan con paralelas y perpendiculares inferidas y esquinas
  redondeables a una grilla.
- **2026-10-07 Ensamble** ([PLAN_ENSAMBLES.md](PLAN_ENSAMBLES.md), fases 1, 2 y parte de la 4):
  instancias de las piezas, relaciones fija, bisagra, deslizante, cilíndrica y plana con un
  solver 3D, grados libres y revisión de choques.
- **2026-10-07 Lista de materiales** ([PLAN_ENSAMBLES.md](PLAN_ENSAMBLES.md), fase 4): piezas,
  cantidades, material y masa del ensamble, exportable a CSV.
- **2026-10-07 Cotas a mano en el plano** ([PLAN_PLANOS_2D.md](PLAN_PLANOS_2D.md), fase 3): largo,
  diámetro y distancia entre paralelas eligiendo líneas de la hoja; siguen al modelo.
- **Hecho (2026-10-07):** las piezas del diseño son objetos del Outliner y pasan solas a
  Preparar, Rig y Fabricar; lo hecho sobre su malla se rehace si cambia el diseño. Ver
  `PLAN_OBJETOS.md`. El Outliner va en tres grupos: Planos, Operaciones y Objetos.
- **Rediseño de la interfaz de Diseñar (2026-10-07, hecho 2026-10-08).** Decisiones del usuario: árbol de
  operaciones en el Outliner (no columna aparte) y botones con ícono y texto.
  - Hecho: fase 1, barra de herramientas arriba del visor (`components/design/DesignToolbar.tsx`,
    íconos en `icons/design.tsx`; principales a la vista, el resto en menús Sólidos / Modificar /
    Patrones / Piezas / Referencias; Diseño|Ensamble, Plano 2D, deshacer) y diálogo de la operación
    flotante y arrastrable en `CadView` (`FeatureEditor` exportado de DesignStep). Las acciones
    salieron a `lib/designActions.ts` (`createDesignActions`, una vez en App). Fase 2, el árbol de
    operaciones (`FeatureTree`, exportado) vive en el grupo Operaciones del Outliner (`SceneNode.content`).
  - Fase 3: el panel en pestañas Diseño / Pieza / Inspección / Desde el escaneo (secciones
    `design*` en `lib/pipeline.ts`, `DesignStep` recibe `section`). `ContextPanel` pasa la sección
    como función: con el valor suelto el `<Match>` la leía una vez y la pestaña cambiaba el
    título pero no el contenido. Suite e2e completa 54/54 (2026-10-08); el arnés abre solo los
    menús de la barra y las pestañas (`clickText`, `clickContains`, `tab()`).
  - Fase 4 (2026-10-08): los atajos del momento van en la barra de estado de la app, con las
    teclas resaltadas como en Blender (`lib/designHints.ts`: sin nada, eligiendo, con diálogo,
    con algo elegido, en el sketch); los mensajes de la app se ven 6 s a la derecha. El aviso
    "elegir …" bajó a la pila de mensajes del visor (arriba tapaba la segunda fila de la barra).
    Ya no aparece "Importa un modelo para comenzar" en Diseñar.
  - Pedidos del usuario (2026-10-08): exportar el diseño solo desde el área Exportar (salieron
    los botones STEP/STL/3MF de las pestañas Pieza e Inspección; ahí se elige "Qué exportar":
    todo el diseño o una pieza), y la selección por caja arrastrando desde cualquier lado, también
    sobre el sólido (un clic sin arrastrar sigue eligiendo uno).
  - Arreglado también: el ojo de Grid del Outliner apaga la grilla del visor de Diseñar.
- **2026-10-08 Línea desde el centro, nervio y reemplazar cara** ([PLAN_CONSTRUCCION.md](PLAN_CONSTRUCCION.md),
  fases 5 y 6 completas): la línea del sketch crece igual a los dos lados del primer clic (punto
  medio); el nervio lleva las líneas de un sketch hasta el sólido con espesor centrado; reemplazar
  cara lleva caras planas hasta un plano o una cara (con dos o más caras elegidas, la última es el
  destino). Pruebas en `model/tests/modify.rs` y e2e.
- **2026-10-08 Exportar el ensamble** ([PLAN_ENSAMBLES.md](PLAN_ENSAMBLES.md)): en Exportar, "Qué
  exportar" ofrece el ensamble cuando tiene instancias; STEP con cada instancia en su lugar, su
  nombre y el color de su pieza, y en malla (3MF, STL, OBJ, PLY, GLB) un objeto por instancia
  (`cad_export` con `assembly`).
- **2026-10-08 Texto del sketch como bloque editable** ([PLAN_SKETCH.md](PLAN_SKETCH.md), fase 7):
  se mueve entero con su ancla y se puede cambiar lo que dice y el tamaño.
