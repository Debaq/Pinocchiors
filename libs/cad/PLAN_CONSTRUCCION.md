# Geometría de referencia y operaciones que faltan

Objetivo: completar el catálogo de operaciones de uso diario en Onshape: planos, ejes y puntos
de referencia; barrido y transición con interfaz; agujero; extrusión con todas sus variantes;
nervio, hélice, mover cara y engrosar.

## Qué tenemos hoy

- Operaciones (`FeatureKind`): sketch, extrusión, revolución, primitiva, redondeo, chaflán,
  vaciado, desmoldeo, patrón (lineal y circular), simetría, partir, importar.
- `Extent`: ciega, simétrica, a través de todo, hasta una cara.
- Planos de sketch (`PlaneSpec`): XY, XZ, YZ, cara plana del sólido, plano a medida (`Custom`);
  `offsetPlane` en la interfaz para desplazar uno base.
- En el puente OCCT ya existen `sweep` (`BRepOffsetAPI_MakePipe`) y `loft`
  (`BRepOffsetAPI_ThruSections`), sin operación en el modelo ni interfaz.
- Ejes para revolución y patrón circular (`AxisSpec`) limitados a los ejes base y aristas.

## Qué falta

### Geometría de referencia

| Elemento | Formas de crearlo (como Onshape) |
|---|---|
| Plano | Desplazado de un plano o cara; en ángulo respecto de un plano alrededor de una arista; plano medio entre dos caras; por 3 puntos; normal a una curva en un punto; tangente a un cilindro |
| Eje | Por dos puntos; arista recta; eje de un cilindro; intersección de dos planos |
| Punto | Vértice; centro de arco; en una curva a una distancia; intersección curva-plano |

Cada uno es una operación del historial (`FeatureKind::Plane/Axis/Point`), se ve en el visor,
se puede elegir como plano de sketch, eje de revolución o patrón, y se oculta con el ojo.

### Operaciones

| Operación | Detalle | OCCT |
|---|---|---|
| Barrido | Perfil (región de sketch) a lo largo de un camino (aristas de sketch o del sólido); modo "mantener orientación" o "seguir camino" | `BRepOffsetAPI_MakePipeShell` (ya enlazado en TKOffset) |
| Transición (*loft*) | Varias regiones en planos distintos; curvas guía opcionales; reglada o suave | `BRepOffsetAPI_ThruSections` (ya en el puente) |
| Agujero | Sobre puntos de sketch: simple, avellanado, con caja (counterbore); profundidad o pasante; tamaños estándar M2–M12 y rosca cosmética | Cilindros + conos restados; rosca solo como dato (cosmética) |
| Extrusión: dos direcciones | Distancia distinta a cada lado | Dos prismas unidos |
| Extrusión: hasta la siguiente | Hasta la primera cara que encuentra | `BRepFeat_MakePrism` con `Perform(Until)` |
| Extrusión: hasta un vértice / desplazada de cara | Variantes de `UpToFace` | Plano auxiliar |
| Extrusión con desmoldeo | Ángulo de las paredes | `BRepOffsetAPI_DraftAngle` o `LocOpe_DPrism` |
| Extrusión delgada | Pared de espesor dado a partir de un contorno abierto o cerrado | Equidistante 2D + extrusión |
| Nervio (*rib*) | Línea de sketch → pared que llega hasta el sólido | `BRepFeat_MakeLinearForm` |
| Hélice | Por eje, paso, vueltas; para resortes y roscas con barrido | `Geom_CylindricalSurface` + línea 2D |
| Mover cara | Desplazar o girar caras elegidas, recalculando las vecinas | `BRepOffsetAPI_MakeOffsetShape` por cara / `BRepFeat` |
| Reemplazar cara | Llevar una cara hasta otra superficie | Igual |
| Engrosar | Dar espesor a una superficie o cara | `BRepOffset_MakeOffset` (modo sólido) |
| Patrón en curva | Copias a lo largo de un camino | Transformaciones por parámetro de la curva |
| Patrón de caras / de piezas | Patrón que repite caras en vez de operaciones | Igual que el actual con otra herramienta |
| Escala | Escalar el sólido (uniforme o por eje) | `BRepBuilderAPI_GTransform` |

