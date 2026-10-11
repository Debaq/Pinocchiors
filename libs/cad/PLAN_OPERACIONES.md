# Operaciones: igualar la lista completa de un CAD

Plan del CAD paramétrico desde el 2026-10-09 (el plano 2D va aparte, en
[PLAN_PLANO_2D.md](PLAN_PLANO_2D.md), desde el 2026-10-10). Reemplaza a todos los planes anteriores
(construcción, edición, ensambles, escaneo, futuro, inspección, objetos, piezas, planos 2D,
recálculo, sketch, visor y el índice PLANES.md), que estaban hechos o casi. Lo hecho queda en la
bitácora de [ROADMAP.md](ROADMAP.md). Sale de comparar el modelo (`model/src/feature.rs`) y la
interfaz de Diseñar con la lista de operaciones de un CAD de referencia que entregó el usuario.

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta · (F*n*) fase de este plan que lo cubre.

## Cobertura de la lista

### Operaciones a partir de croquis
| Función | Estado |
|---|---|
| Extrusión / corte por extrusión | ✅ (`Extrude`, `op: Cut`) |
| Revolución / corte por revolución | ✅ |
| Barrido / corte por barrido | ✅ |
| Recubrimiento / corte por recubrimiento | ✅ (`Loft`, también reglado) |
| Nervio | ✅ (`Rib`) |
| Agujero | ✅ |
| Rosca | ✅ (cosmética, modelada y coordinada con `ThreadLink`) |
| Envolver croquis sobre cara (grabado y relieve) | 🟡 las curvas se llevan a la cara (`SurfaceSketch`); falta grabar/relieve (F7) |

### Opciones de extrusión
| Función | Estado |
|---|---|
| Ciega, simétrica, dos direcciones, hasta el siguiente, pasante | ✅ |
| Hasta vértice | ✅ (`Extent::UpToVertex`, 2026-10-11) |
| Hasta superficie | 🟡 solo hasta el plano de la cara (F3) |
| Hasta superficie con desfase | ❌ (F3) |
| Desde plano desfasado | 🟡 el desfase es del sketch, no de la extrusión (F3) |
| Ángulo de desmoldeo, pared delgada, invertir, selección de regiones | ✅ |
| Dirección personalizada | ❌ (F3) |

### Opciones de revolución
| Función | Estado |
|---|---|
| Completa, por ángulo, selección de eje | ✅ |
| Simétrica, dos ángulos | ✅ (`Revolve.extent`, 2026-10-11) |
| Pared delgada | ✅ (`Revolve.thin`, 2026-10-11) |
| Hasta superficie | ❌ (F3) |

### Opciones de barrido
| Función | Estado |
|---|---|
| Perfil y trayectoria | ✅ (sketch, sketch 3D, hélice) |
| Curvas guía, torsión, orientación, barrido circular simple, pared delgada | ❌ (F4) |

### Opciones de recubrimiento
| Función | Estado |
|---|---|
| Entre varios perfiles | ✅ |
| Curvas guía, línea central, tangencia en extremos, a punto, cerrado, pared delgada | ❌ (F5) |

### Opciones de agujero
| Función | Estado |
|---|---|
| Simple, avellanado, abocardado, roscado, ciego o pasante | ✅ |
| Cónico | ✅ (`HoleStyle::Tapered`, 2026-10-11) |
| Tabla de estándares | ✅ métrico ISO y pulgadas UNC/UNF de #2 a 1/2" (2026-10-11) |

### Operaciones sobre aristas y caras
| Función | Estado |
|---|---|
| Redondeo de radio constante | ✅ |
| Redondeo de radio variable | 🟡 lineal de `radius` a `radius2`; faltan radios por punto (F6) |
| Redondeo de cara, redondeo completo | ❌ (F6) |
| Chaflán por distancia, dos distancias, distancia y ángulo | ✅ |
| Chaflán de vértice | ❌ (F6) |
| Vaciado | ✅ |
| Vaciado con espesores distintos por cara | ❌ (F6) |
| Ángulo de desmoldeo, engrosar | ✅ |
| Desfasar cara, mover cara | 🟡 solo planas y a lo largo de la normal (F6) |
| Rotar cara, eliminar cara | ❌ (F6) |
| Reemplazar cara | 🟡 solo hasta un plano (F6) |

