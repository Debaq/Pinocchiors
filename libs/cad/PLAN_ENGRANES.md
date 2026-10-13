# Engranes

Plan desde el 2026-10-10. Pedido del usuario: «nos falta una forma de hacer engranes… que se
complementen, que tengan relaciones, rectos, helicoidales y otros».

> **No borrar en limpiezas de planes** (pedido del usuario, 2026-10-10). Aunque se pida
> «eliminar cualquier plan previo», este se conserva salvo que el usuario lo nombre.

Estado (2026-10-13): **F1–F8 hechas**; queda «Más adelante» y lo pendiente anotado en cada fase.

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta · (F*n*) fase que lo cubre.

## ¿Sketch u otra cosa? Cómo lo hacen otros

| Programa | Cómo se hace el engrane | Cómo se relacionan |
|---|---|---|
| Onshape | Operación propia (FeatureScript «Spur Gear»): parámetros → pieza | Relaciones de ensamble *Gear*, *Rack and pinion*, *Screw* entre dos uniones |
| SolidWorks | Biblioteca (Toolbox), pieza configurable | *Mates* mecánicos: engrane, piñón-cremallera, tornillo, leva |
| Inventor | Generador de diseño: se diseña **el par** (relación, distancia entre centros, corrección) y crea las dos piezas ya ubicadas | Restricción de movimiento entre las dos |
| Fusion | Script que dibuja el perfil en un sketch y lo extruye | *Motion link* entre dos uniones |
| FreeCAD | Operación «engrane involuta» (perfil 2D paramétrico) + banco FCGear (recto, helicoidal, espina de pescado, interno, cremallera, cónico, sin fin, cicloidal) | No tiene (se anima a mano) |

Lección: **nadie lo dibuja en el sketch a mano**. El perfil de evolvente sale de fórmulas, no de
restricciones (un diente son decenas de puntos de una curva exacta; el solver no aporta nada y
se volvería lento con 40 dientes). Lo que funciona mejor es la combinación de Inventor + Onshape:

1. **Operación Engrane** paramétrica (como el tornillo: una pieza estándar con medidas).
2. **Engranar con…**: el segundo engrane se coordina con el primero (misma idea que las roscas
   coordinadas `ThreadLink`: «no es una booleana, es una coordinación»): toma módulo y ángulo de
   presión, se pone a la distancia entre centros y gira medio diente para que calce.
3. **Relaciones de ensamble** que acoplan giros: girar uno gira el otro según la relación.
4. El sketch queda para lo secundario: **insertar el perfil** como bloque (corte láser, perfiles
   raros) y la hoja del plano muestra la **tabla de datos del engrane**.

## Dónde estamos

- ✅ Piezas estándar (`cad_model::standard`, `PrimitiveShape::Bolt/Nut/Washer`) con diálogo,
  colocación sobre un agujero (`cad_edge_seat`) y caras nombradas por índice.
- ✅ Coordinación entre operaciones: `ThreadLink` + `Evaluation.threads` (eje, fase `phase_x`).
- ✅ Ensamble (`cad_model::assembly`): conectores, revoluta/deslizante/cilíndrica con ángulo y
  distancia impuestos, LM con jacobiano numérico, choques.
- ✅ OCCT: `Curve::BSpline` (perfil exacto), `face`, `prism`, `loft`, `MakePipeShell` con
  binormal fija (rosca), hélices, booleanas, STEP.
