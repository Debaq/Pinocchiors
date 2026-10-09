# Sketch: igualar la lista completa de un CAD

Plan del sketch, rearmado el 2026-10-09 para cubrir **toda** la lista de funciones de sketch
de un CAD de referencia (Onshape, SolidWorks, Fusion) que entregó el usuario. Reemplaza a los
planes anteriores (sketch completo y anclajes). Las fases 1–4, 6 y 7 están hechas; la 5 pasó a
ser la 8. Lo hecho está también en la bitácora de [ROADMAP.md](ROADMAP.md). Al final, la
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

## Fases que faltan

Orden propuesto: primero lo que más se usa a diario y lo que sostiene a lo demás.

### 8. Referencias al modelo (M, C++) — antes fase 5

- **Contorno de cara**: elegir una cara plana del sólido y proyectar todas sus aristas (exterior
  e islas) como `uses` ligados.
- **Silueta del cuerpo**: el contorno del sólido visto en la normal del plano (las líneas
  ocultas de los planos 2D ya calculan siluetas con HLR: reusar).
- **Intersección** del plano del sketch con el sólido (`BRepAlgoAPI_Section`), ligada.
- **Otro sketch**: proyectar curvas y puntos de un sketch anterior; `SketchUse` pasa a tener
  origen arista del sólido o entidad de un sketch, y se mueve si el otro cambia.
- **Romper vínculo**: el `use` se quita y la geometría queda propia y editable.
- Riesgo: curvas B-spline de OCCT a geometría del sketch (aproximar con splines, como las elipses).

### 9. Restricciones a la vista (M)

- **Íconos de restricciones** junto a la geometría (horizontal, vertical, paralelas,
  perpendiculares, tangente, concéntrico, igual, coincidente, simétrico, fijo…), como
  etiquetas HTML igual que las cotas; clic elige la restricción y resalta lo que ata; Supr la
  borra.
- **Mostrar u ocultar restricciones** (botón junto a Cotas / Constr. / Puntos).
- **Elegir todas las restricciones** y **todas las cotas**.
- **Preselección**: resaltar la entidad o el punto bajo el mouse en el visor.
- **Selección por lazo**: arrastrar con Alt dibuja un lazo libre.

*Hecha el 2026-10-09: `lib/sketchGlyphs.ts` (puro, `e2e/sketchGlyphs.test.mjs`) decide símbolo y
lugar (en las dos entidades si ata dos; la tangencia con extremo común, en el punto; patrones
sin ícono); `CadView` los pone en fila por lo que atan (tope 500). `cadUi.selectedConstraints`
(Supr las quita antes que las entidades; se vacía si cambia la cantidad de restricciones o al
deshacer). Panel: «Todas las restricciones», «Todas las cotas», «Quitar elegidas» y un ícono
por fila para elegirla. Preselección en Elegir, Recortar, Extender, Partir y Círculo
tangente. El lazo va con **Ctrl**+arrastrar (Alt+arrastrar ya gira la vista). De paso, elegir
tardaba hasta ~200 ms: `setSketch` liberaba los materiales antes del cuadro y three.js
recompilaba los programas de WebGL; ahora lo reemplazado se libera después del cuadro
(`CadViewer.retired`), 13–27 ms por clic. El doble clic mide con `e.timeStamp` (la hora del
evento).*

### 10. Cotas II (M, backend)

- **Ángulo suplementario** (180° − el ángulo; el lado del ángulo elegido con el clic).
- **Largo total de una curva** (cadena de líneas, arcos y splines): restricción nueva en el
  solver (suma de largos, jacobiano numérico).
- **Entre círculos**: mínima y máxima además de entre centros (|c₁c₂| ∓ r₁ ∓ r₂).
- **Cotas de ordenadas** (un origen y distancias horizontales o verticales a varios puntos,
  dibujadas en fila) y **cotas en cadena** (cada una desde la anterior).
- **Unidades dentro de la cota**: escribir `1 in`, `2 cm`, `30 mm`, `45 deg`.
- **Bloquear una cota** (no se edita por error; transformar no la cambia).
- **Mostrar nombre, valor o fórmula** (cada cota con nombre `d1`, `d2`… o el del parámetro).
- **Mover el texto de la cota** arrastrándolo; el desplazamiento se guarda en la cota (campo
  nuevo en el modelo, el solver lo ignora).

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

### 12. Restricciones II (M, backend)

- **Coradial** (mismo centro y mismo radio).
- **Simétrico de entidades** (líneas, arcos y círculos respecto de una línea; hoy solo puntos).
- **Punto en intersección** como restricción propia (hoy sale del anclaje, como dos «sobre»).
- **Bloquear entidad completa** (todos sus puntos y su radio).
- **Igual curvatura / continuidad G2** entre una spline y un arco u otra spline.
- **Punto sobre spline y sobre elipse** (el solver ya tiene `PointOnSpline`).

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

### 14. Solver II (L, backend)

- **Grados libres por entidad a la vista** (qué se puede mover y hacia dónde).
- **Sugerir restricciones faltantes** (casi horizontal, casi coincidentes, cotas que faltan).
- **Definir completamente de forma automática** (cotas desde el origen y relaciones obvias,
  como «Fully Define» de SolidWorks).
- **Cambios grandes de cota sin invertir la geometría** (pasos graduales cuando el salto es
  grande, y elegir la solución más cercana a la anterior).
- **Resolución parcial con conflicto**: resolver lo que se pueda y marcar lo que choca (hoy no
  se mueve nada).

### 15. Plano y vista (S–M, backend)

- **Dirección horizontal del sketch** elegida con una arista o un eje.
- **Invertir la normal** del sketch.
- **Vista de corte automática** al entrar (opción recordada) y **ocultar el modelo** mientras se
  dibuja.

