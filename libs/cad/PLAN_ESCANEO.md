# Plan: diseñar sobre escaneos y mallas

Plan desde el 2026-10-10. Pedido del usuario: «quiero trabajar como un profesional creando
piezas sobre escaneos o stl previos». Dos usos, los dos importan, **primero B**:

- **B — Diseñar algo nuevo que calce** sobre el escaneo: adaptador que encaja, soporte o base,
  carcasa, férula, molde o negativo.
- **A — Reconstruir** la pieza escaneada como CAD limpio y editable (ingeniería inversa).

El usuario no tiene escáner por ahora: todo se prueba con STL/OBJ cargados (fase E0).

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta.

## Punto de partida (lo que ya hay)

| Función | Estado |
|---|---|
| Escaneo visible translúcido en Diseñar | ✅ (`CadViewer.setScan`) |
| Plano de trabajo / contorno + extrusión desde zona plana | ✅ (`ScanFeature::WorkPlane/PlaneOutline`, profundidad por rayos) |
| Agujero o tetón desde zona cilíndrica | ✅ (`ScanFeature::Cylinder`) |
| Corte de la malla por plano → sketch | ✅ (`ScanFeature::Slice`, círculos, paralelas y perpendiculares) |
| Detección automática (plano, cilindro, esfera) | ✅ (`cad_scan::detect_all`, `merge_coplanar`) |
| Desviación diseño ↔ escaneo | ✅ (`cad_scan::deviation`, mapa de color) |
| Limpiar la malla (reparar, remallar, simplificar, suavizar) | ✅ (Preparar) |
| Escaneo como cuerpo para booleanas | ✅ con holgura por vóxeles (E2) |
| Elegir qué objeto es el escaneo | ✅ (E1.1) |
| Alinear el escaneo por rasgos | ✅ (E1.2) |
| Referencias (punto, plano, eje) tomadas de la malla | ✅ ligadas al escaneo (E1.3) |
| Sketch anclado a la malla | ✅ corte como guía, anclajes y «Usar el corte» (E1.4) |
| Extruir hasta el escaneo | ✅ extrusión, cara y nervio (E3) |
| Formas que siguen la piel (engrosar parche, carcasa, molde) | 🟡 carcasa y parche bajo sketch (E4.1) |
| Curvas sobre la malla | ❌ (E5) |
| Verificar calce (choque/holgura firmada) | 🟡 desviación sin signo de calce (E6) |
| Ajuste de cono, toro, redondeos | ❌ (E7) |
| Revolución / recubrimiento desde cortes | ❌ (E8) |
| Asistente de reconstrucción | ❌ (E9) |
| Superficies libres ajustadas a la malla | ❌ (E10) |

## Problema anotado: las mallas que vienen del CAD se ven y se trabajan mal (2026-10-10) ✅

El usuario: «nuestro gran problema». Un cubo hecho en Diseñar y pasado a Fabricar muestra
abollones en las caras donde se juntan los vértices de los triángulos; con retopología queda
redondeado (malo para piezas cuadradas, sobre todo de CAD a Fabricar); «reparar» al pasar a
malla queda terrible. El STL exportado directo desde el diseño se ve perfecto en OrcaSlicer,
pero **el mismo STL importado en Pinocchiors se ve y se trabaja mal**: el archivo está bien y el
problema es cómo lo toma la app.

Revisado el mismo día:
- ✅ **Abollones = solo sombreado.** La geometría no cambia (al soldar se juntan vértices de
  posición idéntica; Reparar en el STL del diseño solo quitó 2 caras inválidas, mismo
  volumen). Una sola normal por vértice en las aristas vivas, interpolada, hacía manchas en
  las caras planas. Ahora el backend marca por vértice si toca una arista viva (> 40° o de más
  de dos caras, `MeshData.creases`) y el shader de `lib/autoSmooth.ts` dibuja planos los
  triángulos que tocan una: caja plana, esfera y costados de cilindros suaves, escaneos con
  ruido sin facetas falsas. Sin duplicar vértices: pesos, pintura y skinning siguen por índice.
- ✅ **Retopología redondeada**: «Conservar aristas vivas» venía apagado. Con el STL del diseño
  (caja, cilindro, esfera, cono) a 2000 quads: apagado redondea caja y cilindro (máx. 1,13 %
  de la diagonal); encendido quedan vivas (0,49 %). Ahora viene encendido.