- ✅ F1 (2026-10-12): `cad_model::gears` — evolvente exacta, pie por trocoide de la herramienta, avisos, datos del par (`pair`), cremallera (`rack_outline`), helicoidal transversal. 15 pruebas en `model/tests/gears.rs`.
- ✅ F2 (2026-10-12): `PrimitiveShape::Gear(GearPart)` (`cad_model::gear_part`): exterior, corona y cremallera; agujero, cubo, chavetero DIN 6885, aligeramientos en círculo, holgura de impresión. Botón «Engrane» en Sólidos, campos en el cuadro de propiedades con Ø y avisos (`cad_gear_info`). Pruebas en `model/tests/gear_part.rs`. Falta: rayos (aligeramiento), nombres de cara («diente», «flanco»), e2e.
- ✅ F3 (2026-10-12): `GearPart.link: GearLink { feature, angle, offset, distance }` + `Evaluation.gears` (`GearAxis`). El engrane copia módulo y ángulo del otro, se ubica a la distancia entre centros (`gear_part::mesh`) y gira para calzar; con `distance` pedida ajusta su corrección de perfil. Exterior-exterior, piñón-corona (en ambos sentidos) y piñón→cremallera; avisos de interferencia y razón de contacto < 1,1. Pruebas con choque de volúmenes en `model/tests/gear_mesh.rs`. Cuadro «Engranar con» en las propiedades. Falta: diseñar por relación (calcular dientes), clic en el visor para la dirección, cremallera como fuente, ejes no paralelos (aviso), hélice contraria sin probar (F4).
- ✅ F4 (2026-10-12): helicoidales (`helix_angle` ±, módulo normal; perfil transversal) y doble helicoidal con canal central. Construcción por `Shape::loft` de secciones giradas cada ~4° (no hizo falta C++ de torsión: `sweep_twisted` daba 9 s con 60 dientes; el loft ~1,8 s). `Shape::bounds` (C++ `cad_bounds`) reemplaza a `mass()` en `touching`/`diag`: la integral de volumen sobre superficies libres tardaba segundos en cada operación. `gears::pair` exige manos contrarias (exterior-exterior) o iguales (corona); `mesh` invierte la mano sola. Pruebas: choque de volúmenes con helicoidal y espina de pescado. Falta: módulo transversal elegible, cremallera helicoidal, helicoidales cruzados.
- ✅ F5 (2026-10-12): `Assembly.relations: Vec<Relation>` (`Gear`, `RackPinion`, `Screw`) sobre uniones existentes. La unión con ángulo/distancia impuestos manda y la otra sale de ahí (`propagate`, sin incógnitas nuevas, multivuelta sin saltos); sin ninguna impuesta, el solver agrega la ecuación con el ángulo medido (`wrap`). `suggest_gear_relations` crea las relaciones de los `GearLink` con la razón real ±z₁/z₂ (signo por la geometría de las uniones) y la fase de la pose actual; `interferences_for` ignora el polvo de contacto entre engranes que engranan. Panel «Acoplamientos» con «Detectar engranes» y comando `cad_assembly_suggest`. Pruebas en `model/tests/assembly_relations.rs` (giran 0°/17°/95° sin chocar; el control con fase corrida sí choca). Falta: arrastrar un engrane en el visor (hoy: casilla Ángulo de la unión), relaciones a mano (solo se detectan), tren con más de dos y 180° exactos (el solver no sale de ese punto muerto).
- ✅ F6 (2026-10-12): `GearKind::Bevel` (cónico recto, Tredgold): perfil recto virtual de z/cos δ dientes en el cono de atrás llevado al plano del extremo grande por rectas que pasan por el vértice (`GearSpec::bevel_map`); sólido = tronco de cono por el vértice (`loft` ruled de dos secciones planas, sin booleanas). Los extremos quedan planos (perpendiculares al eje) y no cónicos: la versión con cono de atrás/adelante por booleanas fallaba (resultado vacío) según el ángulo. `GearSpec.cone_angle` (δ) y `shaft_angle` (Σ, en el segundo del par). `mesh_bevel`: vértices coincidentes, eje del segundo a Σ, cono del segundo = Σ − δ₁, fase por la fórmula del recto con los radios primitivos; avisa si δ₁ no es atan(senΣ/(z₂/z₁+cosΣ)). Con agujero, cubo y chavetero. Pruebas: choque nulo 20/20, 16/32 y 18/27 a 75°, control con medio paso choca. Panel con «Cónico recto». Falta: espiral e hipoides, cálculo automático del cono del primero, aligeramientos.
- ✅ F7 (2026-10-13): `GearKind::Worm` (`teeth` = entradas, `module` axial, `worm_diameter` primitivo —0: 10·m—, `left_hand`): filete trapecial de flancos 2α de `thread_impl` (C++ generalizado: `cad_make_thread_trapezoid`, varias entradas = la misma hélice corrida un paso entre filetes; `cad_make_thread` pasó a ser el caso ISO de 1 entrada). Rueda = engrane exterior helicoidal con hélice = ángulo de avance γ (misma mano), módulo normal m_x·cosγ, α_n = atan(tanα_x·cosγ) y garganta (`GearPart.throat` = distancia entre centros: dientes más altos hacia las caras y corte por un toro de radio a − r_cabeza, centrado en la cara media). `mesh_worm`: ejes cruzados a 90°, rueda a d₁/2 + r₂ en la dirección u, eje z_b = z_sinfin × u, fase por la posición axial del filete en esa dirección. Relación del ensamble: ∓entradas/dientes según la mano (verificada con choque y control de signo invertido para las dos manos). Pruebas: volumen del sin fin contra Pappus, 1 y 2 entradas, derecho e izquierdo, choque nulo y control. Falta: rueda como fuente (hoy el sin fin va primero), tallado exacto de la rueda por simulación del sin fin, autobloqueo (aviso) y rendimiento, globoide/doble envolvente, hoja (F8).
- ✅ F8 (2026-10-13): **Sketch:** `gear_part::sketch_block` (comando `cad_gear_block`) deja el contorno como `SketchBlock` rígido (origen = centro; arcos de pie/cabeza con centro en el origen y splines de flancos con manijas de tangente exacta; cremallera: líneas), que se inserta con las copias de bloque ya existentes (`insertBlock`, rígidas con su ancla, rápidas con cientos de entidades). «Traer» en Organización → Bloques elige un engrane del diseño y activa «Insertar»; si el bloque ya existe se actualiza (y sus copias). **Hoja:** `DrawingSheet.gears` (ids de engranes elegidos en el panel de la hoja) → círculo primitivo en trazo y punto con cruz en las vistas que miran a lo largo del eje (`gearMarks`) y tabla de datos apilada sobre el cajetín (`gearTable`/`gearTableBoxes`; SVG y DXF). `GearInfo` trae cota sobre k dientes y sobre bolas (`gear_part::measurements`: W = m·cosα[π(k−½)+z·invα_t]+2xm·senα, M por invα_M; engranes exteriores, helicoidales en el plano transversal; coincide con la tabla W₃=7,6604 para m=1, z=20). Pruebas: `model/tests/gear_part.rs` (bloque: 7·z entidades, una sola región con el área del engrane; cotas) y `e2e/gearSheet.test.mjs` (node, sin navegador). Falta: cota sobre bolas de la corona (entre bolas), valores con juego/helicoidal ISO exactos, rotular el círculo base/de pie en las vistas, y perfil de la rueda de sin fin.

