# Sketch: lo que falta para el uso diario

Plan anotado el 2026-10-09 tras repasar el sketch contra la lista completa de un CAD de
referencia (Onshape, SolidWorks, Fusion). Reemplaza a los planes anteriores del sketch
(sketch completo y anclajes), ya cumplidos; lo que hicieron está en la bitácora de
[ROADMAP.md](ROADMAP.md).

## Qué tenemos hoy

- **Planos**: de origen, de referencia, caras planas; cambiar el plano de un sketch; origen
  propio (`Sketch.origin`); vista de frente y encuadre al entrar.
- **Entidades** (`model/src/sketch.rs`, `Geometry`): punto, línea, círculo, arco, spline
  interpolada (manijas en los extremos), elipse; texto como bloque (`Sketch.texts`).
  Herramientas: línea, línea desde el centro, rectángulo por esquinas y por centro, círculo por
  centro, arco por centro y por 3 puntos, arco tangente, elipse, polígono inscrito, ranura
  recta, spline, punto, texto, construcción (Q).
- **Edición**: recortar, extender, redondeo de esquina, equidistante, simetría, patrones
  (lineal en filas, circular con ángulo, en curva, por tabla, de relleno), borrar, arrastrar
  respetando las restricciones.
- **Referencias al sólido**: anclajes a vértices, medios, centros y aristas; «Usar arista» (J)
  ligada al sólido (`Sketch.uses`).
- **Restricciones** (`SketchConstraint`): coincidente, fijo, horizontal y vertical (de línea y
  entre puntos), paralela, perpendicular, igual, tangente, concéntrico, punto en línea, punto
  en círculo, punto medio, simétrico de dos puntos.
- **Cotas**: distancia, horizontal, vertical, largo, radio, diámetro, ángulo; de referencia;
  edición en sitio con Enter/Tab; fórmulas con los parámetros del documento.
- **Solver**: grados de libertad, sub/sobredefinido, color por entidad, conflictos en rojo,
  redundantes, aviso de cota que sobra con «dejarla de referencia».
- **Asistencia**: inferencias (extremos, medios, centros, cuadrantes, cruces, alineaciones,
  paralela, perpendicular, tangente), cotas al dibujar, vista previa, dibujo encadenado.
- **Selección**: clic, caja de ventana y de cruce, Mayús suma.
- **Regiones**: lazos cerrados, islas, elegir regiones para las operaciones.

## Qué falta y por qué

| Falta | Por qué importa |
|---|---|
| Deshacer dentro del sketch | Ctrl+Z hoy deshace el **documento** con el sketch abierto: no se puede volver atrás un trazo y, peor, el documento cambia por debajo del sketch en edición |
| Mover, copiar, rotar, escalar, dividir | Hoy solo se arrastra punto por punto; reacomodar un dibujo obliga a borrarlo y redibujarlo |
| Copiar y pegar entidades | Mismo motivo, y para llevar dibujo de un sketch a otro |
| Cotas punto-línea, entre paralelas, largo de arco, simétrica respecto de un eje | Las más usadas que no se pueden expresar; la simétrica es la forma natural de acotar perfiles de revolución (diámetros) |
| Colineal | Se arma hoy con paralela + punto en línea |
| Círculo por 2 y 3 puntos, tangente a 3; polígono circunscrito; ranuras en arco; rectángulo por 3 puntos | Herramientas de dibujo comunes que obligan a construir a mano |
| Proyectar contorno de cara, intersección con el modelo, geometría de otro sketch | «Usar arista» va de a una arista; dibujar sobre una cara existente es lo más habitual |
| Contornos abiertos, extremos sueltos, cruces y superposiciones | Una extrusión que no encuentra regiones no dice por qué |
| Mostrar u ocultar cotas, restricciones, construcción | Con sketches grandes la vista se llena de glifos |
| Lista de entidades con propiedades editables | No hay forma de escribir la coordenada o el largo de algo ya dibujado sin crear una cota |
| Selección por cadena y por tipo | Elegir un contorno entero es clic a clic |