- ✅ Puente de pruebas: no tenía `get_scene_structure`, la grilla quedaba en metros y la
  cámara entraba en la pieza al pasar a Fabricar (solo en el arnés, no en la app).
- e2e `node e2e/cad.mjs "mallas:"`.

Si después de esto algo se sigue viendo o trabajando mal, conseguir el caso concreto (archivo
y pasos) antes de tocar Reparar.

Meta: un STL de CAD tiene que verse y medirse igual que en un laminador, y pasar de Diseñar a
Fabricar sin perder caras planas ni aristas vivas. Va antes de E1: B también parte de piezas
de CAD y de STL.

## Fases — B (diseñar sobre el escaneo)

### E0 — Banco sin escáner ✅ (2026-10-10)

- `cargo run --release -p cad-scan --example scanify -- entrada salida.(stl|obj) [opciones]`:
  remallado isótropo sin aristas vivas (vóxeles si la malla no es manifold), oclusión de plato
  giratorio (2 alturas × N giros, buffer de profundidad; no ve desde abajo de la mesa), huecos
  de borde irregular, ruido sobre la normal, ondulación suave y `--pose` (matriz en
  `salida.pose.txt`). Conejo: queda 54 % del área (es una placa: la cara de abajo no se ve);
  gonfoterio 92 %; pieza de Onshape 88 %. 3–11 s.
- Modelos: orgánicos para B (gonfoterio, conejo, oído interno), mecánicos para A (piezas de
  Onshape «Part Studio …», molde, espéculo, audiómetro).
- `cargo run --release -p cad-scan --example mesh_boolean -- malla N [--voxeles]`: malla a N
  triángulos → sólido OCCT → caja − malla, ∩, ∪; revisa corte + intersección = caja.
- **Hallazgos**:
  1. El cosido (`BRepBuilderAPI_Sewing`) tardaba 9–32 s con 20–50 k triángulos. Ahora
     `cad_from_mesh` arma el sólido directo compartiendo vértices y aristas si la malla es
     cerrada, manifold y orientada (20 k: 0,8 s; 50 k: 2 s) y solo cose si no.
  2. meshopt deja pares de caras espalda con espalda, aristas de más de dos caras y cruces:
     con eso OCCT no cierra el sólido o lo da inválido y la resta/unión fallan. Preparar por
     **vóxeles + isótropo** (cerrada, manifold, sin cruces) anduvo en los 4 modelos.
  3. Con malla buena las booleanas son exactas (corte + intersección = caja a 1e-6 %) y
     cuestan ~0,2 ms por cara: 5 k ≈ 1 s, 20 k ≈ 3–4 s, 50 k ≈ 8–11 s.
- **Decisión para E2**: OCCT (sin manifold). Escaneo → vóxeles (con el desfase de holgura ya
  incluido) → isótropo a ~20 k triángulos por defecto (hasta 50 k a pedido) → sólido directo.
  Cerrar los bordes abiertos que deje el isótropo antes de convertir: con 6 aristas abiertas
  cayó al cosido (89 s).

### E1 — El escaneo como referencia del diseño

Partes: E1.1 escaneo elegido ✅, E1.2 alinear por rasgos ✅, E1.3 referencias ligadas al
escaneo ✅, E1.4 sketch anclado al corte ✅. **E1 cerrada (2026-10-10).**

- ✅ E1.1 (2026-10-10): el escaneo es un objeto importado elegido, no «el activo».
  `Document.scan_object` guarda la elección (si no hay: el importado activo o el primero con
  malla; selector «Escaneo» en la pestaña si hay más de uno). La app le avisa al backend con
  `cad_scan_use`; `with_scan` toma la escena del casillero del objeto (caché por huella de sus
  bytes) o la activa. Con una pieza del diseño activa, el escaneo se sigue viendo y midiendo.
  De paso: el STL importado calculaba las normales con la del archivo (`facet normal 0 0 0`
  dejaba la malla negra); ahora por el orden de los vértices, como los laminadores.
  e2e «escaneo: sigue de referencia con una pieza activa».
- ✅ E1.2 (2026-10-10): «Alinear el escaneo» en la pestaña: cara al piso (normal hacia −Z y
  en z = 0), cara al frente (gira en torno a Z hasta mirar a −Y), eje vertical (cilindro → Z,
  por el origen en planta) y punto al origen. `lib/scanAlign.ts` arma la matriz en el CAD y
  la pasa al modelo (`toSceneMatrix`); App activa el escaneo y la aplica como una orientación
  más (historial y deshacer, mueve quads/esqueleto/rig; queda como modificación del objeto en
  el Outliner). Pruebas node `e2e/scanAlign.test.mjs` y e2e «escaneo: alinear…». Pendiente:
  alinear por 3 puntos y eje → X.
