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
hoja y la anota: frente, planta, lateral e isométrica con líneas ocultas exactas (HLR), primer o
tercer diedro, escala normalizada, cajetín, cotas generales, cotas a mano (largo, Ø, radio,
distancia entre paralelas, ángulo; recordadas por un punto y que siguen al modelo), detalles,
corte A-A paralelo al frente con posición movible, y salida SVG / PDF / DXF.

**No tiene entidades 2D propias**: todo sale de proyectar el sólido. Por eso casi toda la lista
falta (no hay qué recortar, desfasar ni restringir), no hay selección, ni Esc, ni atajos, ni
arrastre. Las vistas tienen lugar fijo (`layout`) y la isométrica, orientación fija
(`viewSpec("iso")`). El backend ya acepta cualquier dirección de vista (`eye`, `xdir`) y
cualquier plano de corte (`DrawingSection { origin, normal }`): girar vistas y cortar por
otros planos no necesita C++ nuevo.

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
hoja de fondo es el SVG del plano sin el papel, rasterizado (hasta 4096 px, invertido en el
tema oscuro) y puesto como calco que no se guarda. Con la hoja abierta el visor oculta el
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

Pendiente de F1 para más adelante: las cotas de lo dibujado no se exportan (son del sketch; con
F2 las de referencia pasan a la hoja) y la vista se puede girar sobre la hoja (no molesta).

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

### F5 — Lo que le falta al editor (sirve para sketch y hoja) — en curso

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

Queda de F5: campo flotante en las herramientas de edición, límite de recorte elegido a mano,
recortar y extender splines, patrón asociativo y omitir copias, filtros de selección dentro de la
herramienta, arrastre sin restricciones, mantener o romper restricciones al transformar,
polos → puntos de paso, limpieza (duplicados parciales, fusionar puntos sueltos, informe
navegable), marcas de cruces.

Sale de la tabla de cobertura de abajo: todo lo que está 🟡 o ❌. Orden propuesto: primero lo
que cambia el uso diario (vista previa y campo flotante en las herramientas del panel, clic
derecho, extender arcos, redondeo y chaflán con esquina virtual y en varias esquinas, patrón
asociativo y omitir copias), después lo demás.

## Cobertura de la lista (editor del sketch, que es el de la hoja)

Revisada en el código el 2026-10-10. Ojo: equidistante, redondeo, chaflán, simetría, patrones y
transformar por números son **botones del panel** (sin vista previa, campos `type=number`); en
el visor solo hay recortar, extender, partir, estirar, mover, copiar, girar y escalar.

**Comportamiento general** — ✅ herramienta activa hasta Esc (salvo pegar y calibrar), atajos,
deshacer por acción, conserva restricciones, crea restricciones, colores al instante, sin
retardo, aviso de conflicto. 🟡 vista previa (no en las del panel); resaltado bajo el cursor
(solo elegir, recortar, extender, partir, círculo tangente, punto en spline); Esc sí, clic
derecho no cancela (mueve la vista); Enter termina el sketch entero, no la operación; elegir
primero o herramienta primero (mover/copiar/girar/escalar piden elegir antes; recortar,
extender y partir solo herramienta primero); cursor (cruz o flecha, no por herramienta);
mensaje del paso (texto fijo por herramienta); quitar en conflicto desde el aviso (solo la
última cota).

**Entrada de valores** — 🟡 campo flotante (al terminar una forma, junto a la cota; no en vivo
ni en las de edición); escribir sin clic y Tab (solo cotas recién dibujadas); expresiones,
parámetros y unidades (en cotas sí; en el panel y las coordenadas no); recordar el último valor
(en memoria, no entre sesiones); rueda (panel sí, campo flotante no). ❌ ajustar arrastrando
manipuladores.

**Recortar** — ✅ clic, arrastre, hasta el cruce más cercano, sin cruces borra entera,
coincidencias en los extremos nuevos, punto sobre curva en el corte. 🟡 vista previa (resalta la
entidad entera, no el tramo); aristas proyectadas como límite (solo líneas, círculos y arcos que
no son de construcción). ❌ límite elegido a mano; recortar splines y elipses.

**Extender** — ✅ hasta lo más cercano, clic cerca del extremo. ❌ vista previa, límite elegido,
arcos, splines, alternar con recortar por tecla.