## Diseño

### Deshacer en el sketch

- Pilas propias de la sesión en `cadUi`: `undo: Sketch[]`, `redo: Sketch[]` con el sketch
  **antes** de cada cambio. Cada `change` es un paso (cada clic de una línea encadenada es un
  paso, como en Onshape); un arrastre entero es un solo paso (se guarda al primer movimiento y
  se cierra al soltar). Los cambios que no cambian nada (una herramienta que avisa un error)
  no suman pasos.
- Deshacer repone el sketch guardado, limpia de la selección lo que ya no existe, corta la
  herramienta en curso y lo vuelve a resolver.
- Ctrl+Z / Ctrl+Mayús+Z con un sketch abierto van a estas pilas, no al documento. Terminar o
  descartar el sketch las vacía; deshacer en el documento después de terminar deshace el
  sketch entero (como hoy).
- Carrera: un resultado del solver que llega después de un deshacer no debe pisar el sketch
  repuesto → número de generación en la sesión; `solve` descarta respuestas viejas.

### Transformar entidades

- Herramientas en la barra del sketch: **Mover**, **Copiar**, **Rotar**, **Escalar**, sobre lo
  elegido. Mover/copiar: punto base y destino (con anclajes y cotas al dibujar: distancia y
  ángulo). Rotar: centro y ángulo. Escalar: punto base y factor.
- Mover, rotar y escalar cambian los puntos y quitan las restricciones que dejarían de
  cumplirse (fijos, cotas horizontales/verticales con puntos de afuera, coincidencias con
  puntos no elegidos: se desprenden). Copiar duplica entidades y las restricciones internas
  entre ellas (como `linearPattern` con una sola copia, sin igualdades con el original).
- **Dividir**: herramienta que parte una línea, arco o círculo en el punto del clic
  (`splitLineAt` ya existe para líneas; falta arco y círculo).
- **Copiar y pegar** (Ctrl+C / Ctrl+V): portapapeles de la sesión con las entidades, sus puntos y
  las restricciones internas; pegar pide el punto de inserción. Sirve entre sketches (el
  portapapeles sobrevive a cerrar el sketch).

### Cotas nuevas

| Cota | Ecuación / cómo |
|---|---|
| Punto-línea | `DistancePointLine { point, line, value }`: distancia con signo al soporte de la línea (solver: ya hay `PointOnLine`; se agrega la versión con valor) |
| Entre paralelas | Misma restricción con un extremo de la otra línea, y paralela si no lo eran |
| Largo de arco | `ArcLength { arc, value }`: r·θ con θ del barrido |
| Simétrica respecto de un eje | `Distance` × 2 sobre la perpendicular a la línea de construcción: se muestra como diámetro (`⌀`) y se usa para perfiles de revolución |

### Restricciones y herramientas nuevas

- `Collinear { a, b }`: paralelas + extremo de b en el soporte de a.
- Círculo por 2 puntos (diámetro), por 3 puntos (centro calculado), tangente a 3 entidades
  (cálculo inicial por Apolonio simplificado y tangencias en el solver).
- Polígono circunscrito: opción en la barra del polígono (inscrito/circunscrito); el
  circunscrito acota el círculo de construcción tangente a los lados.
- Ranura en arco (centro del arco, extremos, ancho) y ranura por el centro.
- Rectángulo por 3 puntos (inclinado): dos esquinas y el ancho, con paralelas y perpendiculares.

### Referencias al modelo

- **Contorno de cara**: elegir una cara plana del sólido y proyectar todas sus aristas
  (exterior e islas) como `uses`.
- **Intersección**: el plano del sketch corta el sólido (`BRepAlgoAPI_Section`) y las curvas
  quedan como `uses` ligados.