- ✅ E1.3 (2026-10-10): referencias del árbol ligadas al escaneo: plano de una zona, plano
  por 3 puntos, eje de un cilindro, punto (`PlaneDef/AxisDef/PointSpec::Scan { fit, … }`,
  `cad_model::ScanFit`). Guardan dónde se hizo clic (`ScanSeed`: triángulo + baricéntricas +
  punto en mm + huella de los triángulos), no coordenadas: antes de cada recálculo la app las
  ajusta contra el escaneo de hoy (`fit_scan_references` en `evaluate_doc`, caché por escaneo
  y semilla; `cad_scan::fit_reference`). Siguen al escaneo si se alinea, se deshace o se mueve
  (mismo triángulo) y si se repara o remalla (punto más cercano al último lugar). Lo que se
  apoya en ellas (sketches, extrusiones) se rehace solo. El visor del CAD pide recálculo
  cuando cambia la malla del escaneo (`refreshScanReferences`). Prueba `references_follow_the_scan`
  y e2e «escaneo: referencias ligadas siguen al alinear». Pendiente: centro de una esfera.
- ✅ E1.4 (2026-10-10): al editar un sketch, el corte del escaneo por su plano se ve como
  guía naranja punteada (`CadViewer.setScanSection`), sus líneas y círculos reconocidos son
  anclajes (entran en `solidRefs`) y «Usar el corte» en el panel los copia al sketch con sus
  horizontales/verticales/paralelas (`cad_scan_outline` → `outline_sketch`; `ui.scanOutline`).
  e2e «escaneo: el sketch se ancla al corte del escaneo». Pendiente: tramos abiertos (escaneo
  con huecos) solo se ven, no se copian; el corte copiado no queda ligado al escaneo.

### E2 — El escaneo como cuerpo ✅ (2026-10-10)

- ✅ `FeatureKind::ScanBody(ScanBody { offset, faces, op })` («Cuerpo del escaneo»): el
  escaneo elegido como sólido que se une, resta o interseca como cualquier herramienta
  (con piezas, por defecto resta con 0,2 mm de holgura: la cuna o el encaje). La malla
  preparada no se guarda: la app la pone antes de cada recálculo (`fit_scan_references`,
  caché por escaneo + holgura + triángulos en `ScanCache.bodies`) y de ella solo viaja la
  huella (`ScanBodyMesh`, entra en la caché de operaciones). Sigue al escaneo como las
  referencias: si se alinea, repara o se elige otro, se rehace.
- ✅ `cad_scan::prepare_body`: vóxeles con desfase → isótropo → sólido directo. El desfase
  va en la reconstrucción por vóxeles (`VoxelOptions.offset`, `rebuild_with_limit`): la
  banda se ensancha la holgura y adentro = distancia exacta con signo < holgura (el signo,
  el del volumen sin correr: con el del número de vueltas recalculado, en escaneos abiertos
  los cruces quedaban sobre la grilla y salía no manifold). Esfera ±0,2 y cubo redondeado
  exactos al 1 %. El lado del isótropo se calcula con el área ya cerrada y corrida (los
  huecos tapados y el ruido la cambian ×1,5–2) y ¾ de relleno medido. Se tiran las motas
  (< 1 % del volumen mayor). Al soldar se caen astillas sin área y quedan agujeros en fila o
  microscópicos: se funde el vértice más llano con su vecino (`close_open_edges`); los
  demás agujeros, abanico; si algo queda, otro intento con el lado +1,3 %.
  Sin eso OCCT cose (89 s en vez de 1,4 s).
- ✅ Visor: los cuerpos de malla (≥ 64 caras de un triángulo) no dibujan las aristas entre
  facetas a menos de 30° (`hide_facet_edges`, quedan vacías para no correr los índices).
- ✅ UI: «Escaneo como cuerpo» en la pestaña del escaneo; en el diálogo, holgura,
  triángulos (500–100 000) y operación.
