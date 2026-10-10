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

### F1 — Capa de dibujo en la hoja

- Modelo: `doc.drawing.sketch` (un `Sketch` en mm de hoja, origen abajo a la izquierda).
  `doc.drawing` ya es JSON libre (`Document.drawing: Option<Value>`); pasa a tener tipo en Rust
  si hace falta validar.
- Editor: botón «Dibujar» en el plano abre el editor del sketch sobre un plano virtual con la
  hoja de fondo (SVG de las vistas como textura o líneas), cámara ortográfica normal y sin
  órbita. Se reutilizan la barra de herramientas del sketch, el panel y los atajos; se apagan
  las que no tienen sentido (usar del modelo, texto sobre cara, extruir).
- Las líneas de las vistas no se editan: son referencia (ver F2).
- Salida: lo dibujado va al SVG, PDF y DXF (capa propia en DXF; la construcción no se exporta).
- Deshacer y rehacer con las pilas del sketch (`undoSketch`) y, al cerrar, una entrada en el
  historial del documento.
- **Primer paso de la fase**: tabla de cobertura de la lista del usuario contra el editor del
  sketch, ítem por ítem, verificada en el código (la auditoría quedó a medias el 2026-10-10). Lo
  que falte va a F5.

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

### F5 — Lo que le falte al editor (sirve para sketch y hoja)

Sale de la tabla de cobertura de F1. Candidatos a confirmar en esa revisión (no verificados
todavía): campo flotante junto al cursor también en las herramientas de edición (desfase,
redondeo, patrones, mover), Tab entre campos y recordar el último valor por herramienta;
ciclar entre entidades encimadas y elegir similares; cursor distinto por herramienta;
quitar restricciones en conflicto desde el aviso; dividir en N partes; desfase con varias
copias, original a construcción y distancia como cota; redondeo en varias esquinas con esquina
virtual, radios iguales vinculados y «no recortar»; patrón asociativo editable con copias
omitidas; alternar recortar/extender con una tecla; extender splines; arrastre sin
restricciones con una tecla.

## Pruebas

- Funciones puras en node (`e2e/drawing.test.mjs` y las del sketch) para cada cosa nueva.
- e2e dirigido por fase: solo lo nuevo y lo que toca (abrir el plano, dibujar, exportar); la
  suite completa al cerrar el plan.
- Salida: SVG/PDF/DXF revisados abriéndolos (f3d o el lector de DXF del sketch, que ya existe).