### Patrones y simetría
| Función | Estado |
|---|---|
| Lineal, circular, a lo largo de curva, relleno de región | ✅ (además por tabla) |
| Lineal en dos direcciones | ✅ (`Linear.second`, 2026-10-11) |
| Guiado por puntos de croquis | ✅ (`PatternKind::Points`, 2026-10-11) |
| Omitir instancias | 🟡 escritas en el diálogo (`Pattern.skip`); falta prender y apagar con clic en el visor (F2) |
| Simetría de operaciones | ✅ |
| Simetría de cuerpos | ✅ unido o como pieza aparte (`Mirror.op`, 2026-10-11) |

### Operaciones entre cuerpos
Unión, resta, intersección, dividir, separar no conectados, eliminar: ✅ todas.

### Transformaciones
| Función | Estado |
|---|---|
| Mover, rotar, copiar (desplazamiento, giro o de punto a punto) | ✅ (`Transform`, 2026-10-11); falta el manipulador (F1) |
| Alinear a cara o plano | ❌ (F1) |
| Escalar uniforme y no uniforme | ✅ (`Scale.factor` por eje) |

### Geometría de referencia
| Función | Estado |
|---|---|
| Plano desfasado, en ángulo, por 3 puntos, medio; eje; punto | ✅ |
| Plano tangente, plano normal a curva, sistema de coordenadas | ❌ (F8) |

### Curvas 3D
| Función | Estado |
|---|---|
| Hélice | ✅ |
| Espiral | ✅ plana y hélice cónica (`Helix.radius2`, `flat`; spline por puntos, 2026-10-11) |
| Curva por puntos | 🟡 splines del sketch 3D (F8) |
| Curva proyectada, curva de intersección | 🟡 solo hacia el plano de un sketch (F8) |
| Línea de partición | ❌ (F8) |
| Curva compuesta | 🟡 el barrido encadena; no hay operación propia (F8) |

### Superficies
| Función | Estado |
|---|---|
| Por extrusión, por revolución, relleno, recortar, coser, cortar sólido con superficie | ✅ |
| Por barrido | ❌ (F4) |
| Por recubrimiento | ❌ (F5) |
| Plana | ✅ (`SurfacePlanar`, 2026-10-11) |
| Desfasada | ✅ (`SurfaceOffset`; de un sólido, su piel, 2026-10-11) |
| Extender | ❌ (F9) |

### Gestión de operaciones
| Función | Estado |
|---|---|
| Editar, editar croquis, suprimir, reordenar, eliminar, renombrar | ✅ |
| Copiar y pegar | ❌ (solo «Pegar sketch») (F2) |
| Retroceder (rollback), vista previa en vivo | ✅ |
| Manipuladores gráficos | 🟡 flecha de extrusión y agujero (F10) |
| Ámbito, fusionar o crear cuerpo nuevo | ✅ (`scope`, `BodyOp::New`) |

## Fases

Cada fase: modelo (`cad-model`), puente C++ si hace falta (`occt/cpp/cad_occt.cpp`), comando en
`apps/desktop/src/cad.rs`, diálogo en `DesignStep.tsx`, pruebas en `model/tests` y e2e solo de
lo nuevo (ver la regla de e2e dirigido). Campos nuevos siempre con `#[serde(default)]` para que
abran los proyectos viejos.

### F1 — Transformar cuerpos

Hecho el 2026-10-11: `Transform` con `Translate`, `Rotate` y `Points`, `copy`, y `Mirror.op`
(«Mover cuerpo», «Girar cuerpo» y «Copiar cuerpo» en Pieza; «El reflejo como pieza aparte» en la
simetría). Falta: `Motion::Align` y el manipulador de flechas y arcos.

- `FeatureKind::Transform { parts: Vec<PartId>, motion: Motion, copy: bool }` con
  `Motion::Translate { vector }`, `Rotate { axis: AxisSpec, angle }`,
  `Align { from: FaceRef | PlaneSpec, to: PlaneSpec, flip }` (lleva una cara plana sobre un
  plano: normal contra normal y el centro de la cara al plano) y `Points { from, to: PointSpec }`.
- `copy: true` deja la original y agrega la copia como pieza nueva.
- `gp_Trsf` + `BRepBuilderAPI_Transform` (sin copiar geometría si es rígida). Las `FaceRef`
  posteriores se resuelven por geometría: los nombres siguen funcionando.
- Simetría de cuerpos: `Mirror` gana `op: BodyOp` (por defecto `Join`, como hoy; `New` deja el
  reflejo como pieza aparte).