- Banco: `cargo run --release -p cad-scan --example scan_body -- malla N holgura [salida.stl]`
  (`CORTE`, `SOLO_PREPARAR`). A 20 000 triángulos: preparar 2–5 s, a sólido 0,8 s, caja −
  cuerpo 3,5–4,5 s (conejo, gonfoterio y sus escaneos con huecos, holgura 0 / +0,3 / −0,5);
  50 000: 4 + 2,6 + 13 s. Volúmenes: conejo cerrado 13 778 → +0,3 mm 17 489 (≈ V + A·0,3).
  Prueba `scan_body_leaves_a_cradle_with_clearance` (placa con ruido + 0,3 mm,
  volumen = V + A·0,3 al 2 %; bloque − escaneo = mitad agrandada al 2 %) y e2e «escaneo:
  cuerpo del escaneo deja la cuna en un bloque».
- Pendiente: superficie si el escaneo no cierra (con vóxeles siempre cierra); OCCT falla la
  booleana si la cara de la otra pieza cae justo sobre una zona plana del escaneo (conejo
  −0,5 mm cortado a media altura: «el resultado no es un sólido válido»; al 37 % o 62 %
  anda): avisar o correr la herramienta una centésima; avisar el tiempo antes de recalcular
  con muchos triángulos.

### E3 — Extruir hasta el escaneo ✅ (2026-10-10)

- ✅ `Extent::UpToScan { offset }` («Hasta el escaneo» en la extrusión, con «Separación»):
  prisma que pasa el escaneo entero − el escaneo corrido la separación, y quedan los sólidos
  que arrancan en el plano del perfil (`until_scan`). Va hacia el lado donde los rayos desde
  el perfil encuentran el escaneo. Si parte del perfil no queda frente al escaneo (el prisma
  llegaría hasta el final) da error en vez de una columna infinita.
- ✅ `FeatureKind::FaceToScan { faces, offset }` («Cara al escaneo», en Modificar): caras
  planas hasta la piel; suma si el escaneo está afuera de la pieza, resta si está adentro.
- ✅ Nervio con `scan: Some(ScanReach { offset })` («Hasta el escaneo» en el diálogo): las
  paredes llegan a la piel (32 tramos por línea) sin meterse.
- La app pone la malla igual que en E2 (`SCAN_FACES` = 20 000; comparte caché con los
  cuerpos de igual holgura) y el sólido del escaneo queda en caché en el modelo
  (`scan_solid`, los últimos 4 por huella).
- Pruebas: `extrude_up_to_the_scan_follows_the_curved_skin` (cilindro hasta una esfera,
  volumen analítico al 1 %), `face_and_rib_up_to_the_scan` (cara y nervio, y el error de la
  cara más ancha que el escaneo); e2e «escaneo: extruir y llevar una cara hasta el escaneo».
- Mallas de más de un millón de triángulos (los escaneos reales): preparar 5,7 s con
  1 M, 3,7 s con 500 k; el sólido y las booleanas no cambian (se trabaja a 20 000).
- Pendiente: desmolde con «hasta el escaneo»; caras no planas en «Cara al escaneo»; elegir
  los triángulos del escaneo en estas operaciones (fijo en 20 000).

### E4 — Formas que siguen la piel

Partes: E4.1 carcasa y parche bajo un sketch, E4.2 región pintada con pincel, E4.3 molde.

