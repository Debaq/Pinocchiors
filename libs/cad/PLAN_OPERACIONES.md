# Operaciones: igualar la lista completa de un CAD

Plan único del CAD paramétrico desde el 2026-10-09. Reemplaza a todos los planes anteriores
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
| Hasta vértice | ❌ (F3) |
| Hasta superficie | 🟡 solo hasta el plano de la cara (F3) |
| Hasta superficie con desfase | ❌ (F3) |
| Desde plano desfasado | 🟡 el desfase es del sketch, no de la extrusión (F3) |
| Ángulo de desmoldeo, pared delgada, invertir, selección de regiones | ✅ |
| Dirección personalizada | ❌ (F3) |

### Opciones de revolución
| Función | Estado |
|---|---|
| Completa, por ángulo, selección de eje | ✅ |
| Simétrica, dos ángulos, hasta superficie, pared delgada | ❌ (F3) |

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
| Cónico | ❌ (F7) |
| Tabla de estándares | 🟡 métrico ISO; faltan pulgadas UNC/UNF (F7) |

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
| Lineal en dos direcciones | ❌ en operaciones (el sketch sí) (F2) |
| Guiado por puntos de croquis | ❌ (F2) |
| Omitir instancias | ❌ (F2) |
| Simetría de operaciones | ✅ |
| Simetría de cuerpos | 🟡 refleja el cuerpo entero pero siempre une (F1) |

### Operaciones entre cuerpos
Unión, resta, intersección, dividir, separar no conectados, eliminar: ✅ todas.

### Transformaciones
| Función | Estado |
|---|---|
| Mover, rotar, copiar, alinear a cara o plano | ❌ (solo en ensamble y al ubicar primitivas) (F1) |
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
| Espiral | ❌ (F8) |
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
| Plana | 🟡 solo como relleno de un borde plano (F9) |
| Desfasada, extender | ❌ (F9) |

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

## Pendientes heredados de planes borrados

- Escaneo → CAD: unir y separar planos grandes en `detect_all`; alinear el escaneo a ejes y
  proponer cortes; reconocer conos, esferas, toros y redondeos.
- Visor: canvas único (que el visor principal dibuje las capas del CAD y CadView use ese
  canvas). La cámara común ya está (`lib/cameraRig.ts`).
- Chapa: más allá de pestañas, alivios y esquinas cerradas (dobladillos, pestañas en arista
  curva, desdoblar/doblar).
- Suite e2e completa, push y release CAD Windows/macOS cuando el usuario lo pida.

## Riesgos

- `ThruSections` no da tangencia en extremos ni cierre suave; puede requerir `GeomFill`.
- `BRepAlgoAPI_Defeaturing` falla con caras que tocan muchas vecinas; avisar y no romper.
- Redondeo de cara y completo general no tienen API directa en OCCT.
- Copiar y pegar operaciones con referencias por geometría: al pegar en otro lugar pueden
  apuntar a caras distintas; mostrar como referencia perdida en vez de adivinar.
