# Plano 2D: dibujo y edición sobre la hoja

Plan del plano 2D desde el 2026-10-10. Reemplaza cualquier plan anterior del plano (lo hecho
queda en la bitácora de [ROADMAP.md](ROADMAP.md)). Sale de comparar el plano actual con la
lista de edición 2D que entregó el usuario (comportamiento general, entrada de valores,
recortar, extender, dividir y unir, desfase, redondeo y chaflán 2D, simetría, patrones, mover,
estirar, arrastre, splines, construcción, limpieza, selección y retroalimentación), más dos
pedidos suyos: **girar en 3D las vistas que se ven en 3D** e **imágenes de secciones**.

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta · (F*n*) fase que lo cubre.

## Dónde estamos

El plano (`components/layout/DrawingView.tsx`, `lib/drawing.ts`, comando `cad_drawing`) arma la
hoja y la anota: seis vistas más isométrica y vistas nuevas con líneas ocultas exactas (HLR),
primer o tercer diedro, escala normalizada (también por vista), cajetín, cotas generales, cotas a
mano (largo, Ø, radio, distancia entre paralelas, ángulo; recordadas por un punto y que siguen al
modelo), detalles, cortes (A-A, por línea, escalonados, medio corte, rotura, sombreados y con
rayado según el material), varios planos por documento, una capa de dibujo con el editor del
sketch, la tabla y las marcas de los engranes (F8 de PLAN_ENGRANES), y salida SVG / PDF / DXF.

El resto de este documento es el avance fase por fase. Estado al 2026-10-15: F1–F4 hechas, F5
a medias (lo que falta del editor está en la tabla de cobertura) y F6 con sus tres puntos
hechos; «Después, más del plano» sigue sin definir con el usuario. El backend ya acepta
cualquier dirección de vista (`eye`, `xdir`) y cualquier plano de corte
(`DrawingSection { origin, normal }`): girar vistas y cortar por otros planos no necesitó C++
nuevo.

## Decisión (usuario, 2026-10-10): reutilizar el editor del sketch

La capa de dibujo de la hoja **es un sketch**: mismo modelo (`Sketch`), mismo solver
(`cad_solve_sketch` ya resuelve un sketch suelto, sin documento) y mismo editor (herramientas
de `CadView`/`cadUi`, `sketchEdit`, `sketchTransform`, `sketchSnap`, `sketchSplines`,
`sketchCheck`, cotas y restricciones). Así todo lo que el sketch ya sabe (recortar, extender,
desfase de cadena, redondeo y chaflán, simetría, patrones, mover/girar/escalar/estirar,
arrastre con solver, splines, construcción, limpieza, deshacer) llega a la hoja de una vez, y lo
que se agregue después sirve para los dos. Se descartó una capa aparte sin solver (más trabajo y
peor resultado).

## Fases

### F1 — Capa de dibujo en la hoja ✅

*Hecha el 2026-10-10.* «Dibujar» en el plano abre el editor del sketch sobre la hoja
(`cadUi.editSheet`, sesión con `feature = SHEET_SKETCH` y `sheet { width, height, backdrop }`,
plano XY del visor en mm de hoja, Y hacia arriba desde la esquina de abajo a la izquierda). La
hoja de fondo es el SVG del plano rasterizado (hasta 4096 px) y puesto como calco que no se
guarda. Se ve igual que en el plano (usuario, 2026-10-11: el cambio a fondo oscuro «es un
efecto feísimo»): papel blanco en los dos temas, sin tone mapping, el fondo de alrededor
`bg-darker`, encuadrada justo debajo de la barra del sketch con 16 px de margen, sin cubo de
vistas ni ejes, y lo dibujado, las cotas y los símbolos con la paleta clara
(`data-palette="light"`, `themeHex(name, true)`). Con la hoja abierta el visor oculta el
sólido, el escaneo, los planos, las referencias, los otros sketches y la rejilla, y no ancla a
aristas del sólido; la barra no tiene «Usar» y el panel no tiene «Usar del modelo» ni corte u
ocultar modelo. «Volver al plano» guarda en `doc.drawing.sketch` (las fórmulas quedan en sus
cotas) y vuelve a abrir el plano; «Descartar» vuelve sin guardar. Lo dibujado sale en el SVG y
el PDF (`sheetLayerLines` + `layer` en `sheetSvg`) y en el DXF de la hoja (capa DIBUJO; no en
una vista sola a 1:1). Deshacer: el del sketch mientras se dibuja, y una entrada del documento
al volver. Pruebas: node `drawing.test.mjs` (lo dibujado: construcción y capas ocultas fuera, Y
invertida en SVG, DXF en tramos) y e2e "plano 2D: dibujar sobre la hoja con el editor del
sketch". De paso, el e2e del plano buscaba `[aria-label="Plano 2D"]` y daba con el botón de
la barra izquierda (fallaba «no se cerró» aunque se cerraba): ahora `div[aria-label=…]`.