- E4.1 carcasa y parche:
  - `ScanBody.thickness`: carcasa de ese espesor desde la piel corrida la holgura (negativo:
    hacia adentro). Sale de una sola pasada de vóxeles (`VoxelOptions.thickness`: adentro =
    distancia con signo entre los dos desfases), no de dos booleanas. Un escaneo cerrado da
    dos pieles (la de afuera y la del hueco); uno abierto, una «taza» que rodea el borde.
  - `ScanBody.region` (sketch + regiones): el prisma de las regiones (atraviesa todo) ∩ la
    carcasa, y de los pedazos solo los primeros que encuentran los rayos desde el sketch
    hacia cada lado (sketch arriba de un brazo: la cara de arriba, no la de abajo). Paredes
    del borde paralelas a la normal del sketch.
  - UI: «Carcasa» y «Parche desde sketch» en la pestaña del escaneo (2 mm afuera, 0,2 mm de
    holgura, pieza nueva); en el diálogo, Forma (lleno / carcasa afuera / adentro), Espesor y
    «Solo bajo» (sketch + regiones).
  - Hallazgos: (1) en un escaneo abierto el signo de la distancia salta en los huecos y la
    piel de adentro rodea el borde como un tubo del grosor de la holgura; el isótropo lo
    pliega (~900 cruces en el conejo) y OCCT da booleanas vacías sin avisar. Se cierra el
    escaneo primero por vóxeles (del doble de lado: la carcasa cuesta según los triángulos
    que recibe) y la carcasa sale de esa piel cerrada. (2) Lado del isótropo ≤ espesor. (3)
    El isótropo reproyecta solo sobre caras que miran igual (`Bvh::query_closest_facing`):
    en láminas delgadas la piel de enfrente puede estar más cerca. (4) Triángulos-astilla
    (altura < 1 % del lado) se giran o se funden (`fix_slivers`). (5) `cad_from_mesh` arma
    una cáscara por pieza conexa: las de volumen negativo son huecos del sólido que las
    contiene; orientada por volumen con signo. (6) En `rebuild` el corte nunca a menos de
    1e-3 vóxel de un punto de la grilla (la soldadura abría agujeros).
  - Límites: espesor ≥ 2,5 vóxeles (la grilla tiene tope de 640 por eje: en el gonfoterio de
    1 m, ≥ 4,2 mm) y ≤ 100 000 triángulos estimados (lado ≤ espesor; gonfoterio ≥ 7 mm, y
    aun así tarda minutos). Conejo, 2 mm: preparar 12 s, sólido 0,9 s, caja − carcasa 4 s.
  - Pruebas: `voxel_shell_keeps_the_band_between_two_offsets` (quadriflow),
    `scan_shell_and_patch_under_a_sketch` (esfera: carcasa con hueco al 2 %, parche solo
    arriba) y e2e «escaneo: carcasa y parche bajo un sketch». Banco: `ESPESOR=2` en
    `scan_body`.
  - Pendiente: la caja − cuerpo del conejo a −0,5 mm cortado a media altura sigue fallando
    (no eran las astillas); paredes del parche perpendiculares a la piel (llega con E4.2).
- E4.2 región pintada ✅ (2026-10-15, sin probar en la app real): `ScanBody.paint = ScanPaint { radius, dabs }`;
  cada toque (`ScanDab`) guarda su `ScanSeed` (triángulo + baricéntricas, como las referencias de E1.3) y la app
  rellena `point` y `normal` en `fit_scan_references` (`ScanMesh::fit_dab`), así sigue al escaneo si se alinea o
  se repara. El parche es carcasa ∩ unión de cilindros de radio `radius` a lo largo de la normal de cada toque
  (borde perpendicular a la piel), y de los pedazos solo los que cada toque ve desde afuera (`scan_paint`; la cola
  de `scan_patch` pasó a `keep_scan_pieces`). `paint` manda sobre `region`. UI: «Pincel» + radio en la pestaña
  del escaneo (arrastrar con el botón izquierdo; toques cada medio radio, máx. 80; comando `cad_scan_dabs`).
  Prueba: `scan_paint_patch_follows_the_brush` (esfera: volumen de la tapa de radio 8). Falta: e2e, campo por
  vértice con borde suave (hoy son cilindros), borrar/editar los toques después, geodésica en vez de cilindro.
- E4.3 molde ✅ (2026-10-15, sin probar en la app real): `cad_scan::add_mold` (comando `cad_scan_mold`) agrega al
  diseño, como operaciones normales y editables: dos cajas alrededor del escaneo partidas en un plano (eje y
  porcentaje a elegir), «Molde: cavidad» (`ScanBody` restado con la holgura), tres llaves de registro en tres de las
  cuatro esquinas, a mitad de pared (así la tapa solo calza de una forma; perno en la de abajo que entra 1 mm en su
  pared, hueco con 0,2 mm de juego en la de arriba, cada una con su alcance) y un bebedero cilíndrico al centro de
  la de arriba. UI: «Crear molde» en la pestaña del escaneo. Prueba: `scan_mold_makes_two_halves_with_keys_and_sprue`
  (esfera: volumen de las dos mitades, cada una de su lado del plano, pernos que salen). Falta: partir por la línea
  de mayor contorno (hoy solo plano), respiraderos, bebedero que llegue a la cavidad si el centro no la toca,
  escalones de la cavidad por encima del plano (rebajes) y la colocación de las llaves en el contorno.

### E5 — Curvas sobre la piel ✅ (2026-10-15, sin el borde de región; sin probar en la app real)

