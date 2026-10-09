# Sketch: igualar la lista completa de un CAD

Plan del sketch, rearmado el 2026-10-09 para cubrir **toda** la lista de funciones de sketch
de un CAD de referencia (Onshape, SolidWorks, Fusion) que entregó el usuario. Reemplaza a los
planes anteriores (sketch completo y anclajes). Las fases 1–4, 6–10, 12, 14, 16, 17 y 20 están hechas (la 5 pasó a
ser la 8). Lo hecho está también en la bitácora de [ROADMAP.md](ROADMAP.md). Al final, la
**cobertura**: cada ítem de la lista con su estado o la fase que lo trae.

Tamaños: S = un día, M = unos días, L = una semana o más. «Backend» = toca el modelo o el solver
en Rust (recompilar el puente para el e2e: 8–16 min); «C++» = además el puente de OpenCASCADE.

## Fases hechas

1. **Deshacer y rehacer dentro del sketch.** *Hecha el 2026-10-09: pilas en `cadUi` (`undoSketch`/`redoSketch`,
   `sketchHistory`, `sketchRestored`), Ctrl+Z/Ctrl+Mayús+Z enrutados desde `App.tsx` mientras hay
   sketch abierto, botones en la barra del sketch. Ojo con Solid: `on(() => señal().campo)` corre
   con cada cambio de la señal aunque el campo no cambie; por eso «repuesto» es una señal aparte
   (si no, cada clic cortaba la línea encadenada).*
2. **Transformar**: mover, copiar, rotar, escalar, dividir; copiar y pegar. *Hecha el 2026-10-09:
   `lib/sketchTransform.ts` (puro, `e2e/sketchTransform.test.mjs`). Herramientas Partir (D),
   Mover (V), Copiar (K), Girar (H) y Escalar (Y) con clics y vista previa; bloque «Transformar
   lo elegido» en el panel con números (girar y escalar desde el centro de lo elegido);
   Ctrl+C / Ctrl+V con portapapeles que sobrevive al sketch. Decisión al transformar: los puntos
   compartidos con lo no elegido se mueven igual (lo de al lado se estira), los fijos pasan a su
   lugar nuevo, las cotas toman lo que miden ahora (las que tienen fórmula no se tocan) y las
   restricciones que dejan de cumplirse se quitan (horizontal girada 90° pasa a vertical). Lo
   ligado al sólido no se mueve. Partir un círculo deja dos medias vueltas (en el clic y en el
   opuesto). Copiar con Copiar no lleva restricciones con lo de afuera (tampoco los fijos).*
3. **Cotas**: punto-línea, entre paralelas, largo de arco, simétrica respecto de un eje. *Hecha el 2026-10-09:
   solver `DistancePointLine` (sin signo, como la tangencia) y `ArcLength` (barrido antihorario en
   (0, 2π]), los dos con jacobiano numérico; modelo `PointLineDistance`, `AxisDiameter` (el doble
   de la distancia, etiqueta Ø) y `ArcLength` (etiqueta ⌒). «Entre paralelas» no es una
   restricción aparte: distancia de un extremo de la segunda a la primera, más paralelas si
   faltaba. Se ofrecen en el panel según lo elegido (punto + línea, dos líneas, un arco).
   Pruebas: `model/tests/sketch_dims.rs` (6) y e2e "cotas: distancia a la línea…".*
4. **Restricciones y herramientas**: colineal, círculo por 2/3 puntos y tangente a 3,
   polígono circunscrito, ranuras en arco y por centro, rectángulo por 3 puntos. *Hecha el
   2026-10-09: `lib/sketchShapes.ts` (puro, `e2e/sketchShapes.test.mjs`) y `Collinear` en el
   modelo (paralelas + un extremo sobre la recta, sin cambios en el solver). Círculo por 2 y 3
   puntos (los clics sobre puntos existentes quedan sobre el círculo; por dos puntos, el centro
   en el medio de un diámetro de construcción), tangente a 3 (arranca con el círculo por los
   puntos más cercanos a los clics: hacer clic cerca de donde toca), polígono con botón
   Inscrito/Circunscrito (lados tangentes al círculo de construcción), ranura por el centro (eje
   de construcción con el centro en el medio), ranura en arco (eje de construcción, arcos
   concéntricos, tapas tangentes e iguales), rectángulo por 3 puntos (perpendicular y
   paralelas). Queda: la barra del sketch ya ocupa dos filas → agrupar las variantes (círculo,
   rectángulo, arco, ranura) en menús desplegables como Onshape (fase 7).*
