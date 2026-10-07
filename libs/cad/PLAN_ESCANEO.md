# Escaneo → CAD, siguiente etapa

Objetivo: lo que Onshape no tiene y nos diferencia: partir de un escaneo (orizon3d, Revopoint,
plato giratorio, malla importada) y llegar rápido a un diseño paramétrico, con forma de comparar
cuánto se parece el sólido al escaneo.

## Qué tenemos hoy (`libs/cad/scan`)

- `ScanMesh` con soldado de vértices y rayos (`raycast_exit`, `depth`).
- Elegir en el escaneo: plano (`pick_plane`) y cilindro (`pick_cylinder`, ajuste propio por
  normales `fit_cylinder_normals`), con aviso de normales poco fiables.
- `detect_all`: detección automática de planos y cilindros con suavizado por segmentos.
- `outline.rs`: corte del escaneo por un plano, simplificación Douglas-Peucker, ajuste de
  círculos y armado de un sketch (`outline_sketch`).
- Comandos en la app: `cad_scan_pick`, `cad_scan_detect`, `cad_scan_slice`, `cad_scan_add`; el
  escaneo se ve como fantasma en el visor CAD.
- Ejemplo `scan_report` para revisar detecciones en lote.

## Qué falta

| Falta | Por qué importa |
|---|---|
| `detect_all` parte mal planos grandes | En piezas grandes un plano sale en varios trozos o se mezcla con vecinos; hay que unir trozos coplanares y separar por bordes vivos |
| Conos, esferas, toros | Chaflanes cónicos, bolas y redondeos grandes no se reconocen |
| Mapa de desviación sólido ↔ escaneo | Ver en color dónde el diseño se aleja del escaneo (como Geomagic / Fusion "mesh compare") |
| Alinear el escaneo a ejes | El escaneo llega torcido; alinear con un plano al suelo y un eje antes de dibujar (la app ya tiene `placement` para mallas) |
| Cortes guiados | Proponer automáticamente los planos de corte útiles (paralelos a los planos detectados, a distintas alturas) |
| Restricciones inferidas en el sketch del corte | El sketch del contorno sale con cotas pero sin paralelas/perpendiculares/tangentes que el usuario esperaría |
| Ajustar cota al escaneo | Con una cota elegida, proponer el valor medido en el escaneo |
| Redondeos detectados | Reconocer bandas de curvatura entre dos planos como redondeo con su radio |

## Diseño

### Detección

- Unir regiones: después de `detect_all`, fusionar planos con normales a < 2° y distancia entre
  planos < tolerancia; reajustar con la unión.
- Separar por bordes vivos: no crecer regiones a través de aristas con ángulo diédrico alto.
- Primitivas nuevas: cono (eje + semiángulo), esfera (ajuste algebraico), toro (redondeo de
  revolución). Mismo esquema RANSAC + refinamiento que el cilindro.
- Redondeo lineal: banda cilíndrica tangente a dos planos → proponer operación Redondeo con el
  radio ajustado sobre la arista correspondiente del sólido.

### Desviación

- Comando `cad_deviation(muestras) → { distancias por vértice del escaneo, estadísticas }`:
  distancia con signo de cada vértice del escaneo al sólido (`BRepExtrema` con el sólido o, más
  rápido, contra su teselación con un BVH en Rust).
- Visor: el fantasma del escaneo se colorea con una escala (azul dentro, verde ±tolerancia,
  rojo fuera) y leyenda con media, máximo y percentil 95.
- Usarlo también como prueba en el banco: un diseño de referencia y su escaneo sintético.

### Flujo guiado

1. Importar escaneo → alinear (piso + frente) reutilizando la lógica de `placement`.
2. Detectar → lista de planos/cilindros/conos con casilla para usar cada uno.
3. Proponer cortes en las alturas donde cambia el contorno (picos en el perfil de área por
   altura).
4. Cada corte genera un sketch con restricciones inferidas (paralelas y perpendiculares a ±1°,
   tangencias, iguales) y cotas redondeadas a la precisión elegida (0,1 mm / 0,5 mm / 1 mm).
5. Extruir/revolucionar y comparar con el mapa de desviación.

## Fases

1. **Desviación** sólido ↔ escaneo con colores y estadísticas (lo más pedido para validar). *Hecha el
   2026-10-07: `cad_scan::deviation` (árbol de cajas propio sobre la teselación del sólido,
   punto más cercano de Ericson, signo por la normal del sólido; por triángulo del escaneo, en
   su centro, porque los triángulos del escaneo conservan el orden de la malla que dibuja el
   visor). Resumen pesado por área: media con signo, media absoluta, RMS, P95, máximo y
   fracción dentro de la tolerancia. `cad_deviation` responde en binario (f32). El visor
   promedia por vértice y pinta verde dentro de ±tol, de celeste a azul hacia adentro y de
   amarillo a rojo hacia afuera (hasta el P95 o 3 tolerancias). Se borra si cambia el diseño.
   E2E con el espéculo real: el cilindro ajustado deja 58 % dentro de ±0,2 mm.*
2. **Detección mejorada**: unir y separar planos; prueba con las piezas reales de `~/Descargas`.
3. **Restricciones inferidas** en los sketches de corte y redondeo de cotas.
4. **Alinear el escaneo** y **cortes propuestos**.
5. **Conos, esferas y redondeos** detectados.

## Pruebas

- Banco sintético: diseño conocido → teselar → ruido gaussiano → `detect_all` debe recuperar
  cada cara con error de normal < 1° y de radio < 2 %.
- Desviación: sólido igual al escaneo sin ruido → media ≈ 0; desplazado 0,5 mm → media 0,5.
- Escaneos reales (molde, audiómetro): número de planos detectados antes/después de unir, con
  captura para revisar a ojo.

## Riesgos

- **Escaneos ruidosos o con agujeros** dan detecciones falsas; mantener siempre la decisión del
  usuario (casillas) y no crear operaciones automáticamente sin confirmar.
- **Costo de la desviación** con escaneos de millones de puntos: muestrear y usar BVH.
