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
| Creality K1 × 2 (una es `K1-34E2`, 192.168.50.49, pantalla 1.3.5.22) | Creality de fábrica | Protocolo propio de Creality (como Creality Print) | en la app (P2, 2026-11-06): leer probado con la real; subir e imprimir probados solo con una K1 falsa 🧪 |
| Flashforge AD5X × 2 (`.190` fw 3.1.7; `.47` fw 3.1.8, ex-zmod limpiada) | Flashforge de fábrica | API local 8898 (estado, subir+imprimir con carrete) + TCP 8899 (serie, cancelar/pausar) | ✅ en la app (P1, P3, P4); impresión real probada a mano, falta probar desde la app 🧪 |

Por qué no rootear las otras: con el root la K1 se colgaba (era GuppyScreen, ya quitada) y
no les gusta cómo quedan. Orca normal nunca pudo imprimir en las K1 de fábrica: solo el
Orca modificado por Creality (Creality Print), porque Orca solo sabe Moonraker y OctoPrint
para estas.

## Fases

| Fase | Contenido | Estado |
|---|---|---|
| P1 | **Varias impresoras**: lista con nombre, tipo (Moonraker / Creality de fábrica / Flashforge), dirección y credenciales; estado de todas a la vista; al enviar se elige a cuál. Reemplaza `settings.moonraker` (una sola dirección) migrándola. Un `trait`/enum en Rust por tipo de conexión con las mismas operaciones: info, enviar(+imprimir), estado, pausar/reanudar/cancelar | hecha (2026-11-05): lista en la pestaña «Impresora» con foto de cada modelo (`lib/printerModels.ts`, fotos de los perfiles de Orca en `src/assets/printer-*.png`), `settings.netPrinters` (migra la K1 de `settings.moonraker`). Comandos comunes `printer_info/send/status/action` (`printers.rs`). Al enviar se elige impresora y no deja mandar un gcode laminado para otro modelo |
| P2 | **K1 de fábrica (protocolo Creality)**: ver abajo. Primero solo leer el estado de una K1 libre; después subir sin imprimir; al final imprimir con el usuario al lado | en código (2026-11-06): `apps/desktop/src/creality.rs` (WebSocket propio, estado, subir, imprimir, pausar/cancelar), tipo «K1 de fábrica» en la pestaña Impresora; una K1 agregada como Klipper que responde como Creality se cambia sola de tipo. Leído con la K1-34E2 real (`creality_real`). **Falta con el usuario**: subir sin imprimir y después imprimir |
| P3 | **Flashforge AD5X**: API local. Ver abajo. El usuario tiene que activar el modo LAN y dar el número de serie y el código de la pantalla | hecha (2026-11-05): `apps/desktop/src/flashforge.rs` (estado y carretes por `/detail`, subir con `materialMappings` T0→carrete elegido, número de serie por `~M115`, cancelar `~M26` inmediato, pausar `~M25`/reanudar `~M24` sin probar en la real); pruebas con una AD5X falsa (HTTP+TCP) |
| P4 | **Perfiles AD5X en el laminador**: `tools/flatten_presets.py` hoy solo aplana Creality (`VENDOR = "Creality"`); Orca 2.4.2 trae `Flashforge AD5X 0.25/0.4/0.6/0.8 nozzle` y sus procesos (`@FF AD5X`). Primero un color; multicolor (IFS, 4 carretes) necesita que el motor lamine con varios filamentos (hoy fuerza uno: `filament_map` en `engine/main.cpp`) | hecha (un color): `flatten_presets.py` con varios fabricantes y `tools` por impresora; con `tools > 1` el laminador declara un filamento por carrete (`one_filament_per_tool`) → gcode con `T0`; el carrete se elige al enviar. `SET_PRINT_STATS_INFO` solo en Creality K1. Multicolor pendiente |
| P5 | **Encontrarlas solas en la red**: Moonraker por mDNS, Creality como `CrealityHostDiscovery.cpp`, Flashforge por UDP 48899 | pendiente |
| P4b | **Multicolor AD5X**: plan aparte en `PLAN_MULTICOLOR.md` (fases MC0–MC7) | plan escrito (2026-11-05) |
| P6 | Herramienta MCP para mandar a imprimir y ver el estado; cámara (K1 y AD5X) | pendiente |
| P7 | **Klipper autónomo** (pedido del usuario 2026-11-06: «mucha autonomía» con las Moonraker; las cerradas, lo que den): configuración desde Pinocchio (subir/editar `printer.cfg`, reiniciar, leer y explicar errores de Klipper); asistente de puesta en marcha (STEPPER_BUZZ, QUERY_ENDSTOPS, PID, PROBE_CALIBRATE, malla, pasos del extrusor, después pressure advance e input shaping); consola y macros; cola y archivos de la impresora; cámara; todo también como herramientas del chat/MCP. Leer = solo; mover, calentar, imprimir o cambiar la configuración = la IA lo pide y el usuario confirma en la app. Primera: la CR-10 Max | plan |

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
- **Estado leído de la K1-34E2** (firmware de pantalla 1.3.5.22): al conectar empuja ~12
  mensajes con todo el estado (`hostname`, `state`, `printProgress`, `nozzleTemp`, `bedTemp0`,
  `printFileName`, `modelVersion`…). `state`: 0 sin trabajo, 1 imprimiendo, 2 terminada,
  3 falló, 4 cancelada, 5 en pausa (4 visto en la real tras cancelar; el resto, de la comunidad).
