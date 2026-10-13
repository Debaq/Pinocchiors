# Plan: preparación para impresión 3D

Solo preparar el modelo (nada de gcode ni slicer). La lógica vive en
`pinocchio-print3d` con Z arriba y milímetros; el escritorio
(`apps/desktop/src/print_prep.rs`) traduce desde la escena (Y arriba).

| Fase | Contenido | Módulo | Estado |
|------|-----------|--------|--------|
| 1 | Verificación: estanca, manifold, normales, degenerados, cuerpos, voladizos, paredes finas | `printability` | hecha |
| 2 | Orientación: elegir la cara de apoyo que minimiza voladizos | `orientation` | hecha (se aplica con el piso de Preparar) |
| 3 | Perfiles de impresora y material; aviso de que no cabe | `profile` | hecha |
| 4 | Uniones reales: pernos y huecos en las caras de corte planas | `planar_joints`, `contact`, `capping` | hecha |
| 5 | Distribución en la cama (nesting) | `nesting` | hecha |
| 6 | Vaciado (cáscara por vóxeles) y estimación de material | `hollow` | hecha |
| 7 | Exportar placas STL/3MF/OBJ en mm | `print_prep` (escritorio) | hecha |

Además se rehízo el cierre de cortes (`capping`): antes solo cerraba un contorno
convexo y salían piezas abiertas; ahora admite varios contornos, huecos y
polígonos cóncavos. `find_neighbors` y `calculate_connections` miden contactos
planos reales entre piezas.

## Después de las 7 fases

- Marcas de pared fina en el visor; las uniones se rehacen cuando el diseño cambia.
- Impresora, material, uniones y placas se guardan en el proyecto; impresoras propias se recuerdan entre sesiones.
- Soporte estimado con rayos hacia la cama (las orientaciones se reordenan por él).
- Vista previa de la distribución en la cama.
- Agujero de drenaje al vaciar (túnel desde la cara plana de abajo hasta el hueco).
- Distribución por contorno en planta (`nest_contour`, por celdas).
- Cola de milano deslizante (`PlanarShape::Rail`, `groove::subtract_trench`): riel en una pieza y ranura en la otra, de pared a pared; la ranura se resta de cualquier pared (también curva) con un recorte de triángulos y cierre de las caras de la zanja.
- Cierre de cuarto de vuelta (`PlanarShape::Bayonet`): perno con cabeza de barra que entra por una ranura y se traba al girar 90°.

## Límites conocidos

- Las uniones solo van en caras de corte **planas** y dentro de la cara (redonda, cuadrada, trapecio
  cuarto de vuelta) y el riel deslizante, que corre una línea recta de pared a pared (la cuerda tiene que
  caber en la cara sin huecos). No hay puzzles. El riel no tiene tope: se desliza por los dos extremos.
- El drenaje pide una cara plana abajo (al exportar, tras apoyar la pieza) y un solo agujero; sin venteo.
- La verificación mide paredes con rayos desde una muestra de caras.

## Ideas siguientes

- Venteo (segundo agujero) y drenaje sin cara plana.
- Orientación por pieza visible antes de exportar.
- Tope en la ranura (un extremo ciego) y puzzles.

## Tolerancias de fabricación y su conexión con Diseñar (CAD)

Hecho (lado impresión): `FabTolerances::derive(impresora, material)` en `fab.rs` es la única fuente de
holguras y compensaciones: holgura de base (parte de la boquilla × factor del material), `Fit`
(justo / normal / libre), compensación de agujeros por lado, contracción por eje y pared mínima.
Las uniones toman su holgura de ahí (selector «Ajuste») y las placas se agrandan por la contracción
al exportar (el diseño queda en medidas nominales). Comando `get_fab_tolerances`.

Hecho (lado CAD, 2026-10-13):

1. `Document.fabrication` (impresora, material y ajuste), guardado en el `.pinocchio`; Fabricar y Diseñar
   comparten la misma elección (la de Fabricar manda; al abrir un diseño con impresora, esa pasa a Fabricar).
   Entra en la clave de la caché del recálculo.
2. `GearPart.auto_clearance` («Holgura según la impresora»: ajuste justo) y `Hole.compensate`
   («Compensar para imprimir»: el diámetro es el que se quiere medir impreso). Sin impresora avisan.
3. `assembly::tight_gaps` y «Revisar holguras» en el ensamble: pares que no se pisan pero quedan más cerca
   que la holgura de la impresora (se saltan los engranes que engranan).
4. Exportar desde Diseñar con «Compensar la contracción del material» (solo mallas; STEP queda exacto).

5. **Compensar los huecos** (`Fabrication.compensate_all`, casilla «Compensar los huecos del diseño» en Fabricar):
   agujeros (también caja y avellanado), roscas modeladas e interiores (holgura del ajuste normal), tuercas
   (con y sin rosca), arandelas, y eje, chavetero y aligeramientos de los engranes salen más anchos lo que la
   impresora los achica. Los externos (tornillos, ejes) no cambian: la holgura la lleva el hueco.
6. **Exportar compensado, también STEP**: se evalúa el diseño otra vez con todo compensado y cada pieza se
   agranda por eje con la contracción (transformación exacta del sólido, no de la malla). El diseño abierto no cambia.

Pendiente: pruebas e2e de la sincronía impresora ↔ diseño (hoy solo hay pruebas de Rust).

