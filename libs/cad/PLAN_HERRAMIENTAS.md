# Plan: herramientas de Diseñar unificadas y trabajo sobre la piel

Plan desde el 2026-11-07. Pedido del usuario: «pegar objetos o amoldar sobre caras irregulares,
en especial de STL», y después de revisar las herramientas: «hazlo todo».

Dos ideas guían todo:

1. **Una herramienta por idea.** Muchas herramientas nacieron como botones aparte siendo la
   misma operación con otro comportamiento. Se juntan en una con un selector; los botones que
   sirvan de atajo pueden quedar, pero abren la misma herramienta y su diálogo deja cambiar el
   tipo después. Los proyectos guardados siguen abriendo igual (los tipos internos se
   conservan o se convierten al leer).
2. **La piel es un destino como cualquier otro.** Lo que hoy se hace sobre una cara CAD
   (relieve, curva proyectada, referencias, mover cara hasta…) tiene que poder hacerse sobre un
   STL o escaneo, y al revés, con la misma herramienta.

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta.

## Fases

### U0 — Detalles de la revisión de Sólidos ✅
Tangencia de la transición desactivada si las secciones tienen distinta cantidad de lados,
esfera sin «Eje», textos de la caja traducidos, un solo diálogo de STEP a la vez. Desmolde con
agujeros y pared delgada (f17e309).

### U1 — Relieve sobre la piel (proyectar) ✅
Una operación `Relief`: un sketch (texto, logo, cualquier región) llevado a una superficie con
altura. Reemplaza a «Grabado», «Relieve» y al «Parche desde sketch» usado para texto.

- Destino: la piel del escaneo/STL (y después caras de un sólido).
- Cómo llega el sketch: **proyectar** (en línea recta por la normal del sketch; las medidas
  valen en el plano). Envolver llega en U2.
- Altura, cuatro formas:
  1. **Pareja sobre la piel**: el mismo espesor perpendicular a la piel en todo punto (la cara
     de arriba copia la piel).
  2. **Tope plano**: base amoldada a la piel, cara de arriba en un plano (a una distancia del
     plano del sketch).
  3. **Tope plano automático**: el plano a la altura pedida sobre el punto más alto de la piel
     bajo el sketch (ninguna parte queda más baja).
  4. **Según la dirección**: la piel corrida la altura a lo largo de la normal del sketch
     (paredes paralelas, como proyectado con una linterna).
- Sentido: **afuera** (relieve) o **adentro** (grabado).
- Resultado: pieza nueva, unir o restar a piezas del diseño.
- Muestra las dos medidas del sketch: en el plano y sobre la piel.

### U2 — Envolver un sketch sobre la piel ✅
«Envolver» como segunda forma de llevar el sketch: las medidas valen sobre la piel (como una
calcomanía). El trozo de piel bajo el sketch se aplana con el desplegado de UV que ya tiene la
app (uv-core), el sketch se ubica en ese aplanado (ancla = clic en la piel + giro) y se lleva de
vuelta. Mapa de estiramiento a color y aviso si pasa de 2 %. Sirve para relieve (U1) y para
curvas (U5).

### U3 — Espesor sobre piel ✅
«Escaneo como cuerpo», «Carcasa», «Parche desde sketch» y «Pincel» son la misma operación
(`ScanBody`): un solo botón «Espesor sobre piel» con De qué / Dónde (todo, bajo un sketch,
pintado) / Hacia dónde; el diálogo muestra los toques del pincel y deja agregarlos o borrarlos.
«Engrosar» aparece una sola vez.

### U4 — Mover cara hasta… ✅
«Mover cara», «Reemplazar cara», «Cara al escaneo» y «Girar cara»: una herramienta con
«Movimiento: distancia / hasta un plano o cara / hasta el escaneo / girar». Se puede cambiar
entre ellos en el diálogo sin perder las caras elegidas.

### U5 — Llevar a la superficie (curvas) ✅
«Sketch envuelto», «Curva proyectada», «Sketch a la piel»: una herramienta con destino (caras
o escaneo) y forma (proyectar / envolver). «Curva por puntos» y «Camino por puntos»: una sola,
sobre vértices del sólido o sobre la piel.

### U6 — Referencias desde el escaneo en el menú ✅
«Plano», «Eje» y «Punto» del menú aceptan clics sobre el escaneo (zona, 3 puntos, cilindro,
punto) y dejan las referencias ligadas. El panel del escaneo deja de repetirlas.