- **Todo lo que acepta** (2026-11-06, de los textos de `/usr/bin/web-server.disabled` de la K1
  rooteada, compilado nov 2024, mismo programa que corre en las de fábrica). Órdenes
  `{"method":"set","params":{…}}`: `gcodeCmd` (texto: **cualquier gcode**, se lo pasa a Klipper
  por `/tmp/klippy_uds`; probado con `M220 S99`/`S100` en la K1-34E2: funciona, pero no
  devuelve la respuesta de Klipper), `nozzleTempControl` (n), `bedTempControl` ({num, val}),
  `fan`/`fanCase`/`fanAuxiliary` (n), `lightSw`, `setPosition` (texto), `autohome` (texto, «X Y Z»),
  `motorLock`, `heatersOff`, `zOffset` (texto), `autoLevel`, `manualLevel` (texto),
  `startAutoPid`, `nozzleTempAutoPid`, `bedTempAutoPid`, `resonanceTest` (texto),
  `setFeedratePct`, `setFlowratePct`, `pressureAdvance`, `smoothTime`, `velocityLimits`,
  `accelerationLimits`, `accelToDecelLimits`, `cornerVelocityLimits`, `excludeObjectsProc`
  (texto), `pause`, `stop`, `opGcodeFile` (`printprt:` imprime, `deleteprt:` borra), `cleanErr`,
  `powerLossSw`, `repoPlrStatus`, `aiSw`/`aiDetection`/`aiFirstFloor`/`aiPausePrint`,
  `materialDetect`, `enableSelfTest`, `videoElapse`/`videoElapseFrame`/`videoElapseInterval`
  (timelapse), `savePara`, `restartKlipper`, `restartFirmware`, `hostname`, `exportLog`,
  `setPrinterCfg`, y peligrosas que NO se usan: `resetSystem`, `linuxUpgrade`, `mcuUpgrade`.
- **Lecturas** `{"method":"get","params":{X:1}}` (probadas en la K1-34E2): `reqGcodeFile` →
  `retGcodeFileInfo2` (archivos con tiempo, material, peso, color, miniatura); `reqHistory` →
  `totalJob`, `totalUsageTime`, `historyList` (fecha, archivo, duración, terminado o no);
  `reqProbedMatrix` → `probedMatrix` (malla, pero mal armada por Creality: mezcla texto de
  printer.cfg); `reqPrinterCfg` → `printerCfgInfo` (subconjunto de printer.cfg: ejes, TMC,
  PID, malla, límites); `ReqPrinterPara` → `curZOffset`; `reqOtaInfo`; `reqElapseVideoList`;
  `reqPrintObjects`.
- **HTTP**: `/info`, `/upload/{archivo}`, `/downloads/gcode/{archivo}`, `/downloads/video/{archivo}`.
- `state` oficial (tabla `statusArr` de la web de la impresora): 0 detenida, 1 imprimiendo,
  2 terminada, 3 falló, 4 cancelada, 5 en pausa.