## Diseño

- Todas las operaciones nuevas siguen el patrón de `eval.rs`: construyen la herramienta como
  `Tagged` con orígenes de caras propios (`"barrido:pared:<entidad>"`, `"agujero:<k>:fondo"`,
  …) y se combinan con `BodyOp` (y con el alcance de piezas cuando exista
  [PLAN_PIEZAS.md](PLAN_PIEZAS.md)).
- Las funciones nuevas del puente C llevan su prueba en `occt/tests/occt.rs` y su versión en
  el stub (devuelven "sin OCCT").
- Geometría de referencia: `Evaluation.references: Vec<RefGeom { id, kind, plane | axis | point }>`
  separada del sólido; `PlaneSpec::Reference(FeatureId)` y `AxisSpec::Reference(FeatureId)`.
- Interfaz: cada operación con su diálogo según [PLAN_EDICION.md](PLAN_EDICION.md); hasta que
  exista, como las actuales (crear con lo elegido y ajustar campos).
- Agujero: tabla de tamaños estándar en el frontend (métrica ISO: diámetro libre, de rosca, de
  caja y avellanado por M) y la elección de puntos del sketch como ubicación.

## Fases

1. **Planos, ejes y puntos de referencia** (las formas más comunes: desplazado, en ángulo,
   medio, 3 puntos; eje por 2 puntos y de cilindro). *Hecha el 2026-10-07: `FeatureKind::Plane { def: PlaneDef }`
   (desplazado, en ángulo alrededor de un eje, medio entre dos paralelos, por tres puntos),
   `Axis { def: AxisDef }` (dos puntos, arista, eje de cilindro o cono, cruce de dos planos) y
   `Point { def: PointSpec }` (coordenadas, centro de arista, sobre arista recta, otro punto).
   `PlaneSpec::Reference` y `AxisSpec::Reference` los usan sketches, simetrías, desmoldes,
   revoluciones y patrones circulares; dependencias por referencia. `Evaluation.references` (y
   la caché las guarda), `CadResult.references`. Visor: planos ámbar translúcidos elegibles
   (para el sketch, en el modo de elegir lugar y en la selección), ejes punteados y puntos; se
   ocultan con el ojo del árbol. Diálogos con `PlaneField`/`AxisField`/`PointField`; "Plano",
   "Eje" y "Punto" en Agregar arrancan de lo elegido.*
2. **Extrusión completa**: dos direcciones, hasta la siguiente, desmoldeo, delgada. *Hecha el
   2026-10-07: `Extent::TwoSides { distance, second }` y `Extent::UpToNext` (rayos desde el
   interior y los bordes del perfil, `cad_ray_hit` con `IntCurvesFace_ShapeIntersector`);
   `Extrude.draft` (grados, `cad_draft_prism` con `LocOpe_DPrism`, que mide la altura sobre la
   pared inclinada: se divide por cos; hacia atrás cambia el signo del ángulo; simétrica y dos
   direcciones se angostan desde el plano hacia cada lado) y `Extrude.thin` (anillo entre el
   perfil desplazado ±t/2 con `BRepOffsetAPI_MakeOffset`, esquinas de afuera redondeadas). Con
   desmolde o delgada las caras laterales no llevan el origen `lado:` (la sonda queda fuera de
   la pared); las tapas sí.*
3. **Barrido y transición** con interfaz (el puente ya existe). *Hecha el 2026-10-07: `FeatureKind::Sweep(Sweep { sketch,
   regions, path: SweepPath::Sketch { sketch, entities }, op })` (camino encadenado por los
   extremos de líneas, arcos y splines abiertas, orientado para arrancar del lado del perfil)
   y `Loft(Loft { sections, ruled, op })` (una región por sketch). El puente barre caras con
   `BRepOffsetAPI_MakePipeShell` en modo esquina a inglete (`MakePipe` dejaba sólidos
   inválidos en esquinas vivas) y resta los agujeros barridos aparte. Orígenes: "inicio" (y
   "fin" en la transición) por la muestra de la región; el resto por número de cara.
   "Barrido" arma el perfil con las regiones elegidas (o el último sketch con regiones) y el
   camino con otro sketch; "Transición", con los dos últimos sketches con regiones.*