### U7 — Selector de tipo en lo que ya es una sola operación ✅
- Patrón: el diálogo deja cambiar el tipo (lineal, circular, curva, cadena, espiral,
  superficie, tabla, relleno, puntos); un botón «Patrón» y los tipos como atajos.
- Primitiva: cambiar la forma en el diálogo (caja, cilindro, esfera, cono, toro).
- Pieza estándar: tornillo, tuerca, arandela intercambiables.
- Mover pieza: «Mover / Girar / Copiar cuerpo» en uno con «dejar copia».
- Hélice y espiral: un botón. Barrido y tubo: un botón.

### U8 — Partir ✅
«Cortar por plano», «Partir», «Separar», «Recortar con plano», «Recortar con sólido»: «Partir»
con «con: plano o pieza» y «quedarse con: un lado / los dos / lo de adentro», para sólidos y
superficies.

### U9 — Redondeo y chaflán ✅
«Redondeo», «Redondeo completo», «Redondeo de cara»: «Redondeo» con tipo (aristas, completo,
entre caras). «Chaflán» y «Chaflán de vértice»: «Chaflán» con tipo.

### U10 — Sólido o superficie ✅
Extrusión, revolución, transición y barrido con «Resultado: sólido / superficie»; el menú
Superficies queda con relleno, coser, extender, recortar y desfasar.

### U11 — Colocar sobre la piel ✅
Clic en un punto del STL o de una cara curva: un sistema de coordenadas ligado a la piel
(origen en el punto, Z por la normal). Primitivas, piezas estándar y piezas derivadas se
pueden colocar ahí, con «rellenar hasta la piel» (la base se amolda).

### U12 — Unir y grabar en el STL sin perder detalle ✅
Hoy juntar lo agregado con el STL pasa el STL entero por vóxeles (20 000 triángulos): se
redondean las aristas. Booleanas directas sobre la malla (unir, restar) para exportar el STL
con lo agregado o grabado conservando su detalle.

### U13 — Puntos, medidas y cotas sobre el STL ✅
Pedido del usuario (2026-11-08): «poner puntos en el STL, medir entre ellos, hacer cotas para
dimensionar correctamente el objeto, y que esos puntos sirvan de referencia para construir encima».

Hoy: Medir (tecla M, en los espacios de mallas) da una distancia de paso, sin guardarla; en Diseñar hay
puntos ligados al escaneo (`PointSpec::Scan`), sin cotas; las cotas existen solo en el plano 2D de
piezas CAD.

- **Puntos sobre la piel con nombre** (P1, P2…): clic en el STL; quedan ligados (siguen al escaneo si se
  alinea o se repara) y se ven en el visor con su nombre. Varios seguidos sin volver a abrir la
  herramienta.
- **Referencias para construir encima** (el motor ya lo admite vía `PointSpec::Reference`; falta que sea
  directo): plano por 3 puntos, eje por 2 puntos, sistema de coordenadas (origen + hacia dónde van Z y X),
  sketch anclado (su origen en un punto, su plano por 3), «Sobre la piel» en un punto ya puesto, patrón
  por puntos.
- **Cotas guardadas en el diseño**, dibujadas en el visor con flechas y número: distancia directa y por
  eje (X, Y, Z); distancia por la superficie (geodésica, como `ScanPath`); ángulo entre tres puntos;
  diámetro de una zona cilíndrica (ajuste de cilindro existente); espesor en un punto (de una cara a la
  otra, por rayo).
- **Dimensionar el objeto**: «esta cota tiene que medir 85 mm» escala el STL para que la cumpla (escaneos
  sin escala o dudosa); queda como paso del historial del objeto.
- **Cotas como parámetros**: una medida del STL alimenta un parámetro del diseño (el ancho de una férula =
  una distancia medida) y se actualiza si cambia el escaneo.
- **Lista de medidas** exportable (informe, comparar con el objeto real).

## Registro

2026-11-07/08 (sin probar todavía en la app real; pruebas Rust y de tipos):

- U1: `FeatureKind::Relief` (`eval.rs` `relief`): las cuatro formas hacia afuera y hacia adentro,
  sobre el escaneo (`target` auto/escaneo) o sobre el sólido del diseño (`target: body`, carcasa por
  `offset_shape` cosido en sólido). Medidas en el plano y sobre la piel en `FeatureStatus.note`.
  Pruebas `scan/tests/relief.rs` (esfera, valores analíticos) y `relieve_sobre_el_solido_del_diseno`.
  Reemplaza a «Grabado» y «Relieve» en el menú (la operación vieja `wrap` sigue abriendo).
