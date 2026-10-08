# Sketch completo

Objetivo: que dibujar en un sketch se sienta como en Onshape: todo lo que se dibuja queda atado,
el color dice qué falta definir y están las herramientas y restricciones que se usan a diario.

Las inferencias (anclarse a medios, centros, origen, alineaciones) tienen su propio plan:
[PLAN_ANCLAJES.md](PLAN_ANCLAJES.md). Este plan cubre el resto.

## Qué tenemos hoy

- Entidades (`model/src/sketch.rs`, `Geometry`): línea, círculo, arco, spline interpolada.
- Restricciones (`SketchConstraint`): coincidente, fijo, horizontal y vertical de una línea,
  paralela, perpendicular, igual, tangente, punto en línea, punto en círculo, punto medio,
  simétrico, distancia, distancia horizontal y vertical, largo, radio, diámetro, ángulo.
- Herramientas de la interfaz: línea, rectángulo por esquinas, círculo por centro, arco por
  centro, arco tangente, polígono, ranura, recortar, extender, redondeo de esquina, equidistante.
- Cotas al dibujar con Enter para pasar a la siguiente; gestor de restricciones con resaltado.
- Diagnóstico de grados de libertad por espacio nulo (`solver`, `nullspace()`), que hoy solo se
  usa para el estado general del sketch.

## Qué falta y por qué

| Falta | Por qué |
|---|---|
| Radio como incógnita del solver | `Circle2 { center_idx, radius }` y `Arc2` guardan el radio como constante. Las restricciones que lo involucran (igual entre círculos, tangencias entre círculos, concéntrico con radio libre) se resuelven por trucos (p. ej. tangente arco-arco como punto en línea de los centros) o no se pueden expresar |
| Concéntrico | No existe la restricción; hoy se logra compartiendo el centro al dibujar |
| Coincidente curva-curva general, simétrico de entidades | Solo hay simétrico de dos puntos respecto de una línea |
| Horizontal / vertical entre dos puntos | El solver las tiene (`Horizontal { p1, p2 }`), el modelo no |
| Cotas de referencia (*driven*) | Toda cota maneja la geometría; no hay forma de "solo mostrar" una medida |
| Color por entidad según qué tan definida está | El espacio nulo se calcula pero no se reparte por entidad |
| Rectángulo por centro, arco por 3 puntos, elipse, punto suelto, texto | No se priorizaron |
| Simetría y patrón dentro del sketch | Solo existen como operaciones 3D |
| Spline con manijas editables | La spline solo interpola puntos; no hay tangentes en los extremos ni curvatura |
| Convertir a construcción y de vuelta | Existe la marca de construcción pero no una acción rápida (tecla Q en Onshape) |

## Diseño

### Radio como variable

- El solver pasa a guardar el radio de círculos y arcos como un parámetro más del vector de
  incógnitas (igual que las coordenadas de los puntos). `Radius`/`Diameter` pasan a ser
  restricciones sobre esa variable en vez de valores fijos.
- En arcos, el radio queda definido por centro e inicio; se agrega la ecuación
  `|fin − centro| = |inicio − centro|` (hoy implícita) y la variable radio se iguala a la primera.
- Con esto se pueden escribir directo: **igual entre círculos** (`r_a = r_b`),
  **tangente círculo-círculo** externa e interna (`|c_a − c_b| = r_a ± r_b`), **tangente
  línea-círculo** (distancia del centro a la línea = r), **concéntrico** (centros iguales).
- Migración: un documento viejo con `Circle { radius }` se lee igual; el valor guardado pasa a
  ser el valor inicial de la variable. Si no tiene cota de radio queda libre (como en Onshape).

### Restricciones nuevas en el modelo

| Restricción | Ecuación |
|---|---|
| `Concentric { a, b }` | centros coinciden (o se fusionan al crear) |
| `HorizontalPoints { a, b }` / `VerticalPoints { a, b }` | `a.y = b.y` / `a.x = b.x` |
| `SymmetricEntities { a, b, line }` | simetría punto a punto de dos entidades iguales |
| `Coincident` curva-curva | línea sobre línea (colineal), círculo sobre círculo |
| `Collinear { a, b }` | paralelas + un punto de b sobre a |

### Cotas de referencia

- Campo `driving: bool` en las cotas (por defecto `true`). Una cota de referencia no entra al
  solver; después de resolver se calcula su valor y se muestra entre paréntesis, como Onshape.
- Si una cota nueva sobre-define el sketch, se ofrece crearla como referencia en vez de fallar.

### Colores por entidad

- Después de resolver: para cada entidad, sus grados de libertad que quedan en el espacio nulo
  (ya existe `point_dof_in_nullspace` para puntos; se extiende al radio).