- `CurveDef::ScanPath { seeds, closed, points }`: camino más corto (geodésica) por la piel entre los puntos tocados.
  `ScanMesh::geodesic` (`scan/src/geodesic.rs`): Dijkstra por las aristas y después se endereza (puntos parejos
  corridos al punto de la piel más cercano a la mitad de sus vecinos, ≤ 100 por tramo). Las semillas son las de
  E1.3, así que sigue al escaneo si se alinea o se repara (la app lo vuelve a trazar en `fit_scan_references`,
  con caché por semillas). Cerrada con `closed`. Es una spline por esos puntos: sirve de camino de barrido,
  de patrón en curva y de herramienta de partición como cualquier curva del historial.
- `CurveDef::ScanProjected { sketch, entities, flip, offset, mesh }`: las curvas de un sketch (una cadena) llevadas a
  la piel del escaneo cerrado (a `offset` de ella) a lo largo de la normal del sketch; lo primero que se ve desde el
  plano. Muestrea la curva, tira un rayo por punto contra el cuerpo del escaneo y une los impactos en una spline
  (≤ 150 puntos).
- UI (pestaña del escaneo, «Curvas sobre la piel»): «Camino por puntos» (clics, «Terminar» / «Cerrada» /
  «Cancelar») y «Sketch a la piel» (el sketch elegido o el último). Comando: `ScanFeature::PathCurve` por `cad_scan_add`.
- Pruebas: `scan_geodesic_follows_the_great_circle` (cuarto de círculo máximo ±1,5 %, anillo de 3 puntos = 2πR),
  `scan_curves_project_and_path` y e2e «escaneo: curvas sobre la piel».
- Falta: usarlas como borde de región del parche (E4: hoy el borde sale de un sketch o del pincel), probar el
  barrido de un perfil a lo largo del camino sobre un escaneo real (el barrido por curva ya existe), curva por
  puntos con geodésica entre los puntos de otra referencia, y la geodésica exacta (hoy es una aproximación por
  enderezado; en mallas muy irregulares puede quedar algo larga).

### E6 — Verificar el calce ✅ (2026-10-15, holgura firmada; falta el corte vivo)

- Reusa `cad_deviation` (distancia con signo del centro de cada triángulo del escaneo al sólido: + afuera del
  sólido = holgura, − = el escaneo entra en el material). En la tarjeta «Comparar el diseño con el escaneo»,
  «Verificar el calce (holgura)» pide holgura mínima y máxima y clasifica cada triángulo (`fitClass` en `cad.ts`):
  **choca** (rojo, < 0), **apretado** (naranja, < mínima), **en rango** (verde) y **suelto** (azul, > máxima). En
  cada vértice manda el triángulo con menos holgura, así el choque no se diluye.
- Informe (`fitReport`, pesado por área): % en cada clase, menor y mayor holgura con el punto (mm) donde están y
  «Marcar los choques» (resalta los triángulos que chocan). Los triángulos y áreas salen del visor
  (`CadViewer.scanTriangles`).
- Pruebas: `e2e/scanFit.test.mjs` (node: clases y informe) y e2e «escaneo: verificar el calce de una pieza con
  holgura» (bloque con el hueco de una esfera a +0,3 mm: casi todo en rango y sin choques; a −0,3 mm: choca casi
  todo). Falta: corte vivo que muestre pieza y escaneo juntos, el informe por pieza (hoy contra todo el sólido) y
  la holgura medida por la normal del escaneo en vez de la distancia al punto más cercano.

## Fases — A (reconstruir la pieza)

### E7 — Más ajustes ✅ (2026-10-15, salvo la extrusión libre; sin probar en la app real)

- `scan/src/quadric.rs`: **cono** y **toro** como superficies de revolución. El eje sale de que toda recta normal
  corta al eje (Plücker: autovector menor de una matriz 6×6); con el eje cada punto pasa a (altura, radio) y el
  perfil es una recta (cono: vértice y semiángulo) o un círculo (toro: radios mayor y menor). Descarta los puntos
  que no calzan (la zona que se escapó a un plano tangente) y reajusta. Usa los centroides con la normal exacta de
  su cara (los vértices con la normal de una cara vecina desvían la recta y el ajuste falla justo en el límite).
- `pick_cone` / `pick_torus` (clic en la pared): botones «Cono» y «Toro» en «Tomar del escaneo», tarjeta con
  semiángulo, radios y largo; «Agregar el cono» (macizo une, hueco resta) / «Agregar el toro» crean la primitiva con
  su eje y medidas. `ScanFeature::Cone/Torus`.
