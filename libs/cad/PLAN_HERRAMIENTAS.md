# Plan: herramientas de Diseñar unificadas y trabajo sobre la piel

Plan desde el 2026-11-07. Pedido del usuario: «pegar objetos o amoldar sobre caras irregulares,
en especial de STL», y después de revisar las herramientas: «hazlo todo».

Dos ideas guían todo:

1. **Una herramienta por idea.** Muchas herramientas nacieron como botones aparte siendo la
   misma operación con otro comportamiento. Se juntan en una con un selector; los botones que
   sirvan de atajo pueden quedar, pero abren la misma herramienta y su diálogo deja cambiar el
   tipo después. Los proyectos guardados siguen abriendo igual (los tipos internos se
   conservan o se convierten al leer).
2. **La piel es un destino como cualquier otro.** Lo que hoy se hace sobre una cara CAD
   (relieve, curva proyectada, referencias, mover cara hasta…) tiene que poder hacerse sobre un
   STL o escaneo, y al revés, con la misma herramienta.

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta.

## Fases

### U0 — Detalles de la revisión de Sólidos ✅
Tangencia de la transición desactivada si las secciones tienen distinta cantidad de lados,
esfera sin «Eje», textos de la caja traducidos, un solo diálogo de STEP a la vez. Desmolde con
agujeros y pared delgada (f17e309).

### U1 — Relieve sobre la piel (proyectar) ❌
Una operación `Relief`: un sketch (texto, logo, cualquier región) llevado a una superficie con
altura. Reemplaza a «Grabado», «Relieve» y al «Parche desde sketch» usado para texto.

- Destino: la piel del escaneo/STL (y después caras de un sólido).
- Cómo llega el sketch: **proyectar** (en línea recta por la normal del sketch; las medidas
  valen en el plano). Envolver llega en U2.
- Altura, cuatro formas:
  1. **Pareja sobre la piel**: el mismo espesor perpendicular a la piel en todo punto (la cara
     de arriba copia la piel).
  2. **Tope plano**: base amoldada a la piel, cara de arriba en un plano (a una distancia del
     plano del sketch).
  3. **Tope plano automático**: el plano a la altura pedida sobre el punto más alto de la piel
     bajo el sketch (ninguna parte queda más baja).
  4. **Según la dirección**: la piel corrida la altura a lo largo de la normal del sketch
     (paredes paralelas, como proyectado con una linterna).
- Sentido: **afuera** (relieve) o **adentro** (grabado).
- Resultado: pieza nueva, unir o restar a piezas del diseño.
- Muestra las dos medidas del sketch: en el plano y sobre la piel.

### U2 — Envolver un sketch sobre la piel ❌
«Envolver» como segunda forma de llevar el sketch: las medidas valen sobre la piel (como una
calcomanía). El trozo de piel bajo el sketch se aplana con el desplegado de UV que ya tiene la
app (uv-core), el sketch se ubica en ese aplanado (ancla = clic en la piel + giro) y se lleva de
vuelta. Mapa de estiramiento a color y aviso si pasa de 2 %. Sirve para relieve (U1) y para
curvas (U5).

### U3 — Espesor sobre piel ❌
«Escaneo como cuerpo», «Carcasa», «Parche desde sketch» y «Pincel» son la misma operación
(`ScanBody`): un solo botón «Espesor sobre piel» con De qué / Dónde (todo, bajo un sketch,
pintado) / Hacia dónde; el diálogo muestra los toques del pincel y deja agregarlos o borrarlos.
«Engrosar» aparece una sola vez.

### U4 — Mover cara hasta… ❌
«Mover cara», «Reemplazar cara», «Cara al escaneo» y «Girar cara»: una herramienta con
«Movimiento: distancia / hasta un plano o cara / hasta el escaneo / girar». Se puede cambiar
entre ellos en el diálogo sin perder las caras elegidas.

### U5 — Llevar a la superficie (curvas) ❌
«Sketch envuelto», «Curva proyectada», «Sketch a la piel»: una herramienta con destino (caras
o escaneo) y forma (proyectar / envolver). «Curva por puntos» y «Camino por puntos»: una sola,
sobre vértices del sólido o sobre la piel.

### U6 — Referencias desde el escaneo en el menú ❌
«Plano», «Eje» y «Punto» del menú aceptan clics sobre el escaneo (zona, 3 puntos, cilindro,
punto) y dejan las referencias ligadas. El panel del escaneo deja de repetirlas.

### U7 — Selector de tipo en lo que ya es una sola operación ❌
- Patrón: el diálogo deja cambiar el tipo (lineal, circular, curva, cadena, espiral,
  superficie, tabla, relleno, puntos); un botón «Patrón» y los tipos como atajos.
- Primitiva: cambiar la forma en el diálogo (caja, cilindro, esfera, cono, toro).
- Pieza estándar: tornillo, tuerca, arandela intercambiables.
- Mover pieza: «Mover / Girar / Copiar cuerpo» en uno con «dejar copia».
- Hélice y espiral: un botón. Barrido y tubo: un botón.

### U8 — Partir ❌
«Cortar por plano», «Partir», «Separar», «Recortar con plano», «Recortar con sólido»: «Partir»
con «con: plano o pieza» y «quedarse con: un lado / los dos / lo de adentro», para sólidos y
superficies.

### U9 — Redondeo y chaflán ❌
«Redondeo», «Redondeo completo», «Redondeo de cara»: «Redondeo» con tipo (aristas, completo,
entre caras). «Chaflán» y «Chaflán de vértice»: «Chaflán» con tipo.

### U10 — Sólido o superficie ❌
Extrusión, revolución, transición y barrido con «Resultado: sólido / superficie»; el menú
Superficies queda con relleno, coser, extender, recortar y desfasar.

### U11 — Colocar sobre la piel ❌
Clic en un punto del STL o de una cara curva: un sistema de coordenadas ligado a la piel
(origen en el punto, Z por la normal). Primitivas, piezas estándar y piezas derivadas se
pueden colocar ahí, con «rellenar hasta la piel» (la base se amolda).

### U12 — Unir y grabar en el STL sin perder detalle ❌
Hoy juntar lo agregado con el STL pasa el STL entero por vóxeles (20 000 triángulos): se
redondean las aristas. Booleanas directas sobre la malla (unir, restar) para exportar el STL
con lo agregado o grabado conservando su detalle.

## Registro