## Cobertura

| Qué | Estado |
|---|---|
| Recto exterior (evolvente, módulo o paso diametral, ángulo de presión 14,5/20/25°) | ✅ F1+F2 (módulo; paso diametral no) |
| Corrección de perfil (desplazamiento x), juego entre dientes, socavado del pie (trocoide) | ✅ F1 |
| Cubo, agujero del eje, chavetero, prisionero, aligeramientos | 🟡 F2 (hecho: agujero, cubo, chavetero, agujeros; faltan prisionero y rayos) |
| Interno (corona de anillo) | ✅ F2 |
| Cremallera | ✅ F2 |
| Par coordinado: distancia entre centros, fase, relación, avisos (interferencia, módulo distinto) | ✅ F3 |
| Helicoidal (mano, ángulo de hélice, módulo normal/transversal) | ✅ F4 (módulo normal; transversal no) |
| Doble helicoidal / espina de pescado | ✅ F4 |
| Relación de engrane (giro ↔ giro) en el ensamble | ✅ F5 |
| Piñón-cremallera (giro ↔ deslizamiento) y tornillo (giro ↔ avance) | ✅ F5 |
| Animar el mecanismo arrastrando un engrane | ✅ F5 (arrastrar en el visor; sin probar en la app real) |
| Cónico recto (eje a 90° u otro ángulo) | ✅ F6 |
| Tornillo sin fin + corona | ✅ F7 |
| Perfil del engrane en el sketch (bloque), tabla de datos en la hoja | ✅ F8 |
| Holgura para impresión 3D (juego extra por boquilla) | ✅ F2 |
| Tren planetario (sol, satélites, corona) de una vez | ❌ (más adelante) |
| Cicloidal, poleas dentadas (GT2/HTD), piñones de cadena, no circulares | ❌ (más adelante) |
| Cálculo de resistencia (Lewis / AGMA / ISO 6336) | ❌ (más adelante) |