- **Otro sketch**: proyectar curvas de un sketch anterior (mismo mecanismo de `uses` con el
  origen en el sketch en vez del sólido).
- **Romper vínculo**: convierte un `use` en geometría propia.

### Validación del contorno

- Después de resolver: extremos que no se tocan con nada (resaltado), cruces de curvas sin
  punto común, entidades superpuestas. Se muestran en el visor y en la barra del sketch
  («3 extremos sueltos»).
- Al elegir el sketch en una extrusión o revolución sin regiones, el aviso dice por qué.

### Visualización y lista

- Botones en la barra del sketch: cotas, restricciones, construcción, puntos (mostrar/ocultar).
- Lista de entidades en el panel (tipo, largo/radio) con coordenadas y medidas editables: escribir
  un valor mueve los puntos sin agregar cota.
- Selección por cadena (doble clic elige todo lo conectado) y por tipo.

## Fases

1. **Deshacer y rehacer dentro del sketch.**
2. **Transformar**: mover, copiar, rotar, escalar, dividir; copiar y pegar.
3. **Cotas**: punto-línea, entre paralelas, largo de arco, simétrica respecto de un eje.
4. **Restricciones y herramientas**: colineal, círculo por 2/3 puntos y tangente a 3,
   polígono circunscrito, ranuras en arco y por centro, rectángulo por 3 puntos.
5. **Referencias al modelo**: contorno de cara, intersección, otro sketch, romper vínculo.
6. **Validación del contorno**: abiertos, extremos sueltos, cruces, superposiciones; aviso en
   las operaciones.
7. **Visualización y lista**: mostrar/ocultar, lista de entidades editable, selección por
   cadena y por tipo.

### Más adelante (sin fecha)

- Planos: dirección horizontal por arista, invertir normal, vista de corte y ocultar el modelo
  al entrar, sketch 3D, sketch sobre superficie curva.
- Entidades: línea infinita, paralelogramo, arco elíptico, parábola, cónica, spline por puntos de
  control y de ajuste, curvas por ecuación o desde archivo, texto sobre curva, imagen de
  calco, importar DXF.
- Edición: recortar con barrido, unir, equidistante a dos lados o con extremos cerrados,
  chaflán 2D, simetría dinámica, estirar, manijas intermedias de spline, simplificar y
  convertir a spline, eliminar duplicados, reparar, cerrar contorno.
- Restricciones: curvatura G2, coradial, simétrico de entidades, punto en intersección,
  bloquear entidad, perforación.
- Cotas: ángulo suplementario, largo total, entre círculos (mín./máx.), ordenadas, cadena,
  unidades en la cota, bloquear, mostrar nombre o fórmula, mover el texto de la cota.
- Solver: sugerir restricciones, definir automáticamente, grados libres por entidad a la
  vista, no invertir la geometría con cambios grandes, resolución parcial con conflicto.
- Asistencia: activar/desactivar inferencias, anclaje y espaciado de rejilla, coordenadas
  absolutas, relativas y polares.
- Gestión: bloques, subsketches, capas, exportar el sketch a DXF/SVG, peine de curvatura.

## Pruebas

- Funciones puras de `cad.ts` (transformar, copiar, dividir, validar) con `node --test` como
  `e2e/sketchPattern.test.mjs`.
- Solver: cada restricción nueva con un caso definido y uno sobredefinido en
  `solver/tests` o `model/tests`.
- e2e (`e2e/cad.mjs`): un escenario por fase, solo el afectado mientras se trabaja.

## Riesgos

- **Deshacer y el solver asíncrono**: respuestas viejas que pisan lo repuesto (generación).
- **Transformar con restricciones**: mover lo elegido y dejar atado lo de afuera puede no
  tener solución; se desprende antes que fallar.
- **Proyección de intersecciones**: curvas B-spline de OCCT a geometría del sketch (se
  aproximan con splines como las elipses).
