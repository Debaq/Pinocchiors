# Plan: imprimir desde Pinocchio en todas las impresoras del laboratorio

Después de S7 (`PLAN_SLICER.md`), que habla con Moonraker. Objetivo: que cualquier
impresora del laboratorio reciba, imprima y reporte desde Pinocchio, **sin obligar a
rootearlas**, y ver el estado de todas en un solo lugar.

> **Regla:** las otras impresoras están en uso. No conectarse a ellas hasta que el
> usuario lo diga, y empezar siempre solo leyendo (estado), nunca subiendo ni imprimiendo.

## Las impresoras

| Impresora | Firmware | Cómo se habla | Estado |
|---|---|---|---|
| Creality K1 (`192.168.50.250`, K1-436A) | Creality 1.3.3.46 rooteada (Helper Script) | Moonraker, puerto 7125 | ✅ hecho (S7, probado el envío real) |
| Creality K1 × 2 | Creality de fábrica | Protocolo propio de Creality (como Creality Print) | pendiente: fase P2 |
| Flashforge AD5X (`192.168.50.190`, SN SNMTIE9A00089) | Flashforge v3.1.7 de fábrica | Protocolo clásico TCP 8899 (responde sin clave); API local 8898 cerrada (modo LAN apagado) | pendiente: fases P3–P4 |

Por qué no rootear las otras: con el root la K1 se colgaba (era GuppyScreen, ya quitada) y
no les gusta cómo quedan. Orca normal nunca pudo imprimir en las K1 de fábrica: solo el
Orca modificado por Creality (Creality Print), porque Orca solo sabe Moonraker y OctoPrint
para estas.

## Fases

| Fase | Contenido | Estado |
|---|---|---|
| P1 | **Varias impresoras**: lista con nombre, tipo (Moonraker / Creality de fábrica / Flashforge), dirección y credenciales; estado de todas a la vista; al enviar se elige a cuál. Reemplaza `settings.moonraker` (una sola dirección) migrándola. Un `trait`/enum en Rust por tipo de conexión con las mismas operaciones: info, enviar(+imprimir), estado, pausar/reanudar/cancelar | pendiente |
| P2 | **K1 de fábrica (protocolo Creality)**: ver abajo. Primero solo leer el estado de una K1 libre; después subir sin imprimir; al final imprimir con el usuario al lado | pendiente (necesita una K1 de fábrica libre) |
| P3 | **Flashforge AD5X**: API local. Ver abajo. El usuario tiene que activar el modo LAN y dar el número de serie y el código de la pantalla | pendiente |
| P4 | **Perfiles AD5X en el laminador**: `tools/flatten_presets.py` hoy solo aplana Creality (`VENDOR = "Creality"`); Orca 2.4.2 trae `Flashforge AD5X 0.25/0.4/0.6/0.8 nozzle` y sus procesos (`@FF AD5X`). Primero un color; multicolor (IFS, 4 carretes) necesita que el motor lamine con varios filamentos (hoy fuerza uno: `filament_map` en `engine/main.cpp`) | pendiente |
| P5 | **Encontrarlas solas en la red**: Moonraker por mDNS, Creality como `CrealityHostDiscovery.cpp`, Flashforge por UDP 48899 | pendiente |
| P6 | Herramienta MCP para mandar a imprimir y ver el estado; cámara (K1 y AD5X) | pendiente |

## Protocolo Creality de fábrica (K1, K1C, K1 Max)

Lo sirve `/usr/bin/web-server` de Creality (en la K1 rooteada está desactivado como
`web-server.disabled` por el Helper Script; sus textos confirman todo lo de abajo).
Referencia: `src/slic3r/Utils/CrealityPrint.cpp` de Orca 2.4.2.

- **Modelo**: `GET http://<ip>/info` → JSON con `model` (los K2 son `F008`, `F012`, `F021`).
- **Subir**: `POST http://<ip>/upload/<archivo>` (puerto 80), multipart con el campo `file`
  y, en las que no son K2, también `path`.
- **Imprimir**: WebSocket `ws://<ip>:9999`, mensaje
  `{"method":"set","params":{"opGcodeFile":"printprt:/usr/data/printer_data/gcodes/<archivo>"}}`.
  En las K2 la ruta es `/mnt/UDISK/printer_data/gcodes/` (no aplica a las K1).
- **Pausar / cancelar**: mismo WebSocket, `{"method":"set","params":{"pause":1}}`
  (`0` reanuda) y `{"method":"set","params":{"stop":1}}`.
- **Estado**: la impresora empuja JSON por el WebSocket con `printProgress`, `printLeftTime`,
  `printJobTime`, `layer`, `TotalLayer`, `nozzleTemp`, `bedTemp0`, `state`, `deviceState`,
  `printFileName`. Lista de archivos: `{"method":"get","params":{"reqGcodeFile":1}}`.