- Colores como Onshape: **azul** = le falta definir, **negro** (o el color de texto del tema) =
  totalmente definida, **rojo** = en conflicto. El arrastre de una entidad azul mueve solo lo
  que está libre (ya funciona con `solve_drag`).
- El estado del sketch ("faltan 3 grados") ya existe; se agrega el número en la barra del sketch.

### Herramientas nuevas

- **Rectángulo por centro**: cuatro líneas + punto central de construcción con punto medio en
  las diagonales.
- **Arco por 3 puntos**: inicio, fin y un punto por donde pasa; se calcula el centro.
- **Elipse** (entidad nueva `Ellipse { center, major, ratio }`) — necesita soporte en
  `regions.rs` y en el puente (`GC_MakeEllipse` en OCCT). Opcional: arco de elipse.
- **Punto suelto**: entidad punto, útil para agujeros y referencias.
- **Texto**: entidad que genera contornos con una fuente; requiere decidir de dónde salen las
  fuentes (OCCT `Font_BRepTextBuilder` necesita freetype, que hoy no se compila). Alternativa:
  convertir el texto a contornos en el frontend con `opentype.js` y guardarlo como splines.
  Se deja al final.
- **Simetría en el sketch**: elige entidades y una línea; crea copias con `SymmetricEntities`.
- **Patrón en el sketch**: lineal y circular; copias con restricciones de igualdad y distancia
  o ángulo, de modo que cambiar una cota mueve todas.
- **Spline con manijas**: tangente opcional en extremos y en puntos intermedios; manijas
  arrastrables. Usa `GeomAPI_Interpolate` con tangentes (ya disponible en TKGeomAlgo).
- **Construcción rápida**: tecla Q alterna construcción en lo seleccionado.

## Fases

1. **Inferencias** — según [PLAN_ANCLAJES.md](PLAN_ANCLAJES.md), fases 1 a 3. *Hecha el 2026-10-05.*
2. **Radio variable** en el solver + concéntrico, igual y tangencias generales. Es el cambio
   de fondo; todo lo demás se apoya en esto. *Hecha el 2026-10-05: en vez de un parámetro
   aparte, cada círculo lleva un punto oculto en su borde (a la derecha del centro, con
   horizontal implícita); en los arcos el borde es el inicio.*
3. **Colores por entidad** y número de grados libres; cotas de referencia. *Hecha el 2026-10-05.*
4. **Herramientas simples**: rectángulo por centro, arco por 3 puntos, punto, construcción
   rápida. *Hecha el 2026-10-05.*
5. **Simetría y patrón en el sketch.** *Hecha el 2026-10-05.*
6. **Elipse y spline con manijas.** *Hecha el 2026-10-05 (manijas solo en los extremos; las intermedias y la curvatura quedan para más adelante).*
7. **Texto** (después de decidir fuentes). *Hecha el 2026-10-05 con opentype.js (decisión del
   usuario): el texto se inserta como curvas. Desde el 2026-10-08 queda como bloque
   (`Sketch.texts`: texto, tamaño, fuente, ancla, curvas y puntos): sus puntos se mueven con el
   ancla (el comienzo de la línea base), que se arrastra, acota o ancla como cualquier punto; los
   que no toca nada más no entran al solver (se reponen con su distancia al ancla: 3 s → 15 ms
   con "Hola"); elegido con «Elegir», la barra deja cambiar lo que dice y el tamaño y lo rehace
   en el mismo lugar; borrar una de sus curvas borra el texto entero.*

## Pruebas

- Solver (`cad-solver`): casos con radio libre — círculo tangente a dos líneas queda definido
  al dar el radio; dos círculos iguales y tangentes; arco tangente a línea y círculo.
- Modelo: documento viejo con radio fijo se carga y resuelve igual (prueba de migración con un
  JSON guardado en `tests/`).
- Diagnóstico: rectángulo sin cotas → 4 entidades azules; con dos cotas y un punto fijo → todas
  negras; cota que sobra → se ofrece como referencia.
- E2E: dibujar rectángulo por centro, acotar, verificar que quede centrado al cambiar la cota.

## Riesgos

- **Radio variable cambia la convergencia**: más incógnitas y ecuaciones no lineales; arcos que
  se "dan vuelta" (radio negativo o paso por cero). Mitigación: radio con valor inicial del
  dibujo, límites en el paso de Newton y prueba con los sketches del banco e2e.
- **Compatibilidad** de documentos y del cálculo de regiones: el radio deja de ser dato y pasa
  a ser resultado; todo lo que lee `Geometry::Circle { radius }` debe leer el radio resuelto.
- **Texto**: dependencia de fuentes y tamaño del binario.