- UI: «Mover / Girar / Copiar / Alinear» en la barra; manipulador de tres flechas y tres arcos
  (reusar `setHandle`).

### F2 — Copiar y pegar operaciones; patrones que faltan

Hecho el 2026-10-11: `Linear.second` (grilla; la copia k = j·cantidad + i) y `Pattern.skip`
escrito en el diálogo («Omitir copias»). Falta: copiar y pegar, `PatternKind::Points` y omitir
con clic en el visor.

- Copiar: lo elegido en el árbol va al portapapeles de `cadUi` con sus dependencias internas.
  Pegar renumera ids (`FeatureId`) y vuelve a apuntar las referencias internas; las externas
  quedan igual si existen, y si no, la operación queda con referencia perdida (ya se muestra).
  Ctrl+C / Ctrl+V en el árbol y «Duplicar» en el menú del clic derecho.
- `PatternKind::Linear { second: Option<Direction2 { direction, count, spacing }> }`.
- `PatternKind::Points { sketch, points: Vec<u32> }`: una copia por punto suelto del sketch
  (trasladada desde el primero o desde el centro de la operación original).
- `Pattern.skip: Vec<u32>` (índices de copia); en el visor, clic en el punto de una copia la
  apaga o la prende.

### F3 — Extrusión y revolución completas

Hecho el 2026-10-11: `Extent::UpToVertex` («Hasta un punto») y `Revolve.extent`
(`One | Symmetric | TwoAngles { second }`, sin migrar `angle`: queda como el ángulo hacia el
primer lado o el total en la simétrica). Falta: hasta superficie con desfase o curva, dirección
personalizada, desde plano desfasado, revolución hasta una cara y delgada.

- `Extent::UpToVertex { point: PointSpec }` (distancia = proyección sobre la dirección).
- `Extent::UpToFace { face, offset: f64 }`; cara curva: extruir pasante y quedarse con el lado
  de acá de la cara (`BRepAlgoAPI_Splitter` + elegir pedazos por el lado), o
  `BRepFeat_MakePrism` con cara límite.
- `Extrude.direction: Option<AxisSpec>` (dirección personalizada; el desmoldeo se mide contra
  ella) y `Extrude.start: Option<StartSpec { Offset(f64) | Plane(PlaneSpec) }>`.
- `Revolve.extent: RevolveExtent { Angle, Symmetric, TwoAngles { second }, UpToFace { face } }`
  (migrar `angle` con serde) y `Revolve.thin: Option<f64>` (mismo engrosado de contorno que la
  extrusión delgada).

### F4 — Barrido completo (+ superficie por barrido)

- `BRepOffsetAPI_MakePipeShell` (ya se usa):
  - Orientación `SweepOrient { Follow, Fixed, Frenet, Normal { dir } }` → `SetMode`.
  - Curvas guía: `SetMode(guide wire, KeepContact)`; con dos guías, escala y giro del perfil.
  - Torsión: `SetLaw` con ley lineal de giro (vueltas o grados totales).
- `Sweep.profile: SweepProfile { Sketch {..} | Circle { radius } }`: barrido circular simple sin
  sketch de perfil (tubos, cables).
- `Sweep.thin: Option<f64>`.
- `SurfaceSweep`: igual, con curvas abiertas y sin `MakeSolid`.

### F5 — Recubrimiento completo (+ superficie por recubrimiento)

- A punto: `LoftSection::Point { point: PointSpec }` al comienzo o al final
  (`ThruSections::AddVertex`).
- Línea central y curvas guía: pasar a `MakePipeShell` con varias secciones (`Add` de cada
  perfil sobre el camino) cuando hay guías o línea central; `ThruSections` cuando no.
- Tangencia en extremos: `LoftEnd { Free, Normal { weight }, TangentToFace { face, weight } }`.
  `ThruSections` no la soporta: secciones auxiliares desplazadas o `GeomFill` (riesgo).
- Cerrado: volver a la primera sección (unión C0 en la costura; avisar).
- `Loft.thin: Option<f64>`; `SurfaceLoft` sin `isSolid`.

### F6 — Caras, redondeos y chaflanes

- Eliminar cara: `BRepAlgoAPI_Defeaturing` (cierra el hueco extendiendo las vecinas).
- Mover y rotar cara: caras planas → plano transformado (como `ReplaceFace`); cilíndricas
  (agujeros, ejes) → defeaturing y rehacer la herramienta movida.