- U2: `ReliefMap::Wrap { anchor, angle }`: el trozo de piel alrededor del ancla se aplana con
  `uv_core::flatten_patch` (LSCM + ARAP a escala real), el sketch se apoya ahí y vuelve al plano por
  la dirección de la proyección (`wrap_regions`). La app pone el escaneo tal cual (`Relief.skin`);
  sobre el sólido se usa su teselado soldado. Prueba `wrap_keeps_lengths_on_the_skin`.
- U3: un botón «Espesor sobre piel»; el diálogo tiene «Dónde: todo / bajo un sketch / pintado» con
  «Pintar más» y «Pintar de nuevo».
- U4: «Mover cara» con «Movimiento: distancia / hasta un plano o cara / hasta el escaneo / girar»
  (cambia el tipo de la operación conservando las caras).
- U5 a medias: la curva proyectada elige «Sobre: caras del sólido / la piel del escaneo». **Falta** juntar
  «Curva por puntos» (`CurveDef::Points`, spline por el aire) con «Camino por puntos» (`CurveDef::ScanPath`,
  geodésica por la piel): en el diálogo de la curva (`DesignStep.tsx`, `Match` de `def().type === "points"`
  y `"scan_path"`) un selector «Entre los puntos: curva suave / por la superficie» que convierte entre los
  dos (puntos `scan`/`at` ↔ semillas; para las `at` ya existe el comando `cad_scan_seeds_near` en
  `apps/desktop/src/cad.rs`), y un botón «Agregar puntos sobre la piel» (clics `scan_point` → `PointSpec::Scan`).
  «Camino por puntos» del panel del escaneo queda como atajo.
- U6: los diálogos de Plano, Eje y Punto tienen «Del escaneo: zona plana / 3 puntos / cilindro / un
  punto» (reemplaza la operación por la del escaneo). El panel del escaneo conserva sus atajos.
- U7: tipo de patrón, forma de la primitiva (y tornillo/tuerca/arandela) en el diálogo; «Mover
  pieza», «Hélice o espiral»; «Barrido» sin perfil hace un tubo (o una superficie con dos sketches de
  curvas).
- U8: «Partir» con «Quedarse con: los dos lados / el lado de la normal / el otro lado» (convierte entre
  `split_by` y `split`). Los recortes de superficies quedan como atajos.
- U9: «Redondeo» (aristas, completo, entre caras) y «Chaflán» (aristas, vértice) con tipo en el
  diálogo; el botón elige el tipo por lo que esté elegido (tres caras, dos grupos, un vértice).
- U10: «Resultado: sólido / superficie» en extrusión, revolución, transición y barrido; con curvas
  abiertas la extrusión/revolución/transición/barrido dan superficie solas.
- U11: «Sobre la piel» (Sólidos): clic en el escaneo → eje por la normal de la piel
  (`ScanFit::Normal`, sigue al escaneo), cilindro apoyado en él (`Primitive.on`: giro y separación) y
  su base llevada a la piel (`face_to_scan`).
- U12 a medias: «Detalle completo» en el espesor sobre piel (`ScanBody.exact`): un STL cerrado entra tal
  cual como sólido, sin vóxeles, para unir o grabar sin redondearlo (hasta 300 000 triángulos). **Falta**
  «Exportar para imprimir»: el STL original más lo agregado (relieves, piezas sobre la piel) en un solo
  archivo, sin booleanas (el laminador junta lo que se toca), y los grabados como volumen negativo en 3MF.
  Booleanas de malla propias: no hay biblioteca disponible (solo `robust`).

2026-11-08, en el camino (pedido del usuario: «si un STL tiene fallas, que no deje usarlo y pida
repararlo en Preparar»):

- La reparación avanzada («Unir cuerpos solapados») no cerraba el hueso frontal (581 cruces + 14
  aristas non-manifold): unía los cruces después de separar las aristas, con rendijas abiertas que
  confunden el número de vueltas, y se deshacía. Ahora une primero (sobre la malla todavía cerrada) y
  juzga el resultado ya limpio (`libs/pinocchio/repair/src/lib.rs`, `repair/intersect.rs`): el hueso
  queda cerrado y manifold (0 aristas malas, 6 cruces de 581).