## Fases

Cada fase como en PLAN_OPERACIONES: modelo en `cad-model`, C++ solo si hace falta, comando en
`apps/desktop/src/cad.rs`, diálogo en `DesignStep.tsx`, pruebas en `model/tests` y e2e solo de lo
nuevo. Campos nuevos con `#[serde(default)]`.

### F1 — Geometría de la evolvente (Rust puro, sin OCCT)

- Módulo nuevo `cad_model::gears` (sin dependencias de OCCT, probado con números):
  `GearSpec { module, teeth, pressure_angle, helix_angle, shift, backlash, addendum, dedendum,
  root_fillet }` y `GearKind::{External, Internal, Rack}`.
- Círculos: primitivo, base, cabeza, pie; espesor del diente en el primitivo
  (`πm/2 + 2xm·tanα − juego/2`); evolvente por puntos y ajustada a B-spline (error < 1 µm);
  pie como **trocoide de la herramienta** (cremallera generadora), no como arco inventado: así sale
  el socavado correcto con pocos dientes.
- Avisos: socavado (z < 17 a 20° sin corrección), diente en punta (cabeza más ancha que el
  diente), cabeza que no alcanza el círculo base.
- Datos de un par: distancia entre centros (con corrección: `inv α_w`), razón de contacto, juego.
- Salida: perfil 2D cerrado como `Vec<Curve>` (un diente + patrón circular en el plano, o el
  contorno completo).
- Pruebas: paso angular exacto, espesor en el primitivo, simetría del diente, z=8 con socavado,
  par z=12/z=40 sin interferencia y con razón de contacto > 1,2.

### F2 — Operación Engrane (recto, interno, cremallera)

- `PrimitiveShape::Gear { spec, width, bore, hub, keyway, … }` como el tornillo (marco, diálogo,
  caras por índice + nombres «diente», «flanco», «agujero»). Interno = corona con diámetro exterior;
  cremallera = largo y alto.
- Construcción: cara del perfil → `prism`. El perfil completo como una sola cara con N dientes
  (más rápido que unir N dientes).
- Agujero del eje, cubo (diámetro/largo), chavetero DIN 6885 por diámetro, aligeramientos
  (agujeros en círculo o rayos).
- Juego para impresión 3D: opción «Holgura de impresión» (mm) que suma juego y achica la cabeza.
- Diálogo: módulo, dientes, ancho, ángulo de presión, corrección; muestra Ø primitivo, Ø exterior
  y avisos de F1. Botón en la barra junto a Tornillo/Tuerca.
- Masa y STEP salen solos (ya son pieza).

### F3 — Engranes que se complementan (par coordinado)

- `GearLink { feature, flip }` en el engrane, como `ThreadLink`: «Engranar con…» otro engrane
  anterior del historial.
- Al enlazar: copia módulo y ángulo de presión (y hélice con la mano contraria), se ubica a la
  distancia entre centros (eje paralelo, dirección elegida con un ángulo alrededor del otro o con
  clic), y gira el medio diente que hace falta para que los dientes calcen (`phase`).