6. **Validación del contorno**: abiertos, extremos sueltos, cruces, superposiciones; aviso en
   las operaciones. *Hecha el 2026-10-09: `lib/sketchCheck.ts` (puro, `e2e/sketchCheck.test.mjs`)
   con la misma idea que `regions.rs`: extremos unidos por posición, ramas de grado 1 = extremos
   sueltos, cruces de polilíneas que no son un extremo común de las dos (también la T sin
   partir), encimadas (misma recta o mismo círculo con tramos que se pisan). Marcas rojas en el
   visor (`SketchOverlay.problems`, encimadas en rojo), resumen en la barra del sketch y, al
   extruir o girar un sketch sin regiones, el aviso dice por qué y no crea la operación.*
7. **Visualización y lista**: mostrar/ocultar, lista de entidades editable, selección por
   cadena y por tipo; variantes de herramientas agrupadas en menús desplegables. *Hecha el
   2026-10-09: botones Cotas / Constr. / Puntos en la barra (no hay íconos de restricciones en
   el visor: solo las etiquetas de las cotas); sección «Entidades» en el panel con elegir por
   tipo (líneas, círculos y arcos, construcción, puntos, sin definir, todo), lista (un texto es
   una fila) y propiedades de lo elegido (punto X/Y; línea largo y extremos; círculo centro y
   diámetro; arco centro y radio) que mueven con `solve_drag` (las restricciones mandan);
   doble clic = cadena (`connectedChain`), Ctrl+A = todo; familias de herramientas (línea,
   rectángulo, círculo, arco, ranura) con un botón que muestra la última variante y una lista
   (▾). De paso, **modo construcción** como Onshape: botón Construcción y Q alternan lo
   elegido, o sin nada elegido prenden el modo (lo dibujado sale de construcción).*

8. **Referencias al modelo**: contorno de cara, silueta, intersección, otro sketch, romper
   vínculo. *Hecha el 2026-10-09: `cad_model::project` (aristas de cualquier tipo al plano:
   círculos inclinados como elipses, de canto como segmentos, el resto como splines de 16
   puntos, reconociendo las que resultan rectas o circulares) y puente C++ `cad_section`,
   `cad_outline`, `cad_lines_hit`, `cad_edge_points`. `SketchUse` = `edge` opcional + `source`
   (`section`, `silhouette`, `sketch {feature, entity}`). Usar (J) con clic en una cara trae su
   contorno, una arista por entidad. Silueta: líneas visibles de la vista (HLR) y se quedan los
   tramos con sólido de un solo lado (rayos normales al plano a ±ε, bordes por bisección); da
   el borde de la sombra con los agujeros pasantes, no los ciegos. Silueta e intersección se
   calculan con el sólido de antes del sketch (`cad_project_model` con rollback) y al recalcular
   se reparten entre sus entidades por tipo y cercanía (avisa si cambió la cantidad). Otro
   sketch: solo los anteriores, todas sus entidades. Lo ligado se dibuja violeta; Romper
   vínculo lo deja propio. Pruebas: `model/tests/project.rs` (7) y e2e "usar del modelo…".*

