# Anclajes del sketch (inferencias)

## Cómo se llaman

En Onshape (y en SolidWorks, Fusion, FreeCAD) se llaman **inferencias** (*inferencing*):
mientras se dibuja, el cursor "se pega" a puntos y direcciones significativas, y al hacer clic
se crea la **restricción** que corresponde. Hay dos familias:

- **Puntos de anclaje** (*snap points*): origen, extremos, puntos medios, centros, cuadrantes,
  intersecciones, vértices y aristas del sólido.
- **Líneas de inferencia**: líneas punteadas que aparecen al alinearse con algo: horizontal o
  vertical respecto de otro punto, paralela, perpendicular o tangente a otra entidad.

Lo importante es que no son solo imán visual: cada anclaje deja una restricción
(coincidente, punto medio, tangente…), así el dibujo queda "atado" y se mueve bien al cambiar
las cotas.

## Qué tenemos hoy y por qué falta el resto

Hoy (`CadView.tsx`, `snapped` / `placePoint`):

| Anclaje | Estado | Restricción que deja |
|---|---|---|
| Extremo de línea o arco | Sí (puntos a ≤ 8 px) | Coincidente (comparte el punto) |
| Centro de círculo o arco | Sí (es un punto más del sketch) | Coincidente |
| Sobre una línea | Sí | Parte la línea en ese punto |
| Sobre círculo o arco | Sí | Punto en círculo |
| Horizontal / vertical de la propia línea (±3°) | Sí | Horizontal / vertical |
| Origen del sketch | Sí (fase 1) | Comparte el punto origen |
| Punto medio de una línea | Sí (fase 1) | Parte la línea, mitades iguales |
| Cuadrantes de un círculo o arco (0°, 90°, 180°, 270°) | Sí (fase 1) | Punto en círculo + alineado H/V con el centro |
| **Intersección** de dos curvas | **No** | — |
| **Alineación** con otro punto (línea punteada H/V) | **No** | — |
| **Paralela / perpendicular / tangente** al dibujar | **No** | — |
| **Vértices y aristas del sólido** (al dibujar sobre una cara) | **No** | — |
| Aviso visual de qué anclaje está activo | Sí (fase 1): punto resaltado y glifo | — |
| Anclajes en todas las herramientas, Mayús para dibujar libre | Sí (fase 1) | — |

Por qué faltan:

1. **El sketch no tiene origen.** En Onshape cada sketch trae el origen como punto fijo al que
   se puede atar todo; nuestro `Sketch` solo tiene los puntos que dibuja el usuario.
2. **Solo se buscan puntos que existen.** El punto medio, los cuadrantes y las intersecciones
   no son puntos del sketch: hay que calcularlos como candidatos virtuales.
3. **El enganche solo lo usa la herramienta Línea.** Rectángulo, círculo, arco, polígono y
   ranura crean puntos nuevos sin mirar si el clic cayó en algo (ni siquiera reusan un extremo).
4. **No hay restricciones entre dos puntos** para horizontal/vertical: el modelo solo tiene
   "esta línea es horizontal". El solver sí las tiene (`Horizontal { p1, p2 }`), falta exponerlas.
5. **La geometría del sólido no entra al sketch.** Para anclarse a una esquina de la cara hay
   que proyectar las aristas al plano del sketch (lo que Onshape llama "Usar").
6. Se priorizó el núcleo (solver, operaciones, cotas) y el enganche quedó en lo mínimo.

## Diseño

### Un módulo de inferencias, puro y probado

`apps/web/src/lib/sketchSnap.ts`, sin Three.js ni Solid, para probarlo con node como
`lib/history.ts`:

```
infer(sketch, cursor, { tolerancia, puntoAnterior?, entidadDeOrigen?, geometríaDelSólido? })
  → { punto, tipo, glifo, restricciones: SketchConstraint[], referencias, líneasGuía }
```

- **Candidatos** en coordenadas del sketch: origen, extremos, medios, centros, cuadrantes,
  intersecciones (línea-línea, línea-círculo, círculo-círculo, reutilizando lo de `trimCurve`),
  vértices proyectados del sólido.
- **Prioridad** cuando hay varios a menos de la tolerancia (como Onshape): punto existente >
  origen > intersección > punto medio > centro > cuadrante > vértice del sólido > sobre una curva
  > alineación H/V > nada. A igual prioridad gana el más cercano.