- Diseñar por relación: «relación 1:3» o «distancia entre centros 60 mm» → calcula dientes y, si
  no da exacto, la corrección de perfil que la cumple.
- `Evaluation.gears` (como `threads`): eje, círculo primitivo, fase y enlaces, para el visor, el
  ensamble y la hoja. Avisos en el árbol: módulos distintos, interferencia, ejes no paralelos.
- Interno + piñón y cremallera + piñón también se enlazan.

### F4 — Helicoidales

- `helix_angle` y `hand` en `GearSpec`; módulo normal o transversal elegible.
- Construcción: el perfil transversal girando a lo largo del ancho. C++ nuevo
  `cad_twist_prism(face, axis, height, twist)` con `MakePipeShell` + hélice auxiliar (o
  `loft` de secciones giradas si da sólidos inválidos; comparar tiempos y validez).
- Doble helicoidal (espina de pescado): dos mitades de mano opuesta, con o sin canal central.
- El par de F3 exige manos opuestas (ejes paralelos); con la misma mano avisa «ejes cruzados»
  (helicoidales cruzados quedan para más adelante).

### F5 — Relaciones mecánicas en el ensamble

- `Assembly.relations: Vec<Relation>` con `Relation::{Gear { a, b, ratio, reverse },
  RackPinion { revolute, slider, distance_per_turn }, Screw { cylindrical, pitch }}` sobre
  uniones (`Mate`) ya existentes.
- Solver: ecuación extra que acopla el ángulo de una revoluta con el de otra (o con la distancia
  de una deslizante). Hoy el ángulo es un dato fijo; pasa a poder ser incógnita acoplada.
- Al meter al ensamble dos engranes enlazados en F3, la relación se crea sola con su razón real
  (z₂/z₁).
- Arrastrar un engrane en el visor gira todo el tren; control «Mover» con ángulo para animar.
  Los choques entre engranes que engranan se ignoran si el juego es ≥ 0 (contacto, no choque).

### F6 — Cónicos

- Cónico recto con ángulo entre ejes (90° por defecto): perfil por **aproximación de Tredgold**
  (evolvente sobre el cono complementario) en la cara de atrás, y `loft` hacia el vértice hasta el
  ancho del diente.
- Par cónico coordinado como en F3 (ángulos de cono de cada uno a partir de z₁, z₂ y el ángulo
  entre ejes).
- Espiral y hipoides quedan para más adelante.

### F7 — Tornillo sin fin y corona

- Sin fin: reusar la máquina de roscas (`thread` con perfil de cremallera: flancos de 2α en vez
  de 60°, varias entradas).
- Corona: helicoidal con el ángulo de avance del sin fin y la garganta cóncava (corte por toro).
  Primero corona helicoidal simple (sirve para impresión); el tallado exacto por simulación del
  sin fin queda para más adelante.
- Relación sin fin ↔ corona (giro ↔ giro con ejes cruzados) en F5.

### F8 — Sketch y hoja

- En el sketch: «Insertar perfil de engrane» deja el contorno como bloque (curvas B-spline del
  sketch) para extruir a mano, cortar con láser o exportar DXF. No se ata al solver (bloque rígido).
- En la hoja: tabla de datos del engrane (m, z, α, β, x, Ø primitivo, cota sobre dientes / sobre
  bolas) al elegir una vista de un engrane; círculo primitivo en línea de trazo y punto.

## Pendientes (anotados el 2026-10-13, al cerrar F8)

Lo que quedó fuera de cada fase, por área. Nada de esto impide usar lo hecho.

**Pruebas y verificación**
- Nada de F2–F8 se probó en la app real ni con e2e en el navegador (solo `cargo test` y `node --test`). Correr e2e de lo nuevo cuando se pida: botón «Engrane», panel de propiedades (tipos, «Engranar con», cónico, sin fin), «Detectar engranes» en el ensamble, «Traer» perfil en el sketch y engranes en la hoja.
- Las pruebas de choque de `gear_mesh`/`gear_part` tardan minutos (booleanas pesadas de corona y helicoidales): acortarlas o marcarlas lentas.