- Diseñar no usa un escaneo con fallas como sólido (`scan_mesh_faults` en `apps/desktop/src/cad.rs`):
  las operaciones dan el error «repáralo en Preparar → Reparar…» y el panel del escaneo avisa con un
  botón «Reparar en Preparar» (`cad_scan_faults`, `onRepairScan` en App.tsx).

2026-11-08, U5 y U12 terminadas (sin compilar ni probar todavía):

- U5: el diálogo de la curva por puntos tiene «Entre los puntos: curva suave / por la superficie»
  (convierte `CurveDef::Points` ↔ `CurveDef::ScanPath`: los puntos `scan` pasan su semilla, los `at`
  caen en la piel más cercana con `cad_scan_seeds_near`, los demás quedan fuera con aviso) y
  «Agregar puntos sobre la piel» (clics seguidos hasta «Listo»). El camino muestra sus puntos y deja
  quitarlos. Los puntos `PointSpec::Scan` dentro de una curva suave ahora siguen al escaneo
  (`fit_scan_references` y `uses_scan` en `apps/desktop/src/cad.rs`).
- U12: «Exportar el escaneo con lo agregado» (panel del escaneo → Para imprimir; comando
  `cad_export_print`): el escaneo tal cual + las piezas del diseño + la herramienta de cada relieve,
  sin booleanas. Las piezas hechas del escaneo (`ScanBody`) no salen (las reemplaza el original). En
  3MF un solo objeto con partes (`converter_3mf::export_3mf_volumes`, rangos de triángulos y
  `volume_type` en `Metadata/Slic3r_PE_model.config`): los grabados como `NegativeVolume`. En STL los
  grabados quedan fuera con aviso. El laminador propio todavía no entiende volúmenes negativos.
  **Falta probar** que Orca/PrusaSlicer abran el 3MF con el grabado restado.

2026-11-08, U13 primera parte (puntos, construir encima, cotas; pruebas del modelo pasan, la app sin
compilar):

- Puntos con nombre: «Puntos en la piel» (panel del escaneo → Puntos y cotas) pone puntos seguidos
  hasta «Listo»; cada uno es un `Point` con `PointSpec::Scan` llamado P1, P2… (`next_point_name` en
  `apps/desktop/src/cad.rs`). El visor escribe el nombre junto a cada punto (`refLabels` en
  `CadView.tsx`).
- Construir encima, con los puntos elegidos (en orden) o los últimos puestos: «Plano por 3»
  (`PlaneDef::ThreePoints`), «Eje por 2» (`AxisDef::TwoPoints`), «Sistema en el punto» (normal de la
  piel + `Frame`), «Pieza en el punto» (`placeOnSkinAt`). Un sketch se ancla eligiendo ese plano.
- Cotas: `FeatureKind::Dimension { measure: DimensionKind }` — distancia (directa o en X, Y, Z),
  ángulo entre tres puntos, por la superficie (camino más corto), diámetro de una zona redonda y
  espesor en un punto. Las del escaneo las completa la app en `fit_scan_references` (camino,
  `pick_cylinder`, rayo hacia adentro). El resultado va como `RefGeom::Dimension` (valor, tramos,
  dónde va el número): el visor lo dibuja en cian con su valor. Pruebas `model/tests/dimension.rs`.

2026-11-08, U13 segunda parte (pruebas del modelo pasan, la app sin compilar):

- Dimensionar el objeto: «Medidas» en el panel del escaneo lista las cotas con su valor; «Escalar…»
  pide cuánto tiene que medir y escala el escaneo alrededor del primer punto de la cota
  (`scaleAbout` en `scanAlign.ts`, por `onAlignScan`: queda como paso del historial del objeto).
  Solo para cotas en mm que miden el escaneo.
- Cotas como parámetros: `Dimension.parameter` da la medida a ese parámetro (lo crea o lo
  reemplaza) antes de resolver las fórmulas (`published_dimensions` en `eval.rs`); sirve con puntos
  sueltos o del escaneo (si no, aviso). Se actualiza con el escaneo.
- Lista de medidas: «Exportar medidas (CSV)» (cota, qué mide, valor, unidad).
- Patrón en puntos de referencia: `PatternKind::AtPoints` (una copia trasladada en cada punto,
  el original en el más cercano); «Repetir en los puntos» en el panel y tipo nuevo en el diálogo
  del patrón.

