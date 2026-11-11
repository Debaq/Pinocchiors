# Plan: poner a punto impresora y filamento (asistente de calibración)

Leyenda: ✅ hecho · 🟡 a medias · ⬜ falta · 🧪 falta probar con la impresora.

Objetivo (pedido del usuario, 2026-11-06): calibrar la impresora y el material para la mejor
impresión posible **con un asistente fácil**, nada como Orca (menús sueltos, números
sueltos, hay que saber qué mirar). Y que **la IA pueda ayudar**: decir qué prueba sigue,
mandarla a imprimir, mirar una foto de lo impreso y guardar el resultado.

## Decisiones

- **Por impresora física** (decidido por el usuario): cada K1 y cada AD5X del lab tiene su
  calibración con cada filamento. Se identifica por el `id` de la impresora de red
  (`settings.netPrinters`); se guarda también su nombre y modelo.
- **Todas las pruebas** (decidido): básicas (cama y primera capa, temperatura, flujo,
  pressure advance), retracción y velocidad volumétrica, medidas y holguras (las del
  asistente de hoy) y avanzadas (VFA, input shaping, esquinas).
- **El motor es el de Orca 2.4.2** (`libslic3r`): sus modos de calibración
  (`Calib_Params`: torre de temperatura, PA, retracción, velocidad, VFA, input shaping,
  esquinas) ya cambian la temperatura, el PA, etc. capa por capa al escribir el gcode. Los
  ajustes que la GUI de Orca pone antes de cada prueba (`Plater::calib_*` y
  `adjust_settings_for_flowrate_calib`) se copian en Rust.
- **Modelos de prueba**: los de Orca (`resources/calib`, ~2 MB en Draco y 3MF), embebidos
  en el binario. Se cortan a la altura del rango pedido como hace Orca.
- **Dónde se guarda**: en el backend (`calibraciones.json` en la carpeta de configuración),
  no en el navegador: así lo leen el laminador, el conector MCP sin ventana y la IA.
- **Se aplica sola al laminar**: si se lamina para una impresora física con un filamento
  calibrado, sus valores van como base y los cambios a mano del usuario encima. Se ve en
  Laminar («Calibrada el 6/11: 212 °C · flujo 0,98 · PA 0,035») y se puede apagar.
- **Mandar la prueba**: desde el asistente se manda directo a la impresora elegida
  (conectada) con confirmación, o se guarda el gcode (tarjeta/USB). Nunca se imprime sin
  que el usuario apriete el botón.

## Lo que guarda cada calibración

| Nivel | Valor | Clave de Orca | Prueba |
|---|---|---|---|
| Impresora | Altura de la primera capa (Z offset) | (gcode de inicio / `SET_GCODE_OFFSET`) | Primera capa |
| Impresora | Input shaping (frecuencia y amortiguación X/Y) | (printer.cfg; Klipper) | Input shaping |
| Impresora | Holgura de ajuste, compensación de agujeros | `fit_clearance`, `hole_comp` (propias) | Placa de holgura, bloque |
| Impresora | Velocidad sin marcas (VFA) | límite de velocidad de pared | VFA |
| Imp. + filamento | Temperatura de boquilla | `nozzle_temperature(_initial_layer)` | Torre de temperatura |
| Imp. + filamento | Flujo | `filament_flow_ratio` | Flujo (2 pasadas) |
| Imp. + filamento | Pressure advance | `pressure_advance`, `enable_pressure_advance` | Torre / patrón de PA |
| Imp. + filamento | Retracción (largo y velocidad) | `filament_retraction_length/speed` | Torre de retracción |
| Imp. + filamento | Velocidad volumétrica máxima | `filament_max_volumetric_speed` | Prueba de velocidad |
| Imp. + filamento | Contracción por eje | `shrink` (propia) | Bloque medido |

Cada valor lleva la fecha, la prueba de la que salió y una nota; se guarda el historial.

## Fases