- **Formatos que usa la propia web de Creality** (`http://<ip>/static/js/app.*.js`):
  `setZOffset: "+0.05"` / `"-0.05"` (relativo; ojo, no `zOffset`), `setPosition: "X10 F3000"`
  (relativo), `autohome: "X Y Z"`, `nozzleTempControl: 220`, `bedTempControl: {num: 0, val: 60}`,
  ventiladores con `gcodeCmd: "M106 P0|P1|P2 S0-255"`, `fan`/`fanCase`/`fanAuxiliary: 0/1`,
  `lightSw: 0/1`, `setFeedratePct: n`, `excludeObjects: [nombre]`, `deleteHistory`,
  `rmProbedMatrix`, `restartKlipper: 1`, `restartFirmware: 1`, `cleanErr: 1`.
- **Versión del firmware**: es la que llega como «DWIN sw ver» en `modelVersion` (la K1-34E2
  dice 1.3.5.22 y su pantalla muestra lo mismo como versión); «printer sw ver» llega vacío y
  `/info` da la de la API (1.0.0). `creality.rs` ya la toma de ahí. La K1 rooteada está en
  1.3.3.46 (`/etc/ota_info`), más vieja.
- **Firmware de referencia: 1.3.5.22** (2026-11-06). Con esa versión funciona todo lo de
  Pinocchio con las K1 de fábrica (estado, subir, imprimir, `gcodeCmd`, lecturas): el cubito
  salió bien en la K1-34E2. El usuario va a llevar la otra K1 a esa versión. Con otra versión,
  volver a revisar la lista de órdenes antes de confiar en ella.
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

**Primera impresión real (2026-11-05, AD5X .47 tras limpiar zmod y actualizar a v3.1.8):**
cubo de 20 mm laminado con nuestro motor + perfiles AD5X de Orca (4 filamentos declarados,
herramienta cambiada a `T2`), subido por 8899 y empezado con `~M23 0:/user/<archivo>`.
Calentó bien (cama 45, boquilla 215, después 210) y empezó a imprimir, pero **tomó el
carrete 1 y no el 3**: con `~M23` el firmware no usa el `T2` del gcode para elegir carrete
(los gcodes que empiezan en T2 funcionan porque al imprimir desde la pantalla se elige el
carrete). Opciones: (a) API 8898 `uploadGcode` con `materialMappings` (necesita el código
del modo LAN de la pantalla; el 8898 está abierto en la .47), (b) solo subir y que el
usuario empiece desde la pantalla eligiendo carrete, (c) ver si `fileSlotId.json`
(`/usr/data/config`, `"slotId" : "1;2;"` por archivo) es lo que lee `~M23`.

**✅ Receta que funciona para elegir carrete (2026-11-05, AD5X .47, fw 3.1.8):** API 8898,
`POST /uploadGcode` multipart (`gcodeFile`) con cabeceras `serialNumber`, `checkCode`,
`fileSize`, `printNow: true`, `useMatlStation: true`, `gcodeToolCnt: 1` y
`materialMappings` = base64 de
`[{"toolId":2,"slotId":3,"materialName":"PLA","toolMaterialColor":"#FFFFFF","slotMaterialColor":"#FFFFFF"}]`
(**`toolId` desde 0 = la T del gcode; `slotId` desde 1 = el carrete**). Responde
`{"code":0,"message":"Success"}`; en segundos `/detail` da `status: printing` y
`matlStationInfo.currentSlot/currentLoadSlot = 3`. El gcode: laminado con 4 filamentos
declarados (listas de 4 al final) y la herramienta usada como `T2`.
- El **código LAN** (`checkCode`) está en `/usr/prog/config/backup.json` (`"lanCode"`), y el
  número de serie en `~M115`/`printerSerialNumber`: no hace falta pedírselos al usuario si
  hay SSH; si no, se ven en la pantalla (modo LAN).
- El primer intento por la API cargó el carrete 3 y se quedó quieta («se cargó» y nada):
  venía de una impresión cancelada a mitad con el filamento del 1 adentro y otra cancelada
  desde la pantalla. Tras apagar y prender funcionó a la primera. En la app: si tras cargar
  no pasa a `printing` en ~2 min, avisar y sugerir reiniciar la impresora.
- El firmware genera una clave SSH nueva en cada arranque (conectar sin guardar la clave).

**Detener por la red sí funciona al instante:** `~M26` → «CANCELING» en ~10 s y
«CANCEL_BUILD» (calefactores a 0) a los ~23 s. Desde la pantalla tarda muchísimo (queja del
usuario). Pinocchio debe ofrecer «Cancelar» con `~M26`.

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
