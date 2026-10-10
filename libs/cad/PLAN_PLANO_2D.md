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

### F2 — Las vistas como referencia

- Cada línea de vista entra al sketch de la hoja como entidad fija ligada
  (`SketchUse` con `source: view { name, ref }`, recordada por un punto como las cotas a mano;
  se vuelve a buscar al cambiar el modelo y avisa si se perdió).
- Anclajes a vértices, medios, centros e intersecciones de las vistas.
- Recortar y extender contra aristas de las vistas (límite proyectado).
- Las cotas a mano actuales (`UserDim`) pasan a ser cotas del sketch de la hoja ligadas a
  la vista (de referencia: miden, no mandan). Migración de los documentos que las tienen.

### F3 — Vistas libres y girar en 3D

- Mover las vistas arrastrándolas (alineadas con la vista madre, como se proyectan; Mayús o
  Alt para soltarlas), agregar y quitar vistas, vista proyectada desde otra.
- **Girar en 3D las vistas que se ven en 3D** (isométrica y las que se agreguen): con la vista
  elegida, arrastrar orbita la pieza dentro de su marco (misma lógica que `navDrag`/`cameraRig`),
  más botones de 90°, ángulos escritos y «tomar la vista del visor». Se guarda `eye` + `xdir`
  por vista. Cada giro vuelve a pedir HLR (vista previa con la malla mientras se arrastra).
- Lo dibujado y las cotas sobre una vista se mueven con ella (pertenecen a la vista).
- Escala por vista (como los detalles).

### F4 — Cortes e imágenes de secciones

- Cortes por cualquier plano: dibujar la línea de corte sobre una vista (B-B, C-C…), con
  flechas y letra; corte escalonado (línea quebrada), medio corte y corte parcial (rotura).
- Corte en una vista 3D: la isométrica (o cualquier girada en F3) cortada, con la cara cortada
  rayada o pintada.
- **Imágenes de secciones**: vista sombreada (render del visor con la orientación de la vista,
  a la resolución de la hoja) de la pieza entera o cortada, puesta en la hoja como imagen con
  marco, y exportable sola a PNG. En el PDF va incrustada; en el DXF, solo el contorno.
- Rayado según el material (patrón y ángulo por pieza en ensambles).

### F5 — Lo que le falta al editor (sirve para sketch y hoja)

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