| Fase | Contenido | Estado |
|---|---|---|
| C0 | **Motor**: `pinocchio-slicer --calib calib.json`: modo, inicio, fin, paso, números, y configuración por objeto (cada STL con sus claves, p. ej. `print_flow_ratio`). `SliceJob.calib` en Rust. Prueba con el motor real: la torre de temperatura cambia `M104` cada 10 mm | ✅ (2026-11-06): además `@keep_below`/`@keep_above` cortan la pieza con tapa (`cut_mesh` de Orca; con mallas abiertas se cae, por eso el motor recibe el modelo entero y el recorte solo se usa para la miniatura), claves de pieza estrictas (un valor mal escrito es error) y carpeta temporal por proceso |
| C1 | **Pruebas** (`libs/slicer/src/calib.rs`): cada prueba arma modelos + ajustes + parámetros como `Plater::calib_*`: temperatura, flujo (YOLO lineal, 2 pasadas), PA (torre; después patrón), retracción, velocidad volumétrica, VFA, input shaping (frecuencia y amortiguación, Klipper), esquinas, primera capa (parches propios) y las de medidas (placa de holgura y bloque, ya existen). Rangos por defecto según el tipo de filamento (PLA, PETG, ABS/ASA, TPU…) | ✅ (2026-11-06): modelos de Orca empaquetados con `examples/pack_calib.rs` en `libs/slicer/calib/*.bin.gz` (~2 MB); `tests/calib_engine.rs` lamina todas con el motor real (K1 y AD5X). PA solo torre (el patrón queda para después) |
| C2 | **Calibraciones guardadas** (backend): `calibraciones.json`; comandos para leer, guardar un valor, borrar y ver el historial; `overrides` que salen de una impresora + filamento | ✅ (2026-11-06): `apps/desktop/src/calibrations.rs`; en Klipper el Z offset, el input shaping y las esquinas van al final del gcode de inicio (`SET_GCODE_OFFSET`, `SET_INPUT_SHAPER`, `SET_VELOCITY_LIMIT`), sin tocar printer.cfg; el Z que guarda la propia impresora (Creality de fábrica, AD5X) no se repite |
| C3 | **Laminar para una impresora física**: en Laminar se elige «Para: K1-436A» (las de ese modelo); se aplican sus valores (con aviso y opción de apagarlos) y el envío va a esa impresora | ✅ en código (2026-11-06): `slice_plate(calibration)`, «Para» y «Usar la calibración de…» en `SlicerSection.tsx` 🧪 |
| C4 | **Asistente «Poner a punto»**: elegir impresora y filamento → lista de pasos en el orden recomendado, cada uno con su estado (hecho el…, pendiente, desactualizado). Cada paso: qué arregla (dos líneas y un dibujo), rango propuesto, «Imprimir la prueba» (lamina, muestra la vista previa y manda o guarda), progreso de la impresión, y «¿Cuál salió mejor?» con un dibujo de la prueba numerado donde se hace clic. Afinar con una segunda pasada donde sirve (flujo, PA). Reemplaza al asistente de holguras actual (sus dos pruebas pasan a ser pasos) | ✅ en código (2026-11-06): `TuneWizard.tsx` (`lib/calibration.ts`), se abre desde Fabricar → Impresora y desde Laminar; el asistente viejo queda para impresoras sin conexión 🧪 |
| C5 | **Cama y primera capa con la impresora conectada**: autonivelado por la conexión (Klipper `BED_MESH_CALIBRATE`, Creality de fábrica su comando, AD5X nivela al empezar), parche de primera capa y botones «más cerca / más lejos» que mueven el Z en vivo mientras imprime (`SET_GCODE_OFFSET Z_ADJUST`); el resultado se guarda. Input shaping automático con el acelerómetro donde lo hay (K1: `SHAPER_CALIBRATE`) | 🟡 (2026-11-06): medir la cama y ±Z en vivo en el paso de primera capa (Klipper y K1 de fábrica; la AD5X se ajusta en su pantalla y se anota). Falta el botón de input shaping automático 🧪 |
| C6 | **IA**: herramientas MCP `calibration_status` (qué está calibrado, qué falta, qué sigue), `calibration_test` (lamina una prueba; mandarla pide permiso del usuario como `printer_send`) y `calibration_save`; guía y prompt «poner a punto». En la app: «Pregúntale a la IA» en cada paso abre el chat con el contexto de la prueba; se adjunta una foto de lo impreso y la IA elige y guarda | ⬜ |
| C7 | **Pruebas reales con el usuario**: K1 rooteada, K1 de fábrica y AD5X, con PLA y PETG; comparar una pieza antes y después de calibrar | ⬜ 🧪 |