- `detect_all` ahora también devuelve conos y toros (`DetectedShape::Cone/Torus`) y **redondeos**: un tramo de
  cilindro que cubre ≤ 190° y toca dos planos paralelos a su eje se rotula `Fillet { radius, angle }` («Redondeo r 5 ·
  90°») para escribir ese radio en un redondeo del diseño. Lo que sobra entre planos (el redondeo que la zona
  suave mezcla con las paredes) se ajusta aparte por tramos conexos.
- Pruebas: `scan_picks_a_tilted_cone` (cono inclinado: eje, vértice, semiángulo, tramo y radios), `scan_picks_a_torus`,
  `scan_detects_cone_torus_and_fillet` (toro de malla propia; el redondeo de un bloque r 5: radio ±0,1 y 90° ±15°)
  y e2e «escaneo: cono y toro con un clic».
- Límites: con triángulos más chicos que la tolerancia (teselado fino de CAD, tolerancia por defecto 0,2 % de la
  diagonal) todas las normales son «poco confiables» y `detect_all` no segmenta: sube la tolerancia o usa el clic.
  Un redondeo con facetas gruesas se pega a los planos de al lado. Redondeo sin botón de agregar: solo informa.
- **Extrusión libre** (zona de sección constante → perfil + largo): no hizo falta código nuevo. Un clic en la tapa
  plana ya da el contorno (sketch) y el espesor medido (`PlanePick::depth`), y «Sketch y extruir» crea la
  extrusión con ese largo.

### E8 — Revolución y recubrimiento desde cortes ✅ (2026-10-15, sin probar en la app real)

- `cad_scan::add_turned` (comando `cad_scan_turned`): corta el escaneo por un plano que contiene un eje (el de un
  cilindro, cono o toro elegido con un clic), recorta los contornos cerrados a un lado del eje (x ≥ 0, cerrados
  sobre el eje), los pasa a sketch sin reconocer círculos (son perfil) y agrega la revolución de 360° alrededor
  del eje. Sirve para piezas torneadas (el espéculo, jarrones, poleas).
- `cad_scan::add_skin` (comando `cad_scan_skin`): `count` cortes paralelos perpendiculares a una dirección,
  entre el 2 % y el 98 % del escaneo, cada uno con su contorno exterior más grande como sketch, y el recubrimiento
  (`Loft`) que pasa por todos.
- UI: la tarjeta de «Cilindro / agujero», «Cono» o «Toro» tiene «Revolución desde el corte» y «Recubrir cortes»
  (con la cantidad de cortes). Los dos quedan como operaciones editables del árbol.
- Pruebas: `scan_turned_part_is_rebuilt_by_revolution` (jarrón con panza, cuello y hueco: mismo volumen ±2 %),
  `scan_skin_through_parallel_cuts` (esfera con 9 cortes: 90–102 % de su volumen) y e2e «escaneo: revolución y
  recubrimiento desde cortes» (jarrón en STL: volumen por Pappus ±3 %).
- Falta: elegir el plano de corte a mano (hoy es uno cualquiera por el eje; si la pieza no es de revolución el
  perfil sale de ese plano), varios lazos por corte en el recubrimiento (hoy solo el exterior más grande), perfil
  con líneas y arcos reconocidos en vez de segmentos simplificados, y revolución parcial (sector).

### E9 — Asistente de reconstrucción ✅ (2026-10-15, prismáticas y de revolución simples; sin probar en la app real)

- `cad_scan::propose` / `apply_proposal` (`scan/src/assistant.rs`; comandos `cad_scan_propose` y `cad_scan_apply`): del
  `detect_all` arma pasos numerados y deterministas, con lo que cubre cada uno:
  - **Base**: la cara plana más grande, su contorno como sketch y la extrusión hacia adentro hasta el espesor medido.
  - **Agujeros** y **tetones** (cilindros), **conos** y **toros** como primitivas con su eje y medidas.
  - **Redondeos**: solo informan (radio y ángulo; los de eje paralelo a la base ya vienen en el contorno).
  - No repite lo que ya está en otro paso: un agujero pasante que es un lazo interior del contorno de la base, o el
    borde redondo de una base circular. Los cilindros que cubren menos de 0,3 de la vuelta se ignoran.
  - `covered`: fracción del área del escaneo que explican los pasos (la base cuenta sus paredes y la cara de enfrente).