Pendiente de F1 para más adelante: la vista se puede girar sobre la hoja (no molesta). Las cotas
de lo dibujado ya se exportan desde F2 (`sheetSketchDims`), con lo que falta anotado en la tabla
de cobertura («Hoja»).

### Varios planos en el Outliner ✅

*Hecho el 2026-10-10 (pedido del usuario: los detalles en la barra de arriba «es feísimo»).*
`doc.drawing = { sheets: DrawingSheet[], next_id }` (`lib/drawingDoc.ts`; la forma vieja de un
solo plano se lee como «Plano 1» y se reescribe al primer cambio; el backend no cambió: es JSON
libre). Cada plano guarda hoja, proyección, escala, ocultas/tangentes, cotas generales, vistas
ocultas, corte, detalles, cotas a mano, dibujo y cajetín (antes eran señales de la interfaz y
se perdían). Grupo «Planos 2D» en el Outliner con «+» (`SceneNode.addHint`, `onAddNode`); de
cada plano cuelgan Hoja y cajetín, las cuatro vistas, Cotas generales, Corte A-A, Detalles,
Cotas y Dibujo, con ojo (vistas, cotas generales, corte, detalles, cotas, dibujo) y borrar
(corte, detalles, cotas, dibujo, el plano). Clic en una fila o en la hoja (el elemento del SVG o
la caja de la vista) lo elige: se resalta en la hoja, se marca en el Outliner y abre una ventana
flotante arrastrable con sus propiedades; Esc la cierra y Supr borra lo elegido. La barra de
arriba quedó con las herramientas (Cota, Distancia, Ángulo, Detalle, Corte A-A, Dibujar, Hoja)
y exportar. Pruebas: node `drawingDoc.test.mjs` y e2e "planos 2D en el Outliner…".

### Seis vistas ✅

*Hecho el 2026-10-10 (el usuario: «el primer diedro tiene 6 vistas y acá se hacen solo 3 más
el 3d»).* `ViewName` = frente, superior, inferior, lateral izquierda y derecha, posterior e
isométrica; cada vista es la misma en los dos diedros (`viewSpec`, arriba de la hoja = eye ×
xdir como `gp_Ax2`) y `viewSlots` dice dónde va: primer diedro (ISO E) superior abajo, inferior
arriba, izquierda a la derecha y derecha a la izquierda; tercero (ISO A) al revés; la posterior
al final de la fila del frente. Al empezar se ven frente, superior, la lateral que va a la
derecha del frente e isométrica; las otras se prenden con su ojo. Los planos con la vieja
«side» la pasan a izquierda (primer diedro) o derecha (tercero), también en cotas y detalles.

### F2 — Las vistas como referencia ✅