### 16. Entidades II (M, backend + C++)

- **Línea infinita** (de construcción, para referencias).
- **Paralelogramo** (como el rectángulo por 3 puntos, sin el ángulo recto).
- **Arco elíptico** (`GC_MakeArcOfEllipse`).
- **Parábola** y **cónica por factor rho** (B-spline racional; la parábola es rho 0,5).
- **Ranura en arco por 3 puntos** (hoy está la de centro).
- **Texto**: estilo (negrita, cursiva) y alineación (izquierda, centro, derecha); **texto sobre
  una curva** (los glifos siguen una línea o un arco).

### 17. Splines (M–L)

- **Por puntos de control** (B-spline con polos y grado) y **de ajuste** (a partir de puntos
  existentes, con tolerancia).
- **Manijas de tangencia y curvatura en puntos intermedios** (hoy solo en los extremos).
- **Agregar y quitar puntos** de una spline; **simplificar** (menos puntos dentro de una
  tolerancia); **convertir entidades a spline**.
- **Peine de curvatura**, **puntos de inflexión** y **radio mínimo**.

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

### 20. Sketch 3D (L, backend + C++)

- **Sketch 3D**: líneas, arcos y splines en el espacio con restricciones 3D (paralelo a un eje,
  sobre un plano, coincidente con vértices del sólido).
- **Sketch sobre superficie curva**: curvas en las coordenadas UV de una cara.
- **Perforación**: un punto del sketch que atraviesa una curva fuera de su plano (para barridos).

## Cobertura de la lista

✅ hecho · número = fase que lo trae.

**Soporte y planos** — ✅ planos de origen, de referencia y caras planas; cambiar el plano;
origen y ejes propios; vista normal al entrar. — 15: dirección horizontal, invertir normal, vista
de corte al entrar, ocultar el modelo. — 20: sketch 3D, sobre superficie curva.

**Entidades** — ✅ punto, línea, línea de construcción, rectángulo por 2 esquinas, por centro y
por 3 puntos, polígono inscrito y circunscrito, círculo por centro, por 2 y 3 puntos y tangente
a 3, arco por 3 puntos, por centro, tangente y tangente desde una línea, elipse, ranura recta,
recta por centro y en arco por centro, spline por puntos de paso, texto (fuente y tamaño). — 13:
línea central, polilínea línea-arco. — 16: línea infinita, paralelogramo, arco elíptico,
parábola, cónica, ranura en arco (3 puntos), estilo y alineación de texto, texto sobre curva. —
17: spline por puntos de control, manijas de tangencia y curvatura, spline de ajuste. — 18:
curvas por ecuación explícita y paramétrica, desde archivo, imagen de calco, importar DXF/DWG.

**Edición** — ✅ recortar, recortar al más cercano, extender, dividir, equidistante simple y de
cadena, redondeo 2D, simetría, patrón lineal en una y dos direcciones, circular, mover, copiar,
rotar, escalar, arrastrar respetando restricciones, construcción y de vuelta, mover puntos de
spline, tangencia en los extremos de spline, eliminar. — 11: recortar con arrastre, unir,
equidistante bidireccional y con extremos cerrados, chaflán 2D (distancia; distancia y ángulo),
estirar, eliminar duplicados, reparar, cerrar contorno. — 13: simetría dinámica. — 17: agregar y
eliminar puntos de spline, tangencia en puntos intermedios, simplificar, convertir a spline.

**Referencias al modelo** — ✅ proyectar aristas, convertir entidades del modelo, mantener
asociatividad. — 8: contorno de cara, silueta, intersección, geometría y puntos de otro sketch,
romper vínculo.

**Restricciones** — ✅ coincidente, horizontal, vertical (de línea y entre puntos), colineal,
paralelo, perpendicular, tangente, concéntrico, igual largo, igual radio, simétrico (puntos),
punto medio, punto sobre línea y círculo, fijo, tangencia en extremos de spline, alineación con
el origen, restricciones con geometría proyectada. — 12: curvatura igual / G2, igual curvatura,
coradial, simétrico de entidades, punto en intersección, bloqueo de entidad, punto sobre
spline y elipse. — 20: perforación.

**Cotas** — ✅ horizontal, vertical, alineada, angular, radial, diametral, largo de arco,
punto-línea, entre paralelas, entre centros, simétrica respecto de un eje, impulsora, conducida,
conmutar, edición en sitio, expresiones, variables con nombre. — 10: ángulo suplementario, largo
total, entre círculos mín./máx., ordenadas, cadena, unidades en la cota, bloqueo, mostrar
nombre/valor/fórmula, reubicar el texto.

**Solucionador** — ✅ tiempo real, grados de libertad, sub/total/sobredefinido, colores por
estado, indicador global, redundantes, conflictos, lista con eliminar, arrastre estable. — 14:
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

**Selección** — ✅ clic, ventana, cruce, cadena, por tipo, construcción, subdefinidas. — 9:
preselección resaltada, lazo, todas las restricciones, todas las cotas.

**Visualización** — ✅ mostrar/ocultar cotas, construcción y puntos, rejilla del visor, colores
por estado, trazo por tipo, zoom al sketch. — 9: mostrar/ocultar restricciones. — 17: peine de
curvatura, inflexión y radio mínimo.

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
  pasar a jacobiano disperso (`jacobian_sparse` ya existe en el sistema).
- **Glifos de restricciones** (fase 9): con sketches grandes tapan el dibujo; agrupar por
  entidad y ocultar al alejar.
- **Sketch 3D** (fase 20): es casi un modo nuevo (solver con puntos 3D); dejarlo al final.