9. **Restricciones a la vista**: íconos junto a la geometría, mostrar u ocultar, elegir todas
   las restricciones o cotas, preselección, lazo. *Hecha el 2026-10-09: `lib/sketchGlyphs.ts`
   (puro, `e2e/sketchGlyphs.test.mjs`) decide símbolo y lugar (en las dos entidades si ata dos;
   la tangencia con extremo común, en el punto; patrones sin ícono); `CadView` los pone en fila
   por lo que atan (tope 500). `cadUi.selectedConstraints` (Supr las quita antes que las
   entidades; se vacía si cambia la cantidad de restricciones o al deshacer). Panel: «Todas las
   restricciones», «Todas las cotas», «Quitar elegidas» y un ícono por fila para elegirla.
   Preselección en Elegir, Recortar, Extender, Partir y Círculo tangente. El lazo va con
   **Ctrl**+arrastrar (Alt+arrastrar ya gira la vista). De paso, elegir tardaba hasta ~200 ms:
   `setSketch` liberaba los materiales antes del cuadro y three.js recompilaba los programas de
   WebGL; ahora lo reemplazado se libera después del cuadro (`CadViewer.retired`), 13–27 ms por
   clic. El doble clic mide con `e.timeStamp` (la hora del evento).*

10. **Cotas II**: ángulo suplementario, largo total, mínima y máxima con círculos, ordenadas y
    cadena, unidades en la cota, bloquear, nombre/valor/fórmula, mover el texto. *Hecha el
    2026-10-09: `Angle.supplementary` (el ángulo hasta la segunda línea invertida; en el panel
    y en el clic derecho de la cota), `CurveLength` (líneas, arcos y círculos; solver
    `CurveLength`), `CircleDistance` entre un círculo o arco y otro, un punto o una línea
    (solver `RimDistance` con signos; la mínima entre círculos es por fuera o por dentro según
    cómo están al resolver). `DimOpts` en todas las cotas: `locked` (no se edita y transformar
    no la cambia), `offset` (texto arrastrado, en mm del plano: sigue a lo acotado, con guía
    punteada) y `ordinate`. Ordenadas y cadena son acciones del panel con 3 o más puntos
    (distancias horizontales o verticales desde el primero, o cada una desde la anterior, con
    los textos en fila). Unidades en las fórmulas (`mm cm m in " ft deg ° rad`), así que una
    cota acepta `1 in`. Barra del sketch: Valor / Nombre (`d1`, `d2`… o el parámetro) /
    Fórmula. Clic derecho en una cota: bloquear, devolver el texto, suplementario, quitar.*

12. **Restricciones II**: coradial, simetría de entidades, punto en intersección, bloquear
    entidad, punto sobre spline y elipse. *Hecha el 2026-10-09: `Coradial`, `SymmetricEntities`
    (puntos, líneas, círculos, arcos, elipses y splines con la misma cantidad de puntos; los
    extremos se emparejan como están y en los arcos el reflejo da vuelta el sentido),
    `PointOnCurve` (solver `PointOnEllipse` y `PointOnCurveSpline`: la spline de Hermite con
    tangentes de Catmull-Rom que dibuja el sketch; la interpolación de OCCT del sólido puede
    diferir un poco), `Intersection` (dos «sobre» en una) y `Lock` (fijos en el lugar actual;
    transformar no mueve lo bloqueado). **Curvatura igual / G2 pasa a la fase 17**: necesita
    que la spline del sketch y la del sólido sean la misma curva (polos explícitos), si no la
    curvatura que se iguala no es la del modelo. Pruebas: Rust `model/tests/sketch_constraints2.rs`
    (10), unidades en `expr.rs`, node `sketchTransform.test.mjs` y `sketchGlyphs.test.mjs`, e2e
    "cotas II y restricciones II…".*