- Hay latido (heartbeat) y la lista de archivos llega en varios mensajes que hay que juntar
  (lo cuenta el issue de 3d-cli). Sin documentación oficial: puede cambiar con el firmware.
- En Rust hace falta un cliente WebSocket (p. ej. `tungstenite`, síncrono y sin TLS).

## API local de Flashforge (AD5X, Adventurer 5M)

Referencia: `src/slic3r/Utils/Flashforge.cpp` de Orca 2.4.2.

- Requiere **modo LAN** en la impresora y dos datos de su pantalla: **número de serie**
  (`serialNumber`) y **código** (`checkCode`).
- **Estado**: `POST http://<ip>:8898/detail` con JSON `{"serialNumber":…,"checkCode":…}`;
  incluye la estación de materiales (`MatlStationInfo` → `SlotInfos`, `slotCnt`).
- **Subir (e imprimir)**: `POST http://<ip>:8898/uploadGcode`, multipart con `gcodeFile` y
  cabeceras `serialNumber`, `checkCode`, `fileSize`, `printNow` (true/false),
  `levelingBeforePrint`, `flowCalibration`, `firstLayerInspection`, `timeLapseVideo`,
  `useMatlStation`, `gcodeToolCnt`, `materialMappings` (JSON en base64: qué color del gcode
  va a qué carrete del IFS). La respuesta trae `code`/`err`.
- Protocolo viejo por TCP 8899 con comandos `~M…` (`~M28` subir, `~M23 0:/user/<archivo>`
  imprimir); Orca lo usa como respaldo. Descubrimiento por UDP 48899.
- Falta averiguar pausar/cancelar por la API local (probablemente `/control`).

**Lo que se vio en la AD5X del laboratorio (2026-11-05, solo lectura):** puertos 8899 y 22
abiertos; 8898, 80 y 7125 cerrados. Por el 8899, sin credenciales, responde a
`~M601 S1` (tomar control: «Control Success V2.1»), `~M115` (modelo, firmware v3.1.7, SN,
volumen 220×220×220, MAC), `~M119` (`MachineStatus: READY`, `CurrentFile:`), `~M105`
(`T0:20.9/0.0 T1:0.0/0.0 B:22.0/0.0`), `~M27` (`SD printing byte 0/100`, `Layer: 0/0`) y
`~M602` (soltar control). Cada comando termina en `ok`; los mensajes van con `\r\n`.
Camino más simple para P3: **todo por el 8899** (estado + `~M28`/`~M29` subir + `~M23`
imprimir), sin pedirle al usuario el código del modo LAN. Pausar/reanudar/cancelar en el
protocolo clásico: `~M25` / `~M24` / `~M26` (por confirmar en esta impresora).

**Envío probado (2026-11-05):** `~M601 S1` → `~M28 <bytes> 0:/user/<archivo>` (responde
«Writing to file: /usr/data/gcodes/<archivo>») → datos crudos en bloques de 4 KB sin esperar
respuesta → `~M29` («Done saving file») → `~M602`. Quedó `pinocchio-prueba.gcode` (solo
comentarios, sin imprimir). `~M661` lista los archivos en binario (separador `::` y rutas
`/usr/data/gcodes/…`).

**Problema de la AD5X a resolver:** al darle «detener» en la pantalla no para: sigue
imprimiendo un buen rato (y si se detiene es porque algo falló). Sospechas: el firmware de
Flashforge le adelanta mucho gcode al Klipper interno, o su macro de cancelar es lenta.
Probar si `~M26` por la red para más rápido (con una impresión corta y el usuario al lado) y
revisar por SSH (puerto 22 abierto; pedir permiso y clave) los logs y la macro de cancelar.

## Dónde mirar

- Código de Orca 2.4.2 ya descargado: `~/.cargo-target/orca/OrcaSlicer-2.4.2/src/slic3r/Utils/`
  (`CrealityPrint.cpp`, `CrealityPrintAgent.cpp`, `CrealityHostDiscovery.cpp`,
  `Flashforge.cpp`, `Moonraker.cpp`).
- Lo hecho para Moonraker: `apps/desktop/src/moonraker.rs` y
  `apps/web/src/components/panels/PrinterSection.tsx`. Las pruebas con un servidor falso
  sirven de modelo para las de Creality y Flashforge.

## Fuentes

- [3d-cli: adaptador para el firmware de fábrica de Creality (subida HTTP + WebSocket 9999)](https://github.com/alex-mextner/3d-cli/issues/93)
- [OrcaSlicer #2103: subir e imprimir en la K1](https://github.com/SoftFever/OrcaSlicer/issues/2103)
- [helixscreen #1468: mantener vivo el backend de Creality en la K1](https://github.com/prestonbrown/helixscreen/issues/1468)
- [Cómo revisar el monitoreo local de K1/K2](https://printertools.app/blog/guides/troubleshoot-creality-k1-k2-local-monitoring)