- Desfasar cara curva: `BRepOffset_MakeOffset` con desfase solo en las caras elegidas.
- Reemplazar cara por una superficie: `ReplaceFace.target` acepta una pieza superficie.
- Vaciado con espesores distintos: `Shell.overrides: Vec<(FaceRef, f64)>` →
  `BRepOffset_MakeOffset::SetOffsetOnFace`.
- Redondeo variable por puntos: `Fillet.points: Vec<(f64 parámetro, f64 radio)>` →
  `MakeFillet::Add` con ley.
- Redondeo completo: radio = mitad del ancho de la cara central entre dos laterales paralelas;
  general = `ChFi3d` (riesgo).
- Redondeo de cara: entre dos grupos de caras (`BRepFilletAPI` no lo trae: barrido de una bola
  entre caras o fillet en las aristas de intersección; investigar).
- Chaflán de vértice: corte por el plano de tres puntos a distancia sobre cada arista.

### F7 — Agujero y envolver

- `HoleStyle::Tapered { angle }` (cónico, p. ej. NPT 1°47').
- Tabla de pulgadas UNC/UNF (#2–1/2") junto a la ISO; elegir sistema en el diálogo.
- Grabado y relieve: `FeatureKind::Wrap { sketch: FeatureId (SurfaceSketch), depth, emboss }`:
  partir la cara con las curvas envueltas cerradas (`Splitter`), tomar los parches y darles
  espesor sobre la normal (`MakeThickSolid`), sumar o restar.

### F8 — Referencias y curvas

- **Primero (usuario, 2026-10-10: «los planos de referencia se construyen de una forma
  ridícula, con datos, no clics ni referencias a los objetos, líneas ni nada»).** Hoy el plano,
  el eje y el punto de referencia se arman escribiendo números en el diálogo. Tienen que
  armarse como en Onshape: clic en lo que lo define y el tipo se deduce de lo elegido, ligado a
  esas referencias (siguen al modelo). Plano: una cara plana o un plano (desfasado con flecha
  arrastrable y valor), dos caras o planos (plano medio), cara + arista (en ángulo alrededor de
  la arista), tres puntos o vértices, una arista + un punto, una línea de sketch + un punto,
  normal a una arista o curva en un punto, tangente a una cara curva por un punto. Eje: una
  arista recta o línea de sketch, el eje de una cara cilíndrica o cónica, dos puntos, la
  intersección de dos planos. Punto: vértice, medio de arista, centro de círculo o esfera, punto
  de sketch, intersección arista-cara. Vista previa en vivo mientras se elige y la lista de
  referencias en el diálogo (cada una se quita o se cambia), como las demás operaciones con
  cajas de selección. Los datos escritos quedan solo para el desfase y el ángulo.

- `PlaneDef::Tangent { face, through: PointSpec | angle }`,
  `PlaneDef::NormalToCurve { edge, at }` (tangente de `BRepAdaptor_Curve::D1`).
- `FeatureKind::Frame { origin: PointSpec, z: AxisSpec, x: AxisSpec }`: sistema de coordenadas
  usable como sus tres planos y tres ejes (`PlaneSpec::Frame { feature, which }`) y en Alinear.
- Espiral: `Helix` gana `radius2` (cónica) y `flat` (espiral plana de Arquímedes).
- Curvas como operación (`RefGeom::Curves`, usables como camino):
  `CompositeCurve { edges, curves }`, `ProjectedCurve { sketch, face, dir }`
  (`BRepProj_Projection`), `IntersectionCurve { a, b }` (`BRepAlgoAPI_Section`),
  `PointsCurve { points: Vec<PointSpec> }` (`GeomAPI_Interpolate`).
- Línea de partición: `SplitFaces { faces, tool: Curve | Plane | Silhouette { dir } }`
  (`BRepFeat_SplitShape`; silueta = donde la normal es perpendicular a la dirección de
  desmolde).

### F9 — Superficies restantes

- Plana: `SurfacePlanar { sketch, regions }` (`BRepBuilderAPI_MakeFace` del contorno).
- Desfasada: `SurfaceOffset { parts, distance }` (`BRepOffsetAPI_MakeOffsetShape`).
- Extender: `SurfaceExtend { edges, distance, natural }` (`GeomLib::ExtendSurfByLength`).

### F10 — Manipuladores en más operaciones

- `HandleKind` gana `Angle` (arco con punta arrastrable) y `Radius`.
- Revolución (ángulo), redondeo (radio), chaflán (distancia), vaciado y engrosar (espesor),
  patrones (separación y ángulo), mover cara y transformar (F1).

## Errores del sketch a arreglar

- ~~Cota de ángulo entre dos líneas~~ (usuario, 2026-10-10: «es terrible»): arreglado el
  2026-10-10, toma lo que mide y dibuja su arco; además hay botón ∠ en la barra de arriba.

## Pendientes heredados de planes borrados

- Escaneo → CAD: pasó a [PLAN_ESCANEO.md](PLAN_ESCANEO.md) (2026-10-10).
- Visor: canvas único (que el visor principal dibuje las capas del CAD y CadView use ese
  canvas). La cámara común ya está (`lib/cameraRig.ts`).
- Chapa: más allá de pestañas, alivios y esquinas cerradas (dobladillos, pestañas en arista
  curva, desdoblar/doblar).
- Suite e2e completa, push y release CAD Windows/macOS cuando el usuario lo pida.

## Por probar (lo hecho en la nube sin compilar Rust)

Desde el 2026-10-11 el CAD se trabaja también en la nube, donde `cargo` no baja los crates
(`static.crates.io` bloqueado): el Rust se revisa solo por sintaxis (`rustfmt`), el TypeScript con
`tsc` y el C++ con `g++ -fsyntax-only` contra OCCT 7.6. Al correrlo en la PC, en este orden:

1. `cargo test -p cad-model` completo (que compile; se tocaron los literales de varias pruebas
   viejas por los campos nuevos) y en especial `--test operations --test hole`.
2. `cargo run -p pinocchio-app --example cad_http`, `npx vite` y
   `node apps/web/e2e/cad.mjs operaciones` (los escenarios «operaciones:»).
3. Si algo falla, anotar acá el escenario, el mensaje y cuál de las hipótesis de abajo cayó.

Hipótesis y puntos dudosos (si una prueba falla, mirar esto primero):

- **`chooseOption` (e2e)**: elige en los desplegables de Kobalte con clic en el botón de la fila
  y clic en la opción (`[role=option]`). No se probó nunca: si falla, es el arnés, no la operación.
- **`setInputIn` (e2e)**: el punto de «Hasta un punto» se escribe en
  `[data-point-field="…"] input[type=number]` (el `Select` de Kobalte tiene un input oculto; por
  eso el filtro por tipo).
- **Piel desfasada de un sólido** (`planar_and_offset_surfaces`): el área esperada supone que
  `BRepOffset_MakeOffset` con `GeomAbs_Arc` redondea aristas (cuartos de cilindro) y esquinas
  (octantes de esfera). Si da distinto pero la forma se ve bien, corregir la cuenta.
- **Revolución delgada** (Pappus): supone el contorno corrido con esquinas de afuera redondeadas
  y de adentro vivas (lo que hace `offset_face` con `GeomAbs_Arc`), como la extrusión delgada.
- **Hélice cónica y espiral**: spline por 32 puntos por vuelta; las pruebas toleran 0,05 mm en
  el radio. Si se ve facetada en un barrido, subir los puntos.
- **Patrón por puntos**: el original se toma en el punto más cercano al centro de la caja de lo
  que se repite (no se pide a mano). Con la operación lejos de todos los puntos, igual copia desde
  el más cercano: confirmar que es lo esperado.
- **Tablas UNC/UNF** (`standard::INCH` y `INCH_HOLES` en `lib/cad.ts`): brocas, pasante
  (ajuste normal), caja y avellanado a 82° escritos de tablas estándar; cotejar antes de usarlas
  en piezas reales.
- **OCCT 7.6 de la nube**: `g++` da 3 errores que ya estaban (`WriteStream`, `std::max` con
  lista) porque el proyecto usa un OCCT más nuevo; no son de estos cambios.
- **Revolución a dos ángulos**: no se migró `angle`: en la simétrica es el total; en dos ángulos,
  el del primer lado (`second` el del otro).

## Riesgos

- `ThruSections` no da tangencia en extremos ni cierre suave; puede requerir `GeomFill`.
- `BRepAlgoAPI_Defeaturing` falla con caras que tocan muchas vecinas; avisar y no romper.
- Redondeo de cara y completo general no tienen API directa en OCCT.
- Copiar y pegar operaciones con referencias por geometría: al pegar en otro lugar pueden
  apuntar a caras distintas; mostrar como referencia perdida en vez de adivinar.