*Hecha el 2026-10-10* (`lib/sheetRefs.ts`, pruebas node `sheetRefs.test.mjs`, e2e "plano 2D:
las vistas como referencia del dibujo…"). La sesión de la hoja lleva las vistas acomodadas
(`SheetEdit.views`) y si se dibujan las ocultas.
- Anclajes: las líneas de las vistas en mm de hoja (`viewPolylines`) entran a `solidRefs` como
  si fueran aristas del sólido: extremos, medios, centros, cruces y sobre la línea.
- Usar (J) sobre la hoja: la línea de vista más cercana entra como recta, círculo, arco o spline
  (`projectViewLine`) en `uses` (el solver la deja fija, sin backend nuevo) con su vínculo en
  `DrawingSheet.links` (`{entity, view, at}`). `applySheetLinks` la lleva a donde está hoy su
  línea (otra escala, otro lugar o el modelo cambió) al dibujar la hoja y al abrir el dibujo;
  si la línea ya no está, queda donde estaba y se avisa. Romper vínculo la deja suelta.
- Recortar y extender toman las líneas de las vistas como límites de afuera (`TrimLimits` en
  `trimLine`, `trimCurve`, `extendLine`, `trimByStroke`); el punto del corte queda fijo.
- Cotas del dibujo en la hoja, el PDF y el DXF (`sheetSketchDims`: largo, distancia,
  horizontal y vertical, radio, diámetro y ángulo; el valor es lo que mide la geometría). Las de
  una curva usada miden el modelo y lo siguen.
- Decisión: las cotas a mano del plano (`UserDim`) **no** se migran al dibujo: ya miden la
  vista y siguen al modelo; conviven con las del dibujo.
- Pendiente para F3: lo anclado a un vértice de una vista queda fijo en la hoja (no sigue a la
  vista si se corre ni al modelo); lo mismo con todo lo dibujado que no está ligado.

### Escala por vista ✅

*Hecha el 2026-10-10 (pedido del usuario).* `DrawingSheet.viewScales`; en la ventana de una
vista, «Escala» (la de la hoja o una normalizada). `layout` cuenta cada vista a su escala (la
automática de la hoja deja lugar a las ampliadas) y las de escala propia llevan su rótulo debajo
(«Posterior (2:1)», también en el DXF; en el frente cortado va en el rótulo del corte) y en su
fila del Outliner.

### F3 — Vistas libres y girar en 3D ✅

*Hecha el 2026-10-10* (pruebas node `drawing.test.mjs`, `drawingDoc.test.mjs`,
`sheetRefs.test.mjs`; e2e "plano 2D: mover vistas…" y "plano 2D: las escalas dicen la
verdad…").
- **Mover vistas** (pedido del usuario: «automáticamente está bien pero quiero poder moverlas a
  mi gusto»): arrastrar una vista la corre alineada con el frente (`dragAxis`: las de su fila en
  horizontal, las de su columna en vertical; frente e isométrica libres; Mayús o Alt, libre) y
  queda en `DrawingSheet.viewAt` (las demás siguen acomodándose solas). En su ventana: centro
  X/Y escrito y «Volver a su lugar»; en la barra, «Acomodar» (siempre presente, desactivado si no
  hay nada movido). Avisos si una movida se sale de la hoja, pisa el cajetín u otra vista
  (`placementProblems`).
- **Lo dibujado se mueve con su vista**: al volver al plano, cada curva que cae entera en una
  vista queda de ella (`DrawingSheet.owners`, `sheetOwners`) con el lugar y la escala de ese
  momento; si la vista se corre o cambia de escala, `applySheetOwners` la lleva (con sus fijos y
  sus cotas escaladas). Lo que no cae en ninguna queda de la hoja.
- **Girar en 3D** la isométrica y las **vistas nuevas** («Vista nueva»: `customViews`,
  `vista-<id>`, en una columna a la derecha con su nombre debajo; `viewDirs` guarda desde dónde
  mira cada una): en su ventana, giros de 90° (izquierda, derecha, atrás, adelante y en el plano),
  «Tomar la del visor» (la cámara de Diseñar, `viewDirFromCamera`) e «Isométrica»; en la hoja,
  Ctrl+arrastrar orbita con vista previa de las aristas del sólido (`projectPolylines`) y al
  soltar se pide la vista exacta. Las vistas 3D (`isPictorial`) no llevan ocultas, ni cotas
  generales ni a mano (lo que se ve está acortado).
- **Escala a la vista**: botón «Escala 1:5 (auto)» en la barra (el usuario no confiaba en las
  escalas: la automática baja con más vistas). Mensajes y avisos flotan sobre la hoja: antes
  empujaban la hoja y la achicaban en pantalla, parecía que las vistas cambiaban de tamaño.
  Al dibujar sobre la hoja no se ponen cotas solas (decisión: el dibujo anota; el usuario lo va a
  probar).
- Queda: vista auxiliar proyectada desde otra (perpendicular a una arista inclinada) y mover
  los detalles a mano.

### F4 — Cortes e imágenes de secciones ✅ (con pendientes)

*Hecha el 2026-10-10* (backend: `DrawingViewSpec.cut` = prismas que quitan material, unidos;
raya las caras sobre cualquiera de sus planos; `shaded` devuelve la malla; `cad_write_bytes`.
Front: `lib/drawingCuts.ts`, `lib/drawingShade.ts`; pruebas node `drawingCuts.test.mjs`,
`drawing.test.mjs`; e2e "plano 2D: cortes por línea y escalonados…").
- **Corte por una línea** (herramienta Corte de la barra izquierda: dos clics sobre una vista;
  más clics, **escalonado**; doble clic o Enter termina): `DrawingSheet.sections`, letra libre
  (`nextLabel`), línea de trazo y punto gruesa en puntas y quiebres con flechas y letras; su
  vista «Corte B-B» mira en el sentido de las flechas (`sectionViewDir`) y quita lo de atrás de
  la línea (`sectionPrism`). «Invertir el sentido» en su ventana.
- **Medio corte** de una vista (derecha o izquierda: quita el cuarto hacia quien mira,
  `halfPrism`) con la línea central.
- **Rotura** (herramienta Rotura: contorno de clics, doble clic o Enter): corte local hasta una
  profundidad (`breakPrism`), con su contorno fino; la profundidad en su ventana.
- **Vista 3D cortada** (isométrica y vistas nuevas): «Un cuarto» o con cualquier corte.
- **Imágenes**: «Sombreada» (con o sin líneas) en cualquier vista: la malla (entera o cortada,
  las caras cortadas en otro color) dibujada con three.js fuera de pantalla mirando desde la
  vista; va en la hoja, en el PDF (svg2pdf incrusta la imagen) y se guarda sola como PNG.
- **Rayado**: ángulo, separación y cruzado, de la hoja y de cada corte.
- Las vistas de más (nuevas y de cortes) van en el **lugar libre** de la hoja; con la escala
  automática, la mayor con la que también ellas entran (antes quedaban fuera de la hoja).
- De paso (pedido del usuario): la **barra izquierda** con el plano abierto muestra las
  herramientas del plano, no las de Diseñar.

Pendientes de F4 cerrados el mismo día: rayado según el material (`materialHatch`: acero de a
pares, latón alternado, aluminio apretado, plásticos cruzado, madera vertical) y **pieza por
pieza** en un conjunto (el backend corta cada pieza aparte: `hatch_part`; las vecinas con el
ángulo al revés; «Rayado según el material» en Hoja); DXF con las líneas de corte, letras,
central, roturas y el rayado como líneas (capas CORTES y RAYADO; `hatchSegments`); la vista del
corte **alineada** con la de donde sale, del lado que pide el diedro (si no hay lugar, en el
primer lugar libre con su rótulo); letras de cortes que no se pisan y rotura a mano alzada
(`breakOutline`).

### F5 — Lo que le falta al editor (sirve para sketch y hoja) — 🟡 (lo que falta, para más adelante)

Hecho el 2026-10-10 (pruebas node `sketchEdit.test.mjs`; e2e "sketch: barra de restricciones…"
y los 45 del sketch y el plano):
- **Barra de restricciones arriba** (pedido del usuario: estaban escondidas en el panel):
  Cota (inteligente: largo, diámetro, radio, distancia…), **Ángulo ∠** (Mayús: suplementario),
  Coincidente, Horizontal, Vertical, Paralelas, Perpendiculares, Tangente, Igual, Punto medio,
  Concéntricos, Simetría y Fijo (`lib/sketchSuggest.ts`: `constraintSuggestions`, salido del
  panel, y `CONSTRAINT_TOOLS`). Con lo elegido se ponen al apretar; sin nada, el botón espera y
  cada clic suma hasta completarla (sigue activo hasta Esc).
- Cota de ángulo (error del sketch): toma lo que mide y dibuja su arco entre las líneas.
- Redondeo y chaflán en **varias esquinas** (radios iguales vinculados), con **esquina
  virtual** (punto sobre las dos rectas; el largo de las líneas pasa a medir desde ella),
  redondeo **sin recortar** y chaflán con sus **dos distancias como cotas**.
- **Extender arcos** (siguen su círculo), **partir en N** partes iguales (líneas con el largo
  atado), **equidistante** con varias copias, original a construcción, distancia como cota y la
  **cadena entera con un clic**; simetría en los **ejes X/Y** y «quitar el original»; patrón
  lineal por **distancia total**.
- Clic derecho cancela como Esc; Enter termina lo que está a medias antes que el sketch;
  Ctrl+clic suma; Mayús alterna recortar/extender; aviso de conflicto con «quitar» por
  restricción; cursor por herramienta y rótulo de la herramienta activa; rueda en el campo de
  cota; el panel recuerda los últimos valores.
- De paso: seis e2e del sketch fallaban desde que las herramientas pasaron a la barra izquierda
  (buscaban «Línea», «Círculo», «Elegir»… por texto y agarraban otra cosa): ahora por atajo, y el
  respaldo de `clickText` encuentra los ítems de las listas de familias (llevan el atajo al lado).

Segundo lote (e2e "sketch edición III…"): **vista previa** de los botones del panel al pasar el
mouse (modo de prueba en `cadUi`: `previewOf` corre la acción sobre una copia y dibuja lo nuevo;
los botones llevan `data-preview`), **recortar en rojo** lo que se va (`trimRemoved`),
**arrastrar una curva entera** y el **radio** de un círculo (`dragEntity`), **elegir similares**,
**Tab** recorre lo encimado (`hitAll`), **copiar N veces**, **cerrar/abrir una spline** y
construcción igualada en varias (también puntos sueltos).

Tercer lote (e2e "sketch edición IV…", pruebas `sketchTrim.test.mjs`): **recortar splines y
elipses** sin cambiar la forma (`lib/sketchTrim.ts`: la elipse pasa a arco de elipse; la spline
queda por polos, partida insertando nudos; la por puntos conserva sus puntos como uniones), las
líneas, círculos y arcos **cortan contra splines y elipses** (punto en la curva), **extender una
spline** por puntos (sigue la tangente) y un **arco de elipse**, **límite elegido a mano**
(Ctrl+clic en recortar o extender: solo eso corta; Esc lo suelta) y **polos → puntos de paso**
(adaptativo, se aparta a lo sumo 0,5 %).

Cuarto lote (e2e "sketch edición V…", pruebas `sketchPatterns.test.mjs` y `sketchEdit.test.mjs`):
**patrones que se vuelven a editar** (`lib/sketchPatterns.ts`; quedan en `Sketch.patterns`, campo
libre en Rust): al elegir una copia aparece su patrón con «Editar el patrón» (carga cantidad y
paso en los campos y el botón pasa a «Rehacer el patrón»), «Omitir esta copia» (queda de
construcción: no se corta la cadena de restricciones y sigue omitida al rehacer), «Rehacer con
el original» (si se le agregó algo) y «Soltar el patrón»; **duplicados parciales** (líneas que se
pisan en parte y arcos del mismo círculo se juntan, con «Eliminar duplicados»), Reparar **junta
los puntos sueltos** con el punto que tengan encima, **lista navegable** de los problemas (clic:
elige y centra la vista) y **marcas de cruces** (× amarilla; «Cruces» en las opciones de mostrar).

Quinto lote (e2e "sketch edición VI…", pruebas `sketchFree`, `sketchTyped`, `sketchPick` y
`sketchEdit`): herramienta **Equidistante en el visor** (F: clic en la curva toma su cadena; el
**lado y la distancia salen del cursor**, con vista previa, o se escribe la distancia), el
equidistante **saca las piezas que quedan más cerca del original que la distancia** (una muesca
angosta ya no lo cruza consigo mismo; las vecinas se vuelven a unir), **campo flotante** al mover,
copiar, girar y escalar después del punto base (distancia hacia el cursor, `dx; dy`, `d<ángulo`,
grados o factor; escribir sin hacer clic), **«Soltar de lo que no se mueve»** (lo elegido se lleva
copias de los puntos compartidos y deja sus restricciones con lo demás; si no, como siempre, lo de
al lado se estira), **arrastre libre** con Ctrl+Mayús (suelta las restricciones de lo arrastrado) y
**filtro de selección** en la barra del sketch (todo, puntos, líneas, círculos, splines; vale
también al recortar, extender y partir) y **vista previa de Extender** (hasta dónde llega, antes
del clic).

F5 **no** quedó completa: la nota anterior decía que solo faltaban los manipuladores, pero al
revisar el código ítem por ítem (2026-10-10) quedan varios 🟡 y ❌. La tabla de abajo ya está al
día; lo que queda del editor se deja para más adelante (ver F6).

### F6 — La hoja

Decisión del usuario (2026-10-10): centrarse en la hoja impresa con las medidas del modelo 3D y
**no** sumar más cosas del sketch por ahora (lo 🟡/❌ del editor en la tabla de abajo queda para
el futuro, quizás en el plan de operaciones). Primero:

1. **Cotas exportadas donde se dejaron**: SVG/PDF/DXF respetan el lugar del texto que el usuario
   arrastró (`opts.offset`) en vez de correrlas siempre 7 mm; sin lugar elegido, hacia afuera del
   dibujo como en el editor.
2. **Textos y cotas que no se pisan**: las cotas (generales, a mano y del dibujo) y los textos de
   la hoja se separan solos cuando se encimarían.
3. **Medida al dibujar sobre la hoja**: al terminar una forma aparece el campo de la cota como en
   el sketch; si se escribe un valor queda como cota de la hoja, si no (Esc o seguir dibujando)
   no se agrega nada.

Hechos los tres (2026-10-10; pruebas `drawing.test.mjs`, `sheetRefs.test.mjs` y e2e "plano 2D:
medida al dibujar sobre la hoja…"):
- `sheetSketchDims` toma el lugar del texto (`placed`): las lineales pasan por él (`along` corre
  el texto y alarga la línea de cota), el radio alarga su línea hasta él (`tail`), el diámetro va
  en su dirección y el ángulo agranda el arco. Sin lugar elegido, hacia afuera del centro de lo
  dibujado. SVG y DXF ponen el texto en el mismo lugar (`dimLabelBox`).
- `sheetDimensions` junta las generales, las de las vistas y las del dibujo y `spreadDimensions`
  corre las automáticas cuyo texto pisaría otro o un texto dibujado (`sheetTextBoxes`): lineales
  de a 6 mm o el texto a un costado, radio más largo, arco más grande. Las puestas a mano no se
  mueven. No esquiva todavía las líneas de las vistas ni las líneas de otras cotas (solo textos).
- Sobre la hoja `dim()` deja la cota pendiente (índices -2, -3…, `sheetPending` en CadView): el
  campo aparece como en el sketch; escribir un valor (Enter, Tab o clic afuera) o Enter sin
  escribir la agrega; Esc, Tab sin escribir o seguir dibujando, no.

Después, más del plano (a revisar con el usuario).

Lo que entró al sketch después de F5 y sirve también en la hoja (commits 9d6c9af, 03b5ac1 y
d08fea9): ángulos con referencia y sentido, avisos en un desplegable, aviso de cota que
sobre-define (Esc, aceptar y volver a verlo), elegir entre lo encimado y ángulos fijos al dibujar
líneas. De la hoja de engranes (118e678): los círculos da / df / db salen rotulados en las vistas
que miran a lo largo del eje.

## Cobertura de la lista (editor del sketch, que es el de la hoja)

Revisada en el código el 2026-10-10, **después de F5** (`C` = `components/layout/CadView.tsx`,
`D` = `components/steps/DesignStep.tsx`, `U` = `lib/cadUi.ts`).

**Comportamiento general** — ✅ herramienta activa hasta Esc (salvo pegar y calibrar), atajos,
deshacer por acción, conserva y crea restricciones, colores al instante, sin retardo, aviso de
conflicto con «quitar» por restricción, clic derecho cancela, Enter termina la operación antes
que el sketch, cursor por herramienta, rótulo de la herramienta activa en el visor, vista previa
de los botones del panel (al pasar el mouse). 🟡 resaltado bajo el cursor (elegir, recortar,
extender, partir, círculo tangente, punto en spline y el anclaje al dibujar; no en el primer clic
de Equidistante, Usar, arco tangente, Estirar ni mover/copiar/girar/escalar); elegir primero o
herramienta primero (la barra de restricciones acepta las dos; recortar y extender borran la
selección al entrar, partir solo con clic —«Partir en N» del panel sí usa lo elegido—, y
mover/copiar/girar/escalar exigen elegir antes); mensaje del paso (texto fijo por herramienta en
`designHints.ts`; solo Equidistante, círculo tangente, Estirar, Ctrl+clic al recortar y la
restricción que espera dicen el paso; las de dibujo —arco, elipse, ranura…— no).

**Entrada de valores** — ✅ escribir sin clic (cota recién dibujada y campo flotante), Tab entre
las cotas pedidas, rueda en la cota y en el panel. 🟡 campo flotante (cota al terminar una forma
—sobre la hoja queda pendiente en `sheetPending`, F6—, mover/copiar/girar/escalar y Equidistante; no en redondeo,
chaflán ni patrones); expresiones, parámetros y unidades (en cotas sí; el panel usa `Num`
numérico, y las coordenadas y el campo flotante solo aceptan números); recordar el último valor
(entre sesiones radio, partes, veces, copias y distancia del equidistante, chaflán y patrón; solo
en memoria mover/girar/escalar, modo y segunda medida del chaflán, «a los dos lados» y extremos
del equidistante, tipo de patrón, filas y paso 2, tolerancia de Reparar); rueda (no en el campo
flotante de mover/girar/escalar/equidistante). ❌ ajustar arrastrando manipuladores (`handleOf`
no da flechas con un sketch abierto; solo existen para extrusión, agujero y chapa).

**Recortar** — ✅ clic, arrastre, hasta el cruce más cercano, sin cruces borra entera,
coincidencias en los extremos nuevos, punto sobre curva en el corte, vista previa en rojo del
tramo que se va, splines y elipses sin cambiar la forma, contra curvas usadas (también splines y
elipses) y contra las líneas de las vistas en la hoja, límite elegido a mano con Ctrl+clic (la
construcción corta solo si se elige así).

**Extender** — ✅ hasta lo más cercano, clic cerca del extremo, vista previa, límite elegido,
arcos, splines por puntos, arcos de elipse, Mayús alterna con recortar.

**Dividir y unir** — ✅ partir líneas, arcos y círculos con clic, unir alineadas o del mismo
círculo, conserva restricciones, en N partes iguales (líneas y arcos). 🟡 en intersecciones (en
bloque solo dentro de Reparar y solo entre líneas, arcos y círculos). ❌ partir splines y
elipses (`splitEntityAt`: «Solo se parten líneas, arcos y círculos»).

**Desfase** — ✅ a los dos lados, extremos redondos o rectos, cadena con un clic, lado y
distancia según el cursor, original a construcción, distancia como cota, varias copias. 🟡
esquinas (en punta, redondea sola si no se cruzan; sin opción); autointersecciones (quita las
piezas más cerca del original que la distancia; no recorta cruces entre piezas no vecinas).

**Redondeo 2D** — ✅ tangencias, recorta las vecinas, deja la cota de radio, varias esquinas,
radios iguales vinculados, esquina virtual, sin recortar, vista previa (botón del panel). 🟡
radio por valor (sin campo flotante ni arrastre). ❌ elegir dos entidades (solo el punto común de
dos líneas), línea con arco, arco con arco.

**Chaflán 2D** — ✅ distancia igual, dos distancias, distancia y ángulo, varias esquinas, esquina
virtual, cotas editables, vista previa (botón del panel). ❌ dos entidades sin punto común, línea
con arco.

**Simetría** — ✅ restricción automática, extremos sobre el eje compartidos, ejes X/Y del sketch,
mover («Quitar el original»). 🟡 dinámica al dibujar (se refleja al confirmar, no en la vista
previa).

**Patrones** — ✅ lineal en una o dos direcciones, circular, restricciones entre copias,
asociativo (se vuelve a editar y rehacer), omitir copias, lineal por distancia total. 🟡 circular
solo por ángulo total (sin paso entre copias); vista previa (al pasar el mouse por el botón; no
se actualiza al cambiar los campos).

**Mover, copiar, rotar, escalar** — ✅ punto base, desde-hasta, girar y escalar por valor o por
referencia, anclajes, fantasma, campo flotante, copiar N veces, soltar de lo que no se mueve
(romper restricciones externas). ❌ mover arrastrando varias cosas elegidas (el arrastre es de un
punto o una entidad).

**Estirar** — ✅ ventana de cruce, solo los puntos de adentro, conexiones.

**Arrastre directo** — ✅ extremos, puntos y manijas de spline, curva entera, radio del círculo,
solver en vivo, flechas de lo libre, arrastre libre con Ctrl+Mayús. ❌ aviso al arrastrar lo
totalmente definido (vuelve en silencio).

**Splines** — ✅ quitar punto, mover, peine en vivo, cerrar o abrir una ya dibujada, paso → polos
y polos → paso. 🟡 agregar (no en por polos cerrada: «todavía no se agregan polos»); manijas
(solo dirección: el largo se ignora, sin curvatura); simplificar (no por polos cerrada ni
racional).

**Construcción** — ✅ Q y botón, punteada, varias a la vez iguala (también puntos sueltos).

**Eliminación y limpieza** — ✅ sin restricciones huérfanas, solo restricciones o solo cotas,
duplicados exactos y parciales, informe con lista navegable y cruces. 🟡 fusionar puntos (solo
dentro de Reparar: extremos y puntos sueltos sobre un punto de curva; sin «juntar los
elegidos»). ❌ cerrar huecos con distancia máxima (`closeContour` no la tiene); quitar las muy
cortas fuera de Reparar.

**Selección** — ✅ clic, ventana, cruce (y lazo con Ctrl), cadena con doble clic, por tipo,
Ctrl y Mayús suman o quitan, similares, Tab entre encimadas, filtro durante la herramienta. ❌
clic sin modificador sobre lo elegido no lo quita.

**Retroalimentación visual** — ✅ símbolos de restricciones, contornos cerrados, extremos
sueltos, colores, marcas de cruces, rótulo de la herramienta activa (solo el nombre, sin el
paso).

**Hoja (fuera de la lista)** — ✅ las cotas de lo dibujado salen en SVG/PDF/DXF (`sheetSketchDims`).
Desde F6 respetan dónde dejó la cota el usuario (`opts.offset`) y la medida se pide al terminar la forma (ver F6). 🟡 solo largo, distancia, H/V, radio, diámetro y ángulo; letras encimadas (solo se separan las de cortes
en el mismo lugar; las cotas y textos que se pisan se separan solos desde F6, pero no esquivan las
líneas de las vistas).

## Pruebas

- Funciones puras en node (`e2e/drawing.test.mjs` y las del sketch) para cada cosa nueva.
- e2e dirigido por fase: solo lo nuevo y lo que toca (abrir el plano, dibujar, exportar); la
  suite completa al cerrar el plan.
- Salida: SVG/PDF/DXF revisados abriéndolos (f3d o el lector de DXF del sketch, que ya existe).
