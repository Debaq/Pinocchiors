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

## Fase 8: pieles y texturas imprimibles (hecha)

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

Estado (2026-10-13): **fases 1 a 6 hechas** (zonas, imágenes, desplazamiento, parámetros en mm, revisión antes y después de aplicar, vista previa).

Fases:

1. **Zonas** ✅: elegir caras (pincel / por clic y crecer por ángulo, como el pincel de pesos) y guardarlas en el
   proyecto como máscara por cara. Las caras de corte y de uniones se excluyen solas.
2. **Imagen** ✅: biblioteca de fábrica (`texture.rs`: patrones procedurales de altura, sin costuras, 256–512 px) e
   importar; muestreo bilineal con repetición.
3. **Desplazamiento** ✅ (`skin.rs`): subdividir solo las caras de la zona hasta que el lado sea ≤ la mitad del píxel en mm;
   altura muestreada en el UV; se desplaza a lo largo de la normal suave del vértice con `amplitud` (mm) y `desfase`
   (hacia afuera / centrado / hacia adentro). Los vértices del borde de la zona quedan fijos y la altura se funde
   hacia ahí (para no abrir grietas). Los vértices que comparten posición pero no UV (costuras) toman la misma altura,
   para que la malla siga cerrada.
4. **Escala en mm** ✅: tamaño de la baldosa en mm (la relación UV→mm sale del área de la zona), rotación e inversión.
5. **Revisión** ✅ (`skin_estimate` avisa antes de aplicar de relieve menor que una capa, mayor que 2 mm, detalle bajo la boquilla y malla muy pesada; después de aplicar se vuelve a correr `check_printability` y los avisos salen en «¿Se puede imprimir?»; no se decima): el límite físico lo da la boquilla (relieve útil de 0,2 a 1 mm; detalle mínimo ≈ una boquilla). Avisar
   si la imagen pide más detalle del que se imprime, si aparecen voladizos o paredes finas (se vuelve a pasar
   `check_printability`), y cuántos triángulos salen (decimar al final si pasa de un tope).
6. **Interfaz**: panel «Texturas imprimibles» con zona, imagen, tamaño, amplitud, desfase, rotación y una vista previa
   (sombreado con la altura) antes de generar la geometría. Hecho: zona (elegir en el visor con clic o recuadro, crecer por
   ángulo, invertir, quitar), biblioteca con miniaturas, importar, invertir / contraste / suavizar. Además: baldosa en mm, altura, giro, 
   «crece» (hacia afuera / centrado / hacia adentro), estimación de triángulos y «Aplicar piel a la zona». La vista
   previa («Ver el relieve») arma el resultado con menos detalle (hasta 250 000 triángulos) y lo muestra en lugar del
   modelo sin cambiarlo.

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

Cómo se aplica (decisiones de la fase 3): se unen los vértices por posición y las aristas largas de la zona se parten
por bisección, arrastrando a los vecinos (la malla sigue cerrada); el lado objetivo sale del detalle del mapa
(`baldosa / píxeles`, sin bajar de 0,2 mm). Cada vértice toma la altura del mapa por UV (la baldosa mide `tile_mm`; la
relación UV→mm es el promedio de la zona) y se mueve por su normal suave, con el borde de la zona fijo y el relieve
fundido en tres alturas. Los vértices de una costura de UV toman la misma altura (así no se abre). El resultado
reemplaza la malla del modelo y la escena queda como geometría sola; la copia para deshacer es la del historial.
Límite conocido: una sola escala de UV para toda la zona (si las islas tienen estiramientos distintos, la baldosa
cambia de tamaño entre ellas).

Cortar antes o después (resuelto): se texturiza el modelo entero y se corta después. Medido: cortar una malla texturizada
de 33 000 triángulos tarda ~17 ms (release), las piezas salen cerradas y las tapas de corte planas; así no hace falta
que el cortador lleve UV ni máscara. De paso se arregló el cortador con planos que pasan justo por vértices de la malla
(dejaba piezas abiertas): ahora esos vértices cuentan como del lado de adelante.