## Calibración con piezas de prueba (wizard)

`calibration.rs` genera dos modelos sin soportes y con los huecos verticales, y saca las holguras de lo medido:

- **Placa de holgura**: agujeros del diámetro de la clavija más una holgura creciente (numerados en relieve) y dos
  clavijas con base. Se elige el agujero donde entra justa → holgura de un ajuste normal; se guarda en la
  impresora como `fit_clearance = holgura / factor del material`.
- **Bloque de calibración** (53 × 30 × 20 mm con agujeros de Ø4, 6, 8 y 10; largo en X, fondo en Y, alto en Z):
  `contracción = 1 − medido / nominal` por eje y `hole_comp = promedio((nominal − medido) / 2)`.

Comandos `export_calibration_model` y `solve_print_calibration`; asistente `CalibrationWizard.tsx` (en Fabricar,
«Calibrar con piezas de prueba»). Los materiales propios (con contracción por eje y factor de holgura) y las
impresoras propias se recuerdan entre sesiones. Los resultados se pueden guardar como perfiles nuevos.

## Fase 8: pieles y texturas imprimibles (por hacer)

Idea (del usuario): aplicar a las piezas una imagen en blanco y negro como **mapa de altura** (blanco = relieve
alto, negro = bajo) que **desplaza la malla de verdad**, para que el patrón salga impreso: agarre, cuero, líneas,
rejillas, logos, braille.

Decisiones tomadas (2026-10-13):

- **Solo en zonas seleccionadas**, no en toda la pieza. Las uniones, las caras de corte, los agujeros y las roscas
  nunca llevan textura (ahí mandan las holguras).
- **Mapeo por UV**: la app ya tiene UV completo (`uv-core`), así que la textura pasa antes por esa sección (la
  pieza necesita UV; si no tiene, se le ofrece desplegarla). Triplanar y caja quedan para después, solo si hacen falta.
- **Imágenes de fábrica**, pocas y sin costuras (se generan por código, sin archivos): grano de cuero, líneas,
  rejilla, puntos, rombos, panal. Más las que el usuario importe (PNG/JPG a grises, con invertir, contraste y desenfoque).

Estado (2026-10-13): **fases 1 y 2 hechas** (zonas e imágenes, sin generar relieve todavía); faltan la 3 a la 6.

Fases:

1. **Zonas** ✅: elegir caras (pincel / por clic y crecer por ángulo, como el pincel de pesos) y guardarlas en el
   proyecto como máscara por cara. Las caras de corte y de uniones se excluyen solas.
2. **Imagen** ✅: biblioteca de fábrica (`texture.rs`: patrones procedurales de altura, sin costuras, 256–512 px) e
   importar; muestreo bilineal con repetición.
3. **Desplazamiento** (`skin.rs`): subdividir solo las caras de la zona hasta que el lado sea ≤ la mitad del píxel en mm;
   altura muestreada en el UV; se desplaza a lo largo de la normal suave del vértice con `amplitud` (mm) y `desfase`
   (hacia afuera / centrado / hacia adentro). Los vértices del borde de la zona quedan fijos y la altura se funde
   hacia ahí (para no abrir grietas). Los vértices que comparten posición pero no UV (costuras) toman la misma altura,
   para que la malla siga cerrada.
4. **Escala en mm**: tamaño de la baldosa en mm (la relación UV→mm sale del área de la zona), rotación e inversión.
5. **Revisión**: el límite físico lo da la boquilla (relieve útil de 0,2 a 1 mm; detalle mínimo ≈ una boquilla). Avisar
   si la imagen pide más detalle del que se imprime, si aparecen voladizos o paredes finas (se vuelve a pasar
   `check_printability`), y cuántos triángulos salen (decimar al final si pasa de un tope).
6. **Interfaz**: panel «Texturas imprimibles» con zona, imagen, tamaño, amplitud, desfase, rotación y una vista previa
   (sombreado con la altura) antes de generar la geometría. Hecho: zona (elegir en el visor con clic o recuadro, crecer por
   ángulo, invertir, quitar), biblioteca con miniaturas, importar, invertir / contraste / suavizar. Falta: tamaño, amplitud,
   desfase, rotación y la vista previa del relieve.

Dónde está: `texture.rs` y `zones.rs` (lib), `print_skin.rs` (comandos y estado; la zona y las imágenes van en el
.pinocchio y la zona viaja con su objeto), `SkinPanel.tsx` (interfaz). La elección de caras reutiliza la del visor
(las caras se numeran como las dibuja, que es el orden de la malla activa).

Preguntas abiertas:

- **Cortar antes o después**: el cortador (`slicer`) hoy solo conserva posiciones, así que la pieza cortada pierde los
  UV y la máscara. Opciones: (a) texturizar el modelo entero y cortar después (las tapas de corte salen planas, pero la
  malla es enorme al cortar) o (b) hacer que el cortador lleve UV y máscara por los cortes (más trabajo, mejor
  resultado). Recomendado: (b).
- **Amplitud y compensación**: el relieve debe quedar dentro de la medida de la pieza (desfase hacia adentro o centrado)
  para no romper ajustes; la compensación de contracción se aplica después, al exportar.
- **Texturas con color**: esta fase solo usa la altura (grises); el color de la imagen se ignora.
