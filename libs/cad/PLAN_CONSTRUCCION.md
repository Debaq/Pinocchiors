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
   medio, 3 puntos; eje por 2 puntos y de cilindro).
2. **Extrusión completa**: dos direcciones, hasta la siguiente, desmoldeo, delgada.
3. **Barrido y transición** con interfaz (el puente ya existe).
4. **Agujero** con tamaños estándar.
5. **Hélice** (y ejemplo de rosca/resorte con barrido), **nervio**, **engrosar**.
6. **Mover y reemplazar cara**, **escala**, **patrón en curva**.

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