## Pendientes y límites (2026-10-13)

Leyenda: ⬜ falta · 🧪 falta probarlo de verdad · ⚠ límite conocido (funciona así a propósito).

### Sin probar

- 🧪 Nada de esto se ha probado en la app abierta ni con una impresora: solo pruebas de Rust (lib y escritorio),
  `tsc` y `vite build`. Falta recorrer con un modelo real: dividir, uniones, vaciar con drenaje, exportar placas,
  elegir zona y ver el relieve, y el asistente de calibración.
- 🧪 Sin e2e de la sincronía impresora ↔ diseño, de la selección de zona ni del asistente de calibración.
- 🧪 Los modelos de calibración y las uniones (clavija, cuarto de vuelta, riel) son geometría correcta y cerrada,
  pero no se ha verificado cómo los corta un slicer ni cómo ajustan impresos. Los presets de holgura y de
  contracción son valores típicos, no medidos.
- 🧪 El sombreado del relieve en el visor no se vio con una malla real (solo se probó que el dato sale bien).

### Verificación, orientación y soporte

- ⚠ La pared fina se mide con rayos desde una muestra de 1 500 caras; puede perder una zona chica.
- ⚠ El soporte estimado son columnas verticales hasta lo primero que hay debajo; no simula soporte en árbol ni
  ángulos de voladizo del slicer. Con muchas caras se muestrea.
- ⚠ La orientación sugerida es para el modelo entero. Al exportar las placas cada pieza se orienta sola, pero no
  se ve antes. ⬜ Orientación por pieza visible antes de exportar.
- ⬜ `SubdivideStrategy::Optimal` («menos cortes») sigue usando la grilla.
- ⬜ Cortes manuales (planos elegidos por el usuario), vista de los planos y las piezas en el visor, y candado de
  proporción por eje al escalar.

### Uniones

- ⚠ Las uniones solo van en caras de corte **planas**. La huella es redonda, cuadrada, trapecio, cuarto de vuelta
  o riel deslizante; el riel corre en línea recta de pared a pared y la cuerda tiene que caber en la cara sin huecos.
- ⬜ El riel no tiene tope (un extremo ciego): se desliza por los dos lados. ⬜ No hay puzzles ni terraza.
- ⚠ Si la cara de corte tiene un hueco en el medio (un anillo), el riel no se pone (lo avisa).

### Vaciado, material y placas

- ⚠ El vaciado deja un hueco cerrado (partes más finas que el doble de la pared quedan macizas). El drenaje pide
  una cara plana abajo, es un solo agujero y no hay venteo. ⬜ Venteo y drenaje sin cara plana.
- ⚠ La estimación de material no cuenta perímetros ni capas superiores: es una cota aproximada.
- ⚠ La distribución por contorno va por celdas (la separación real es la pedida más, como mucho, una celda) y
  solo gira de 90 en 90°.

### Tolerancias y Diseñar

- ⚠ La compensación de agujeros solo se puede hacer en el CAD (la cara cilíndrica es conocida); en una malla suelta
  solo se aplica la contracción.
- ⚠ Las roscas exteriores (tornillos y ejes) no llevan holgura: la lleva el hueco, para que el ajuste no quede el
  doble de flojo. Los engranes no compensan los dientes (eso es la «holgura según la impresora»).
- ⚠ La contracción se aplica desde el origen: en un archivo con varias piezas la distancia entre ellas crece lo
  mismo (0,3 % en PLA).
- ⬜ Contracción distinta por eje solo está en el modelo y el asistente; el material de la interfaz admite los tres
  ejes pero los presets los traen iguales.
- 🧪 Dos pruebas de `cad-model` fallan desde antes (`gear_part::avisos_y_errores` y
  `corona_helicoidal_y_cremallera`): esperan error con cremallera helicoidal, que el commit `80f7a83` permite.

### Calibración (asistente)

- ⚠ Asume que el bloque y la placa se imprimieron sin ninguna compensación; con la compensación activada los
  números salen mal.