4. **Agujero** con tamaños estándar. *Hecha el 2026-10-07: `FeatureKind::Hole(Hole { sketch, points,
   diameter, depth: Blind | ThroughAll, style: Simple | Counterbore | Countersink, tip_angle,
   thread })`: centros en los puntos sueltos del sketch (o los de `points`, o los centros de
   sus círculos), contra la normal del plano; la herramienta (cilindro, punta de broca, caja o
   cono) arranca apenas por encima de la cara y se resta respetando el alcance. Orígenes
   `agujero:k:pared` y `agujero:k:caja`. Interfaz: tabla métrica ISO M2–M12 (pasante holgado o
   para roscar, con rosca cosmética; caja ISO 4762 y avellanado a 90° ISO 10642). E2E:
   avellanado M6 en una placa y medir la pared: diámetro 6,6 mm.*
5. **Hélice** (y ejemplo de rosca/resorte con barrido), **nervio**, **engrosar**. *Hélice y engrosar hechos el
   2026-10-07: `FeatureKind::Helix { axis, radius, pitch, turns, left }` (alambre sobre un
   cilindro, `cad_make_helix`; se ve como curva y es camino de barridos con
   `SweepPath::Curve`; un resorte = círculo en el arranque + barrido) y `Thicken { faces,
   thickness, op }` (`BRepOffset_MakeOffset` en modo engrosar). Nervio hecho el 2026-10-08 sin
   `BRepFeat_MakeLinearForm`: `Rib { sketch, thickness, flip }`, por cada línea del sketch 33
   rayos en su plano hasta el sólido (`ray_hit`), el polígono entre la línea y lo que tocan
   (un poco adentro para que la unión no quede apenas tocando) con el espesor centrado en el
   plano; prueba el otro lado si de uno no llega, y si no llega en todo el largo es error.*
6. **Mover y reemplazar cara**, **escala**, **patrón en curva**. *Hecha el 2026-10-07 (reemplazar cara el 2026-10-08: `ReplaceFace { faces, target }`, la cara
   barrida de sobra hacia el plano y recortada en él con `split_keep`; suma o resta según el lado;
   plano destino = cara plana, de referencia o propio, también inclinado):
   `MoveFace { faces, distance }` (caras planas: el prisma que barre la cara se suma o se
   resta de su pieza; conserva historia y orígenes; caras no planas, error claro),
   `Scale { factor: [sx, sy, sz], center }` (transformación general; el puente ahora detecta
   si la matriz es semejanza antes de usar `gp_Trsf`, que con una escala no uniforme se
   quedaba solo con la traslación) y `PatternKind::Curve { path, count }` (copias trasladadas
   a distancias iguales de punta a punta del camino, `cad_wire_sample` con
   `GCPnts_UniformAbscissa`).*

## Pruebas

- OCCT: cada operación nueva con volumen esperado en un caso simple (barrido recto = extrusión;
  transición de dos cuadrados iguales = prisma; agujero con caja: volumen restado exacto;
  hélice de N vueltas: largo analítico).
- Modelo: orígenes de caras de cada operación nueva y una prueba de nombres robustos (cambiar
  una cota previa y que el redondeo sobre una cara del barrido siga ahí).
- E2E: agujero avellanado en una placa y medir su diámetro con la herramienta de medir.

## Riesgos

- **Robustez de OCCT** en barridos con caminos con esquinas y en transiciones con perfiles de
  distinto número de aristas: validar el sólido (`BRepCheck`) y dar mensajes claros.
- **Mover cara** es frágil en OCCT; puede quedar limitado a casos planos al principio.
- **Tamaño del binario** si alguna operación exige toolkits nuevos (revisar `TOOLKITS` en
  `occt/build.rs` y `scripts/build-occt.sh`).