14. **Solver II**: grados libres por entidad a la vista, sugerir restricciones faltantes,
    definir completamente, cambios grandes de cota sin invertir, resolución parcial con
    conflicto. *Hecha el 2026-10-09: el diagnóstico devuelve la base del espacio nulo y, por
    punto con un solo grado libre, la dirección en que se mueve (`SolveReport.free_dirs`,
    `entity_dof`, `free_radius`); el visor dibuja flechas dobles amarillas (cruz en los puntos
    libres del todo, radial en los círculos con radio libre; botón «Libres»), y lo elegido dice
    cuántos grados le faltan. `cad_model::sketch_assist`: candidatas casi cumplidas (coincidentes,
    alineadas con el origen, horizontal/vertical, tangentes, perpendiculares, paralelas, mismo
    largo o radio, concéntricas, punto sobre línea o círculo) y cotas (radio, diámetro, largo,
    semiejes, posición desde el origen); solo pasan las que restringen algo nuevo: cada una se
    proyecta contra el espacio nulo, que se achica al aceptarla, y las relaciones se resuelven
    en una copia antes de seguir (sobre la geometría torcida, dos casi horizontales no son
    paralelas «gratis»). Definir todo toma primero las cotas bien condicionadas (un 20 % de la
    fila tiene que ser nuevo: largo + distancia horizontal de una línea casi horizontal define
    su altura pero un redondeo la mueve mucho). Cambios grandes: más de un 25 % (o 15°) se
    recorren en pasos geométricos (hasta 16), cada uno desde el anterior; los ángulos por el
    lado corto. Conflicto: se vuelve a resolver sin el conjunto mínimo que choca (todo lo demás
    se cumple entero, `partial`). Comandos `cad_sketch_suggest`, `cad_sketch_define`; sección
    «Definir» del panel. Pruebas: Rust `model/tests/sketch_solver2.rs` (6); e2e "solver II…".*

16. **Entidades II**: línea infinita, paralelogramo, arco elíptico, parábola, cónica, ranura en
    arco por 3 puntos, estilo y alineación del texto, texto sobre una curva. *Hecha el
    2026-10-09: `Geometry::EllipseArc` (el solver deja los extremos sobre la elipse; OCCT
    `GC_MakeArcOfEllipse`, al revés con la arista invertida) y `Geometry::BSpline` (polos,
    grado, cerrada periódica, pesos y nudos opcionales; OCCT `Geom_BSplineCurve`). La cónica
    es una B-spline de grado 2 con el peso del medio rho / (1 − rho); la parábola, rho 0,5.
    `SketchEntity.infinite` (de construcción, se dibuja de punta a punta y el anclaje la toma
    entera). Herramientas nuevas en las listas de la barra (Línea infinita, Paralelogramo,
    Arco elíptico, Spline por polos, Cónica con su rho, Parábola, Ranura arco 3 p.). Texto:
    `SketchText.style` (negrita y cursiva con Liberation Sans Bold/Italic incluidas, +1,2 MB;
    una fuente elegida se usa tal cual), alineación izquierda/centro/derecha respecto del
    clic, y con una curva elegida las letras la siguen (`layoutText`; Rehacer texto vuelve a
    seguirla, no se mueve sola si la curva cambia).*

17. **Splines**: por polos y de ajuste, manijas intermedias, agregar y quitar puntos,
    simplificar, convertir, peine de curvatura, inflexiones y radio mínimo, curvatura igual
    (G2). *Hecha el 2026-10-09: `lib/sketchCurves.ts` (evaluación NURBS con derivadas, igual
    que `cad_solver::bspline`), `lib/sketchSplines.ts`. Spline por polos con grado; Spline de
    ajuste (mínimos cuadrados con los menos polos dentro de la tolerancia); manijas en puntos
    intermedios de la spline por puntos (`Spline.handles`, OCCT `GeomAPI_Interpolate::Load`
    con tangentes por punto); Punto en spline (en una por polos inserta un nudo: la forma no
    cambia); quitar punto o polo; Simplificar; Convertir a spline por polos una cadena de
    líneas, arcos y splines por puntos (las splines exactas: cada tramo de Hermite es una
    Bézier; los arcos con ~0,03 % del radio); peine de curvatura con inflexiones y radio
    mínimo. G2: `SketchConstraint::Curvature` (tangencia en la unión + solver
    `EqualCurvature` con la curvatura con signo en el sentido de avance) entre una spline por
    polos y una línea, un arco u otra por polos; tangencia con splines por polos. Una curva
    abierta que vuelve a su comienzo cuenta como región. Las manijas de curvatura en puntos
    intermedios no: la curvatura se maneja con los polos. Pruebas: Rust
    `model/tests/sketch_curves.rs` (10), `solver/src/bspline.rs` (4), `occt.rs` (1); node
    `sketchCurves.test.mjs` (7), `sketchSplines.test.mjs` (6), `sketchText.test.mjs` (+2); e2e
    "curvas: spline por polos…".*