- ⚠ Los números de la placa son de siete segmentos en relieve (sin tipografía); los agujeros del bloque (Ø4, 6, 8 y
  10) y su tamaño (53 × 30 × 20) son fijos, no se editan desde la interfaz.
- ⚠ La holgura se guarda en la impresora como `holgura / factor del material`; si cambias el factor después, el
  ajuste normal cambia.

### Pieles y texturas imprimibles

- ⚠ Una sola escala de UV por zona: si las islas están estiradas distinto, la baldosa cambia de tamaño entre ellas.
- ⚠ Aplicar la piel deja el modelo como geometría sola (sin UV, texturas ni rig) y borra la zona. Se deshace con el
  historial; si el diseño cambia y se rehace, la piel no se vuelve a poner.
- ⚠ Necesita UV: sin ellos el botón se desactiva y pide desplegar el modelo. Solo usa la altura (grises); el color
  de la imagen se ignora. Las imágenes importadas se guardan en 8 bits y hasta 1 024 px; los patrones de fábrica
  son de 256 px.
- ⚠ La vista previa tiene menos detalle (hasta 250 000 triángulos) y se calcula sobre el modelo entero.
- ⬜ Mapeo triplanar o en caja (sin UV), máscara de borde suave pintada, y decimar la malla después de aplicar.
- ⬜ Comprobar que la piel no invade uniones, agujeros ni roscas: hoy depende de que la zona elegida no los toque.



## Fase 9: cortes (hecha)

- **Óptimo** (`cuts::optimal_planes`): el menor número de piezas, repartiendo el eje más largo del modelo contra el espacio
  más largo de la impresora (las piezas se pueden girar: si el modelo cabe girado, no se corta), y cada plano donde la sección
  es más chica (programación dinámica sobre áreas de sección calculadas por plano, con piezas de al menos un décimo del
  máximo y una pequeña preferencia por el reparto parejo).
- **A mano** (`SubdivideStrategy::Manual`, `CutPlane { axis, position, tilt_deg }`): planos perpendiculares a un eje, a tantos
  mm del borde del modelo, con inclinación (la normal gira hacia el eje que sigue: X→Y, Y→Z, Z→X, entre ±80°). Parte aunque
  el modelo ya quepa. En la interfaz: lista de planos con eje, distancia y giro; se dibujan en el visor mientras se eligen.
- **Piezas en el visor**: «Ver las piezas separadas» las muestra de colores y separadas del centro (control de separación).
- El cortador ya no se rompe con planos que pasan justo por vértices de la malla (ver fase 8).

## Fase 10: escala con candado de proporción (hecha)

- «Medida final (mm)» parte de la actual y se rehace cuando el modelo cambia. Con **proporción fija** (el candado, por defecto) cambiar
  una medida mueve las tres con el mismo factor y se escala uniforme; con **proporción libre** cada medida cambia sola y el modelo se
  estira (modo `axes` del backend: un factor por eje de la vista, Z arriba; malla y escena se escalan igual).
- «Reducir para que quepa en la cama» es el antiguo «Ajustar a medida»: solo reduce, al volumen de la impresora menos el margen.
- Límite: estirar con proporción libre un modelo con rig lo deja como geometría sola (igual que el escalado uniforme con esqueleto).

## Fase 11: placas y orientación por pieza, a la vista (hecha)

- **Orientación que cabe**: `suggest_orientations_in` / `orient_for_print_in` reciben el volumen de la impresora y ponen primero
  las orientaciones en que la pieza cabe (girándola sobre la cama); cada candidata trae sus medidas (`dims`) y `fits`. Al armar las
  placas cada pieza usa esa elección, así una barra larga se acuesta si de pie no entra.
- **Cómo quedó cada pieza**: la vista previa de placas lista alto, base, voladizo y soporte estimado por pieza, y avisa las que
  no caben en la posición elegida.
- **Placa en 3D**: «Ver en 3D» arma la placa en el visor (cama gris, piezas de colores en su lugar, en milímetros).
- Límite: el chequeo de que cabe usa la caja de la pieza en el giro mínimo que la apoya; no prueba ángulos intermedios sobre la cama.