**Geometría y piezas (F1–F2)**
- Paso diametral (solo hay módulo).
- Aligeramientos con rayos (hoy solo agujeros en círculo) y prisionero junto al chavetero.
- Nombres de cara («diente», «flanco», «agujero»): hoy las caras van por índice.
- Módulo transversal elegible (hoy el módulo es siempre el normal).

**Pares y enlaces (F3)**
- (Diseñar por relación: hecho 2026-10-13, `gear_part::design_teeth` + `cad_gear_design`; campo «Relación» y botón de distancia en «Engranar con». Sin probar en la app real.)
- Elegir la dirección del centro con un clic en el visor (hoy es un ángulo).
- Cremallera como fuente (hoy el piñón va primero).
- Avisar de ejes no paralelos y de helicoidales cruzados.

**Helicoidales (F4)**
- (Cremallera helicoidal: hecha 2026-10-13, sección transversal corrida por z·tanβ; probada con choque en las dos manos.)
- Espina de pescado: canal central solo en exteriores.

**Ensamble (F5)**
- (Hechos 2026-10-13, sin probar en la app real: arrastrar una pieza en el visor del ensamble gira la revoluta/cilíndrica que la sostiene —`lib/assemblyDrag.ts`, un solo paso de deshacer por gesto— y «Agregar acoplamiento» crea relaciones a mano. Falta: elegir la unión cuando una pieza tiene varias, y arrastrar uniones deslizantes.)
- Trenes de más de dos engranes y planetarios sin probar.
- El solver no sale del punto muerto de 180° exactos desde cero.
- La detección toma la primera revoluta/cilíndrica de cada engrane.

**Cónicos (F6)**
- Extremos planos en vez de cónicos (la versión por booleanas con los conos fallaba con δ ≥ 55°): revisar si se puede volver a conos de atrás/adelante.
- Calcular solo el cono del primer cónico según los dientes y Σ (hoy solo avisa).
- Aligeramientos en cónicos; ancho de cara óptimo (hoy solo se valida < 0,6 de la generatriz).
- Espirales e hipoides.

**Sin fin y rueda (F7)**
- La rueda no puede ser la fuente (el sin fin va primero).
- Tallado exacto de la rueda por simulación del sin fin (hoy: helicoidal con garganta).
- Sin fin globoide (doble envolvente). (Autobloqueo y rendimiento: hecho, aviso en el árbol con μ = 0,1.)

**Sketch y hoja (F8)**
- (Hechos 2026-10-13: cota entre bolas de la corona —`measurement_between_pins`, verificada solo contra el cálculo a mano—, círculos da/df/db rotulados y aviso de engranes borrados.) Falta: cotas ISO exactas con juego y helicoidales (hoy: nominales, bola de 1,7·m).
- Bloque de sketch de la rueda de sin fin (hoy sale como helicoidal normal) y del sin fin (sección axial).

## Más adelante

Tren planetario completo (sol + satélites + corona con la condición de montaje), cicloidales,
poleas dentadas y piñones de cadena, engranes no circulares, helicoidales cruzados, cónicos
espirales, resistencia (Lewis/AGMA), corona tallada por simulación.

## Riesgos

- Helicoidal con `MakePipeShell` y una cara de muchos dientes puede dar sólidos inválidos o lentos:
  probar temprano (F4) con z=60 y alternativa por `loft`.
- Muchos dientes = muchas caras: nombrar caras por índice como el tornillo (medir cada cara tarda).
- La teselación del visor con evolventes finas: revisar que el tamaño de triángulos no explote.
- El solver de ensamble usa jacobiano numérico; las relaciones acopladas pueden necesitar mejores
  valores iniciales (partir de la fase calculada en F3).