**Dividir y unir** — ✅ partir con clic (no splines), unir alineadas o del mismo círculo,
conserva restricciones (las cotas de largo se quitan). 🟡 en intersecciones (de a una; en bloque
solo con Reparar). ❌ en N partes iguales.

**Desfase** — ✅ a los dos lados, extremos redondos o rectos. 🟡 cadena (doble clic + botón);
distancia (valor, no arrastre); esquinas (en punta, redondea sola si no se cruzan; sin
opción). ❌ lado según el cursor (va por el signo), original a construcción, distancia como
cota, recorte de autointersecciones, varias copias.

**Redondeo 2D** — ✅ tangencias, recorta las vecinas, deja la cota de radio. 🟡 se elige el
punto común de dos líneas (no dos entidades, no línea con arco); radio por valor (no
arrastre). ❌ varias esquinas, vista previa, esquina virtual, radios iguales vinculados, no
recortar.

**Chaflán 2D** — ✅ distancia igual, dos distancias, distancia y ángulo. ❌ varias esquinas,
vista previa, esquina virtual, cotas editables (no deja restricciones y quita las cotas de
largo de las dos líneas).

**Simetría** — ✅ restricción automática, extremos sobre el eje compartidos. 🟡 línea de simetría
(cualquier línea, la primera elegida; los ejes X/Y del sketch no son entidades); solo copia (no
mover); dinámica al dibujar (se refleja al confirmar, no en la vista previa).

**Patrones** — ✅ lineal en una o dos direcciones, circular, restricciones entre copias. 🟡
cantidad y espaciado (después solo el espaciado, con la cota del primer par); espaciado (lineal
solo entre copias, circular solo ángulo total). ❌ omitir copias, vista previa, patrón asociativo
(son copias sueltas con `equal_offset`/`equal_rotation`).

**Mover, copiar, rotar, escalar** — ✅ punto base, desde-hasta, girar y escalar por valor o por
referencia, anclajes, fantasma. 🟡 mover (clic-clic o coordenadas; no arrastrando lo elegido);
copias múltiples (una por clic). ❌ elegir mantener o romper restricciones externas.

**Estirar** — ✅ ventana de cruce, solo los puntos de adentro, conexiones.

**Arrastre directo** — ✅ extremos, puntos y manijas de spline, solver en vivo, flechas de lo
libre. 🟡 solo puntos (no una línea o curva entera); centro sí, radio del círculo no; bloqueo de
lo definido (vuelve solo, sin aviso). ❌ arrastre sin restricciones con una tecla.

**Splines** — ✅ quitar punto, mover, peine en vivo (con cada respuesta del solver). 🟡 agregar
(no en por polos cerrada); manijas (solo tangencia: el largo se ignora, sin curvatura); paso →
polos sí, polos → paso no; simplificar (no por polos cerrada ni racional). ❌ cerrar o abrir una
spline ya dibujada.

**Construcción** — ✅ Q y botón, punteada. 🟡 varias a la vez (invierte cada una, no iguala; un
punto suelto no cambia).

**Eliminación y limpieza** — ✅ sin restricciones huérfanas, solo restricciones o solo cotas,
cerrar huecos (Cerrar contorno no tiene distancia máxima). 🟡 duplicados (solo exactos), muy
cortas y fusionar puntos (dentro de Reparar; solo extremos de curvas abiertas); informe
(resumen y marcas, sin lista navegable; los cruces no se cuentan).

**Selección** — ✅ clic, ventana, cruce (y lazo con Ctrl), cadena con doble clic, por tipo. 🟡
agregar y quitar (Mayús sí; Ctrl no; clic sobre lo elegido no lo quita). ❌ similares, ciclar
entre encimadas, filtros durante la herramienta.

**Retroalimentación visual** — ✅ símbolos de restricciones, contornos cerrados, extremos
sueltos, colores. 🟡 marcas de cruces (solo el anclaje bajo el cursor); herramienta activa
(botón y pista, sin rótulo en el visor).

## Pruebas

- Funciones puras en node (`e2e/drawing.test.mjs` y las del sketch) para cada cosa nueva.
- e2e dirigido por fase: solo lo nuevo y lo que toca (abrir el plano, dibujar, exportar); la
  suite completa al cerrar el plan.
- Salida: SVG/PDF/DXF revisados abriéndolos (f3d o el lector de DXF del sketch, que ya existe).