20. **Sketch 3D**: líneas, arcos y splines en el espacio con restricciones 3D, sketch sobre
    superficie curva, perforación. *Hecha el 2026-10-09: operación `Sketch3d`
    (`cad_model::sketch3d`): puntos xyz, línea, arco por 3 puntos, spline por puntos y punto;
    restricciones coincidente, fijo, sobre un punto del modelo (`Attach` con un `PointSpec`:
    vértice = `PointSpec::EdgeEnd` nuevo, centro, punto de referencia), sobre un plano (base,
    cara o de referencia, con desplazamiento), paralela a un eje, paralelas, perpendiculares,
    mismo largo, tangentes (línea y arco o dos arcos en el punto común), punto medio, largo,
    distancia y ángulo. Solver propio Levenberg-Marquardt denso con jacobiano numérico; el
    conflicto saca la que más falla y resuelve lo demás entero. Lo del modelo lo resuelve el
    historial (no se guarda en el documento). Los caminos (sin ramas) son alambres
    (`Evaluation.curves`, barrido por «curva») y `RefGeom::Curves` para el visor. Editor en el
    diálogo: Línea (encadenada; casi paralela a un eje queda paralela), Arco, Spline, Punto y
    Elegir con clics en el plano activo (planta, frente o lateral corrido), en un punto o en un
    vértice del sólido (queda atado: `cad_vertex_spec`); restricciones según lo elegido, lista
    con valores y conflictos en rojo. Sketch envuelto (`SurfaceSketch`): un sketch 2D común
    (mismo editor) dibujado en el plano tangente en el centro de la cara; cada curva se
    muestrea y se lleva a la superficie midiendo sobre ella (u = x / |∂S/∂u|; en un cilindro,
    el desarrollo exacto). Perforación: `SketchConstraint::Pierce { point, curve, at }`, el
    historial pone `at` donde la curva cruza el plano (`cad_plane_hits`, el cruce más cercano);
    en el panel, botones «Perforar» con un punto elegido. Puente C++: `cad_face_eval`
    (superficie, derivadas y normal en (u, v)) y `cad_plane_hits`. Pruebas: Rust
    `model/tests/sketch3d.rs` (7); e2e "sketch 3D…".*

## Fases que faltan

Orden propuesto: primero lo que más se usa a diario y lo que sostiene a lo demás.

### 11. Edición II (M)

- **Recortar con arrastre**: con Recortar, arrastrar quita cada tramo que toca el trazo.
- **Unir**: dos líneas colineales con un extremo común pasan a ser una; dos arcos del mismo
  círculo, uno.
- **Equidistante a los dos lados** y **con extremos cerrados** (una curva abierta se convierte en
  un contorno cerrado con arcos o líneas en las puntas).
- **Chaflán 2D** por distancia y por distancia y ángulo (como el redondeo de esquina).
- **Estirar**: con caja de cruce se eligen los puntos de adentro y solo esos se mueven.
- **Eliminar duplicados** (las encimadas exactas que ya marca la revisión).
- **Reparar**: unir extremos a menos de una tolerancia, quitar entidades cortísimas, partir en
  los cruces y las T sin partir (lo que marca la revisión).
- **Cerrar contorno**: unir los extremos sueltos más cercanos con una línea.

### 13. Asistencia al dibujo (M)

- **Activar o desactivar las inferencias** con un botón (Mayús sigue valiendo para un clic).
- **Rejilla del sketch**: espaciado configurable, mostrar u ocultar, anclaje a los nudos.
- **Coordenadas al dibujar**: un cuadro para el próximo punto que acepta `x, y` (absoluta),
  `@dx, dy` (relativa) y `@d < ángulo` (polar).
