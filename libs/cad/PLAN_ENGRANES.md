# Engranes

Plan desde el 2026-10-10. Pedido del usuario: «nos falta una forma de hacer engranes… que se
complementen, que tengan relaciones, rectos, helicoidales y otros».

> **No borrar en limpiezas de planes** (pedido del usuario, 2026-10-10). Aunque se pida
> «eliminar cualquier plan previo», este se conserva salvo que el usuario lo nombre.

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
- ❌ Todo lo de engranes: geometría, operación, pares, relaciones, tipos.

## Cobertura

| Qué | Estado |
|---|---|
| Recto exterior (evolvente, módulo o paso diametral, ángulo de presión 14,5/20/25°) | ❌ (F1, F2) |
| Corrección de perfil (desplazamiento x), juego entre dientes, socavado del pie (trocoide) | ❌ (F1) |
| Cubo, agujero del eje, chavetero, prisionero, aligeramientos | ❌ (F2) |
| Interno (corona de anillo) | ❌ (F2) |
| Cremallera | ❌ (F2) |
| Par coordinado: distancia entre centros, fase, relación, avisos (interferencia, módulo distinto) | ❌ (F3) |
| Helicoidal (mano, ángulo de hélice, módulo normal/transversal) | ❌ (F4) |
| Doble helicoidal / espina de pescado | ❌ (F4) |
| Relación de engrane (giro ↔ giro) en el ensamble | ❌ (F5) |
| Piñón-cremallera (giro ↔ deslizamiento) y tornillo (giro ↔ avance) | ❌ (F5) |
| Animar el mecanismo arrastrando un engrane | ❌ (F5) |
| Cónico recto (eje a 90° u otro ángulo) | ❌ (F6) |
| Tornillo sin fin + corona | ❌ (F7) |
| Perfil del engrane en el sketch (bloque), tabla de datos en la hoja | ❌ (F8) |
| Holgura para impresión 3D (juego extra por boquilla) | ❌ (F2, opción) |
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