- **Alineaciones** (no son puntos): si el cursor está a ≤ tolerancia de la horizontal o vertical
  de un punto existente, se corrige esa coordenada y se dibuja la línea guía. Puede combinarse
  con un punto ("medio de esa línea, alineado con aquel punto").
- **Desactivar**: mantener **Mayús** apretada mientras se dibuja apaga las inferencias (es lo
  que hace Onshape).

### Restricción que deja cada uno

| Inferencia | Restricción |
|---|---|
| Punto existente | Comparte el punto (coincidente) |
| Origen | Coincidente con el punto origen del sketch |
| Punto medio | `midpoint` |
| Centro | Comparte el centro |
| Cuadrante | `point_on_circle` + horizontal/vertical con el centro |
| Intersección | Sobre las dos curvas (las líneas se parten, como hoy) |
| Sobre una curva | Como hoy (partir línea / `point_on_circle`) |
| Alineado H/V con un punto | **Nueva**: horizontal/vertical entre dos puntos |
| Paralela / perpendicular a una línea | `parallel` / `perpendicular` |
| Tangente al salir del extremo de un arco | `tangent` |
| Vértice o arista del sólido | Ver fase 4 |

### Aviso visual

- **Glifo** junto al cursor según el tipo (◎ origen, △ medio, ⊙ centro, ✕ intersección,
  ◇ cuadrante, ⊥, ∥, tangente) y el punto candidato resaltado.
- **Líneas guía** punteadas para alineaciones, paralelas y perpendiculares.
- La entidad de la que sale la inferencia se resalta (como el gestor de restricciones).

## Fases

1. **Origen y puntos clave.** *Hecha el 2026-10-05; ver la bitácora de [ROADMAP.md](ROADMAP.md).*
   - Origen del sketch: un punto reservado y fijo en (0, 0) del plano (backend: punto marcado
     como origen, con restricción fija implícita, sin poder borrarlo), dibujado como en Onshape.
   - Candidatos: punto medio, cuadrantes, más lo que ya hay.
   - Usarlo en **todas** las herramientas (rectángulo, círculo, arco, polígono, ranura, arco
     tangente), no solo en Línea.
   - Glifo del anclaje activo.
2. **Intersecciones.** Candidato en los cruces entre curvas, dejando el punto sobre las dos.
3. **Líneas de inferencia.**
   - Restricciones nuevas en el modelo: horizontal y vertical **entre dos puntos** (el solver
     ya las tiene).
   - Alineación H/V con puntos existentes, con línea guía.
   - Al dibujar una línea desde un extremo: paralela o perpendicular a otra línea, y tangente si
     sale de un arco.
4. **Geometría del sólido.**
   - Al dibujar sobre una cara: proyectar al plano sus vértices y aristas (bordes de la cara
     primero, después el resto visible) y ofrecerlos como candidatos.
   - Primera versión: el anclaje solo toma la posición (sin restricción).
   - Segunda versión, "Usar arista": entidad de construcción ligada a la arista del sólido (por
     su origen, como los redondeos) que se recalcula si el sólido cambia; los anclajes a ella sí
     dejan restricción.
5. **Ajustes.** Mayús para desactivar, tolerancia en píxeles configurable, y opcional: anclar a
   la grilla.

## Pruebas

- `sketchSnap.ts` con pruebas en node: cada tipo de candidato, la prioridad entre varios
  cercanos, alineaciones combinadas, Mayús.
- Escenario de punta a punta: línea desde el origen hasta el punto medio de otra, círculo con
  centro en una intersección y rectángulo alineado; se verifica que el documento tenga las
  restricciones esperadas y que el sketch quede definido como corresponde.

## Riesgos

- **Demasiadas restricciones automáticas** pueden chocar entre sí. Mitigación: la prioridad deja
  una sola inferencia de punto por clic, más como mucho una alineación, y se puede deshacer.
  Mayús permite dibujar libre.
- **Rendimiento** con sketches grandes: limitar los candidatos a los cercanos al cursor (una
  grilla espacial si hiciera falta).
- **Fase 4** necesita que el backend entregue las aristas proyectadas, y que la referencia a la
  arista sobreviva los recálculos (los orígenes de caras ya resuelven buena parte).