- **Polilínea línea-arco**: dibujando líneas, la tecla A alterna el próximo tramo a arco tangente.
- **Simetría dinámica**: con una línea de simetría activa, lo que se dibuja sale reflejado y
  atado mientras se dibuja.
- **Línea central / eje de revolución** como herramienta (construcción marcada como eje; la
  toman la revolución y el diámetro respecto del eje sin elegirla).

### 15. Plano y vista (S–M, backend)

- **Dirección horizontal del sketch** elegida con una arista o un eje.
- **Invertir la normal** del sketch.
- **Vista de corte automática** al entrar (opción recordada) y **ocultar el modelo** mientras se
  dibuja.

### 18. Archivos y fórmulas (M)

- **Curva por ecuación** explícita `y = f(x)` y paramétrica `x(t), y(t)` (spline por muestreo;
  usa los parámetros del documento).
- **Curva desde un archivo de coordenadas** (CSV o TXT).
- **Imagen de referencia (calco)** con escala (dos puntos y una medida) y rotación.
- **Importar DXF** al sketch (líneas, arcos, círculos, polilíneas, splines). DWG no: formato
  cerrado (convertir a DXF antes).
- **Exportar el sketch a DXF y SVG** (reusar `drawing.ts`).

### 19. Organización (M)

- **Copiar y pegar un sketch entero** (operación nueva con la misma geometría, en otro plano).
- **Bloques reutilizables**: un grupo de entidades con su propio origen que se inserta varias
  veces; editar el bloque cambia todas las copias. Hace las veces de **subsketch**.
- **Capas o grupos**: ocultar o bloquear por grupo.
- **Validación según la operación**: revolución (el perfil no cruza el eje), barrido (camino
  sin ramas), transición (una región por sección).

## Cobertura de la lista

✅ hecho · número = fase que lo trae.

**Soporte y planos** — ✅ planos de origen, de referencia y caras planas; cambiar el plano;
origen y ejes propios; vista normal al entrar; sketch 3D, sobre superficie curva. — 15:
dirección horizontal, invertir normal, vista de corte al entrar, ocultar el modelo.

**Entidades** — ✅ punto, línea, línea de construcción, rectángulo por 2 esquinas, por centro y
por 3 puntos, polígono inscrito y circunscrito, círculo por centro, por 2 y 3 puntos y tangente
a 3, arco por 3 puntos, por centro, tangente y tangente desde una línea, elipse, ranura recta,
recta por centro y en arco por centro, spline por puntos de paso, texto (fuente y tamaño),
línea infinita, paralelogramo, arco elíptico, parábola, cónica, ranura en arco (3 puntos),
estilo y alineación de texto, texto sobre curva, spline por puntos de control, manijas de
tangencia (la curvatura, con los polos), spline de ajuste. — 13: línea central, polilínea
línea-arco. — 18:
curvas por ecuación explícita y paramétrica, desde archivo, imagen de calco, importar DXF/DWG.

**Edición** — ✅ recortar, recortar al más cercano, extender, dividir, equidistante simple y de
cadena, redondeo 2D, simetría, patrón lineal en una y dos direcciones, circular, mover, copiar,
rotar, escalar, arrastrar respetando restricciones, construcción y de vuelta, mover puntos de
spline, tangencia en los extremos de spline, eliminar, agregar y eliminar puntos de spline,
tangencia en puntos intermedios, simplificar, convertir a spline. — 11: recortar con arrastre,
unir, equidistante bidireccional y con extremos cerrados, chaflán 2D (distancia; distancia y
ángulo), estirar, eliminar duplicados, reparar, cerrar contorno. — 13: simetría dinámica.

**Referencias al modelo** — ✅ proyectar aristas, convertir entidades del modelo, mantener
asociatividad, contorno de cara, silueta, intersección, geometría y puntos de otro sketch,
romper vínculo.

