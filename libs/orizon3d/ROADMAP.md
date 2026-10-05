# Orizon3D — escáner de plato giratorio (webcam UVC + láser)

Escáner propio sin programas externos: el objeto gira en un plato, la cámara
sube por un arco y dos láseres de línea vertical lo cortan. Todo en Rust, en
`src/turntable/`. Primero se prueba en un banco sintético donde la forma real
es conocida, después en el hardware.

## Hardware (2026-10-01)

- **Cámara**: HD USB Camera (ARC `05a3:4689`, `/dev/video8`). Solo MJPG:
  640×360, 1280×720, 1920×1080. Controles V4L2:
  - `focus_automatic_continuous` (activo por defecto: apagar) y
    `focus_absolute` 0–1023
  - `auto_exposure` 1 = manual, `exposure_time_absolute` 1–8188, `gain` 1–176
  - `white_balance_automatic` y `white_balance_temperature` 2806–6500
  - `sharpness` 0 (dejar en 0: el realce artificial deforma el perfil de la
    línea)
- **Arco**: 12 cm, de 0 a 135° (eje "Z" del firmware). Supuesto: 12 cm es la
  distancia de la cámara al centro del arco. Por ahora sin servo de
  inclinación, así que la cámara apunta al centro del arco.
- **Plato**: motor NEMA en directo (eje "Y" del firmware).
- **Controlador**: Arduino del equipo
  [scanEar](https://github.com/Debaq/scanEar), en `/dev/ttyACM0` a 115200
  baudios. El protocolo se dedujo del `main.py` porque el firmware no está en
  el repo:
  - `PY:MOVE_Y:<grados>`, `PY:MOVE_Z:<grados>`, `PY:RESET`
  - `PY:GET_STATUS` → `STATUS:{state, y_angle, z_angle, z_step, total_z_steps}`
  - respuestas `OK:{…}` y `ERROR:{message}`
  - `PY:SET_Y_CORRECTION`/`PY:SET_Z_CORRECTION` (Z = 9 por defecto, quizás la
    reducción del arco) y `set_y_speed`, `set_z_speed`, `set_y_inc`,
    `set_z_inc` (sin el prefijo `PY:`)
  - `PY:START_SCAN`/`PAUSE`/`RESUME`/`STOP`: escaneo manejado por el Arduino,
    que avisa "fotografiar" sin esperar a la cámara. No se usa.
  - **No hay comandos de láser ni de luz.** Se proponen `PY:LASER:<n>:<0|1>`
    y `PY:LIGHT:<0-255>`.
- **Uso**: el `3d_mod` agrega conductos de ventilación a un STL, así que se
  escanean impresiones de oído (objetos chicos).
- **Láseres y luz**: dos láseres rojos de línea vertical. Los láseres y la luz
  suben con la cámara, montados en el brazo del arco (`lasers_on_arm`).

Las medidas que faltan se suponen en `RigGeometry::webcam_1080p()`:
- centro del arco a 30 mm sobre el plato;
- láseres a ±30° de la cámara, a 120 mm del eje y a la altura del centro del
  arco (con el arco en 0°);
- campo de visión horizontal de 70°.

## Fase 1 — banco sintético y láser ✅

- `lens.rs`: cámara estenopeica con distorsión Brown-Conrady (la de OpenCV).
- `rig.rs`:
  - `RigGeometry` es el rig físico: arco, eje del plato y montaje de los
    láseres.
  - `Calibration`/`View` es lo que usa la reconstrucción: pose de la cámara y
    planos de los láseres por cada altura del arco.
- `laser.rs`:
  - Línea por fila: centroide sobre el piso del pico, con el cuadro sin láser
    restado.
  - Descarta las filas ambiguas (segundo pico de más del 50 %), las más anchas
    que 1,8 veces la mediana y 2 filas en cada punta de cada tramo, porque el
    desenfoque estira la línea fuera del borde y deja puntos en el aire.
  - Triangula como rayo ∩ plano del láser y deshace el giro del plato.
- `scan.rs`:
  - Traits `Rig` (home, move_to, set_laser, set_light) y `FrameSource` (grab).
  - `run_laser_scan` sigue un `ScanPlan`. En cada parada toma un cuadro de
    color, uno oscuro y uno por láser.
- `synth.rs`:
  - Render por trazado de rayos de un SDF o de una malla (con la BVH de
    `pinocchio-spatial`, en el ejemplo).
  - Simula la luz del domo, la línea gaussiana integrada en el píxel, la sombra
    del láser, el desenfoque, el ruido, la gamma y el color 4:2:0 del MJPG.
  - `SynthRig` simula pasos y juego del motor.
  - `accuracy` mide contra la forma real.

Ejemplo de uso:

```
cargo run --release -p orizon3d-core --example turntable_synth -- \
  [--stl x.stl --size 60] [--width 960] [--steps 90] [--elev 0,30,60] \
  [--axis-error mm] [--jitter grados] [--frames dir] [--out nube.ply]
```

Resultados con 960×540, 3 alturas y calibración exacta:

| Caso | Error medio | RMS | p95 |
|---|---|---|---|
| Objeto de prueba (caja, esfera, poste, mordisco), 60 paradas | 0,03 mm | 0,06 | 0,11 |
| `head.stl` a 60 mm, 90 paradas | 0,024 mm | 0,041 | 0,063 |
| Eje del plato corrido 0,5 mm en la calibración | 0,30 mm | 0,38 | 0,77 |
| Plato con σ = 0,1° por parada | 0,033 mm | 0,06 | 0,11 |

Láseres en la base (a 150 mm del eje y 40 mm de altura) contra láseres en el
brazo. Medido a 640×360 con 60 paradas; la cobertura cuenta la superficie
visible desde arriba del plato que tiene un punto a menos de 2 mm:

| Caso | Cobertura | Error medio | RMS | Máx |
|---|---|---|---|---|
| Objeto de prueba, base | 95,7 % | 0,043 | 0,074 | 1,14 |
| Objeto de prueba, brazo | 99,5 % | 0,032 | 0,046 | 0,95 |
| Cabeza, base | 99,1 % | 0,035 | 0,050 | 1,36 |
| Cabeza, brazo | 97,7 % | 0,030 | 0,039 | 0,69 |
| Arco errado +0,3°, base | 95,6 % | 0,047 | 0,093 | 8,56 |
| Arco errado +0,3°, brazo | 99,5 % | 0,056 | 0,073 | 0,94 |

El brazo gana: desde arriba el láser ilumina las caras superiores sin rasante.
Además, el plano del láser es fijo respecto de la cámara, así que se calibra
una sola vez.

**Conclusión**: lo crítico es calibrar bien el eje del plato. La precisión del
motor casi no influye.

Lo que todavía queda mal: aristas vivas y pliegues cóncavos dejan unos pocos
puntos de hasta ~1 mm.

## Fase 2 — calibración (siguiente)

Todo en Rust: tablero de ajedrez impreso apoyado en el plato.

- Detector propio de esquinas con refinamiento subpíxel.
- Método de Zhang con refinamiento Levenberg-Marquardt: intrínsecos y
  distorsión.
- Por cada altura del arco: pose de la cámara frente al tablero.
- Eje del plato: se gira el tablero y se ajusta la rotación común de sus poses.
- Plano de cada láser, en el marco de la cámara (va montado en el brazo): la
  línea sobre el tablero en varias poses da puntos 3D del plano, y se ajusta
  el plano a esos puntos. Se calibra una sola vez para todas las alturas.
- Arco (centro, radio, eje): se ajusta a las poses de la cámara medidas en
  varias alturas, así cualquier altura queda calibrada sin tablero.
- Validación en el sintético: recuperar la calibración real a partir de los
  tableros renderizados, y medir el error de escaneo que resulta.

## Fase 3 — hardware

- ✅ `scanear.rs`: `ScanEarRig` implementa `Rig` con el protocolo de arriba.
  - Da cada movimiento por terminado cuando `GET_STATUS` informa quieto en el
    ángulo pedido, y después espera a que se asiente la vibración.
  - Se probó contra un Arduino simulado en `tests/turntable.rs`.
- Pendiente: el firmware real, y agregarle los comandos de láser y luz.
- `FrameSource` V4L2 sobre la webcam:
  - foco, exposición, ganancia y balance de blancos en manual;
  - descartar los cuadros viejos del búfer después de cada movimiento.
- Comparar el escaneo real con el sintético del mismo rig.

## Fase 4 — nube a malla

- Filtrar por ángulo rasante usando la normal estimada y la dirección de vista
  (quita los puntos de aristas).
- Fusión y malla con `mesh::reconstruct`; después reparar y retopologizar con
  la cadena existente.
- Silueta con luz trasera y visual hull: como límite y para objetos oscuros o
  brillantes.

## Fase 5 — color y fotogrametría con poses conocidas

- Proyectar las fotos de color sobre la malla (horneado de `uv-core`).
- Estéreo multivista (barrido de planos o PatchMatch) usando las poses del
  rig, sin SfM.

## No simulado todavía

- Interreflexiones en concavidades.
- Moteado (speckle) del láser.
- Artefactos de bloque del JPEG (solo se simula el submuestreo de color).
- Superficies brillantes, translúcidas u oscuras.