- UI («Asistente de reconstrucción» en la pestaña del escaneo): «Proponer un árbol» lista los pasos con casilla (los
  toros vienen sin marcar), el detalle y, al tocar uno, resalta sus triángulos en el escaneo; «Crear lo marcado» agrega
  las operaciones (sketch, extrusión, primitivas: normales y editables) y **al final compara con el escaneo**
  (colores de desviación y «Dentro de tolerancia»).
- Pruebas: `assistant_proposes_and_rebuilds_the_plate` (placa con agujero pasante y tetón: pasos Base + Tetón, cubre
  > 85 %, volumen reconstruido ±2 %, un paso suelto no arrastra la base) y e2e «escaneo: asistente de reconstrucción
  propone base y tetón» (disco con tetón: el borde del disco se descarta; volumen ±2 %; desviación > 90 % dentro de
  tolerancia; 4/4 corridas).
- Límites: una sola base (la cara plana más grande); el espesor es la mediana de rayos desde esa cara, así que con
  un tetón grande debajo de la cara el espesor puede mezclar los dos niveles; agujeros ciegos y escalones salen como
  primitivas sueltas; no reconoce patrones (varios agujeros iguales) ni simetrías; los redondeos no se crean; nada
  de piezas de varias caras de apoyo. Para piezas torneadas usar «Revolución desde el corte» (E8).

### E10 — Superficies libres ✅ (2026-10-15, por relleno de OpenCASCADE; sin probar en la app real)

- `FeatureKind::ScanSurface { seed, angle, thickness, close, op, boundary, interior, close_plane }`: la zona suave
  (normales vecinas a menos de `angle`°, 25 por defecto) bajo un clic, ajustada con una superficie de forma libre.
  Sola es una superficie; con `thickness` una cáscara (`thicken`); con `close` (clic en una cara plana del
  escaneo) un **sólido cerrado contra ese plano**: la superficie, una pared reglada entre su borde y su proyección
  sobre el plano y el fondo plano, cosidos (`sew`).
- `ScanMesh::free_zone` (`scan/src/freeform.rs`): crece la zona, toma su lazo de borde más largo y lo reparte en
  12–40 puntos (más segmentos hacen el relleno de OCC 40 veces más lento: 120 → 9 s por intento, 40 → 0,25 s), y
  puntos de adentro. `FreeZone::fit_surface` prueba 12…100 puntos de adentro (los más lejanos entre sí) y se queda con
  la cara **válida** de menor desviación contra los triángulos de la zona: con puntos de más o de menos
  `BRepOffsetAPI_MakeFilling` entrega caras inválidas o apartadas. Los puntos quedan guardados en la operación (se
  rehace igual aunque cambie el algoritmo) y la app los vuelve a ajustar si el escaneo cambia (`zones`, en caché).
- UI («Superficie libre» en la pestaña del escaneo): espesor, casilla «Cerrar contra una cara plana (sólido)» y
  «Elegir la zona» (clic en la zona; con la casilla, segundo clic en la cara plana). `ScanFeature::FreeSurface`.
- Pruebas: `scan_free_surface_closes_into_a_solid` (casquete de esfera: zona entera, desviación < 0,8 mm, superficie
  sola y sólido cerrado con el volumen ±4 %) y e2e «escaneo: superficie libre de una cúpula cerrada contra una cara
  plana» (cúpula sobre pedestal y placa: volumen ±5 %).
- Límites: es relleno por restricciones, no un ajuste de mínimos cuadrados (`GeomAPI_PointsToBSplineSurface` pide una
  grilla de puntos que una zona cualquiera no tiene); el borde exterior se aproxima con una polilínea de ≤ 40
  tramos (se pierden esquinas finas); solo el lazo exterior (los huecos de la zona se rellenan); no ata la superficie
  a las vecinas analíticas con tangencia; cerrar exige un plano debajo de todo el borde (un borde que se cruza con su
  proyección da un sólido inválido); la desviación se mide sobre vértices de la cara, no en todo el parche.

## Riesgos

- Booleanas OCCT: ~0,2 ms por cara; más de 50 k triángulos se vuelve lento (E0).
- Escaneos abiertos: hay que cerrar antes de sólido; huecos grandes no se inventan.
- Desfase por vóxeles pierde detalle fino según la resolución: tamaño de vóxel ≤ holgura / 2.