**Restricciones** — ✅ coincidente, horizontal, vertical (de línea y entre puntos), colineal,
paralelo, perpendicular, tangente, concéntrico, igual largo, igual radio, simétrico (puntos),
punto medio, punto sobre línea y círculo, fijo, tangencia en extremos de spline, alineación con
el origen, restricciones con geometría proyectada, coradial, simétrico de entidades, punto en
intersección, bloqueo de entidad, punto sobre spline y elipse, curvatura igual / G2,
perforación.

**Cotas** — ✅ horizontal, vertical, alineada, angular, radial, diametral, largo de arco,
punto-línea, entre paralelas, entre centros, simétrica respecto de un eje, impulsora, conducida,
conmutar, edición en sitio, expresiones, variables con nombre, ángulo suplementario, largo
total, entre círculos mín./máx., ordenadas, cadena, unidades en la cota, bloqueo, mostrar
nombre/valor/fórmula, reubicar el texto.

**Solucionador** — ✅ tiempo real, grados de libertad, sub/total/sobredefinido, colores por
estado, indicador global, redundantes, conflictos, lista con eliminar, arrastre estable,
sugerir restricciones, definir automáticamente, grados libres por entidad, cambios grandes sin
invertir, resolución parcial.

**Asistencia al dibujo** — ✅ inferencia automática, ajuste a extremos, medios, centros,
cuadrantes, intersecciones, tangentes, perpendiculares y geometría proyectada, líneas de
inferencia, entrada numérica (cotas al dibujar), cota automática, vista previa, dibujo
encadenado. — 13: activar/desactivar inferencias, rejilla configurable con anclaje, coordenadas
absolutas, relativas y polares.

**Perfiles y regiones** — ✅ perfiles cerrados, regiones, islas, elegir regiones, contornos
(cadena), abiertos, cruces, superpuestas, extremos sueltos, sombreado. — 19: validación según la
operación.

**Selección** — ✅ clic, ventana, cruce, cadena, por tipo, construcción, subdefinidas,
preselección resaltada, lazo, todas las restricciones, todas las cotas.

**Visualización** — ✅ mostrar/ocultar cotas, construcción y puntos, rejilla del visor, colores
por estado, trazo por tipo, zoom al sketch, mostrar/ocultar restricciones, peine de curvatura,
inflexión y radio mínimo.

**Gestión** — ✅ crear, editar, terminar, descartar, renombrar, copiar y pegar entidades entre
sketches, sketch compartido entre operaciones, deshacer y rehacer, lista de entidades, lista de
restricciones, propiedades. — 18: exportar a DXF/SVG. — 19: copiar y pegar el sketch entero,
bloques, subsketches, capas.

## Pruebas

- Funciones puras en `lib/sketch*.ts` con `node --test` (`e2e/sketch*.test.mjs`).
- Restricciones y cotas nuevas del solver con un caso definido y uno sobredefinido en
  `model/tests/sketch_dims.rs` (o un archivo por fase).
- e2e (`e2e/cad.mjs`): un escenario por fase, solo los afectados mientras se trabaja; la suite
  completa (~30 min) una vez al final.

## Riesgos

- **Proyección de intersecciones y siluetas** (fase 8): curvas B-spline de OCCT a geometría
  del sketch; aproximar con splines y avisar si quedan muchas.
- **Solver denso** (~800 incógnitas ≈ 4 s): las fases 14 y 17 suman incógnitas; si hace falta,
  pasar a jacobiano disperso (`jacobian_sparse` ya existe en el sistema). Sugerir y definir
  todo proyectan contra una base densa n × n (cuadrático por ecuación): con miles de
  incógnitas tardan segundos.
- **Glifos de restricciones** (fase 9): con sketches grandes tapan el dibujo; agrupar por
  entidad y ocultar al alejar.
- **Sketch 3D** (fase 20, hecha): el solver 3D es denso con jacobiano numérico (sketches de
  decenas de puntos); no se arrastra en el visor (se dibuja con clics y se ajusta con números).