## Orden recomendado del asistente

1. Cama limpia, autonivelado y primera capa (todo lo demás depende de esto).
2. Temperatura.
3. Flujo (pasada gruesa y fina).
4. Pressure advance.
5. Retracción.
6. Velocidad volumétrica máxima.
7. Medidas y holguras (placa de holgura y bloque con calibre).
8. Avanzadas: input shaping, VFA, esquinas.

Si cambia algo de antes (por ejemplo, la temperatura), los pasos que dependen de él se
marcan «conviene repetir».

## Riesgos

- Los modos de calibración de Orca suponen Klipper o Marlin según `gcode_flavor`; la AD5X
  (Klipper de Flashforge) y la K1 de fábrica (Klipper de Creality) hay que probarlos 🧪.
- Z offset en la K1 de fábrica: se guarda por su pantalla/firmware; ver qué acepta
  `gcodeCmd` (PLAN_IMPRESORAS.md). Si no se puede guardar en la impresora, se pone en el
  gcode de inicio.
- Input shaping: el resultado va al `printer.cfg` (Klipper). En la K1 de fábrica no se
  toca el `printer.cfg` sin permiso explícito.

## Dónde mirar

- Orca 2.4.2: `src/slic3r/GUI/Plater.cpp` (`calib_*`, `adjust_settings_for_flowrate_calib`),
  `src/libslic3r/calib.hpp/.cpp`, `src/libslic3r/GCode.cpp` (busca `calib_params`),
  `resources/calib/` (en `~/.cargo-target/orca/OrcaSlicer-2.4.2`).
- Lo nuestro: `libs/slicer/engine/main.cpp`, `libs/slicer/src/{settings,engine}.rs`,
  `libs/pinocchio/print3d/src/calibration.rs` (placa y bloque),
  `apps/web/src/components/layout/CalibrationWizard.tsx` (asistente de holguras de hoy).

## 2026-11-11: por boquilla, escuadra, patrón y puesta a punto de la máquina

- ✅ Calibraciones por boquilla (`calibrations.rs`: `NozzleCal`, migración de lo viejo a 0,4). Lo del filamento y z_offset/holguras van por diámetro; shaper, VFA, esquinas y escuadra son de la impresora.
- ✅ Escuadra de ejes (skew): placa generada en `calib.rs`, 3 medidas → `SET_SKEW` en el gcode de inicio (Klipper con `[skew_correction]`). 🧪 Falta confirmar el signo con una impresora real.
- 🟡 Pressure advance con patrón: la prueba y el paso del asistente existen, pero el motor no dibuja el patrón (en Orca lo agrega la GUI con `CalibPressureAdvancePattern::generate_custom_gcodes` sobre `model.plates_custom_gcodes`; hay que hacerlo en `engine/main.cpp` y recompilar el motor). Mientras tanto el asistente usa la torre.
- ✅ Puesta a punto de la máquina (Klipper, `PrinterSetup.tsx` + `printer_setup_info/action`): PID con temperatura, revisión (finales de carrera, motores, calentadores, ventiladores), extrusor (rotation_distance con copia de printer.cfg), sonda (prueba del papel y precisión), tornillos, malla. 🧪 Nada probado con impresora real.
