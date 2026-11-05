# Plan: laminador (slicer) con el motor de OrcaSlicer

Meter el laminado dentro de Pinocchio usando el **motor de OrcaSlicer congelado
en v2.4.2** (última estable, 2026-07-07; la 2.5.0 sigue en rc). Lo malo de Orca
es la GUI (se pega al laminar), no el motor: nos quedamos con `libslic3r` y la
interfaz es la nuestra, solo con lo que usa la K1.

Primera impresora: **Creality K1** (Klipper). Después K1C / K1 Max (mismo perfil
con otro volumen y extrusor).

## Decisiones

- **Motor en un proceso aparte** (`pinocchio-slicer`, binario C++ propio que
  enlaza `libslic3r`), no enlazado dentro de la app:
  - Orca usa OCCT 7.6 y nosotros 7.9.3: en el mismo binario chocan los símbolos.
    En otro proceso no.
  - Si el motor se cae o se cuelga, la app sigue viva; cancelar = matar el proceso.
  - La interfaz nunca se congela mientras lamina (lo que hoy falla en Orca).
- **Versión congelada**: tag `v2.4.2`. Arreglos de versiones nuevas se traen a
  mano (cherry-pick) solo si valen la pena.
- **Vista previa propia**: parser de gcode en Rust (comentarios `;TYPE:`,
  `;LAYER_CHANGE`, `;Z:`, `;WIDTH:`, `;HEIGHT:` que escribe Orca). No se usa
  `libvgcode` (OpenGL de Orca).
- **Miniaturas**: Orca las dibuja con la GUI (OpenGL). Las renderizamos nosotros
  y las insertamos en el gcode desde Rust (la pantalla de la K1 las muestra).
- Android fuera al principio (stub, como el CAD sin OCCT).

## Dependencias de libslic3r en v2.4.2

Boost 1.84, TBB 2021.5, CGAL 5.6.3 (+ GMP/MPFR), Eigen, OpenCV 4.6 (`core`),
OpenVDB (fork de tamasmeszaros) + Blosc, Draco 1.5.7, OCCT 7.6.0, Cereal, EXPAT,
PNG, JPEG, ZLIB, Qhull, NLopt, libnoise. Internas en el árbol: admesh, libigl,
mcut, libnest2d, clipper, Clipper2, miniz, glu-libtess, semver.
Orca trae su superbuild en `deps/`: se usa ese mismo, sin wxWidgets, GLFW,
FFMPEG, WebView2, CURL, OpenSSL ni Assimp (son de la GUI).

## Fases

| Fase | Contenido | Estado |
|------|-----------|--------|
| S0 | Compilar `libslic3r` v2.4.2 estático en Linux con `scripts/build-orca-engine.sh` (deps de Orca sin GUI). Prueba: cubo y benchy con perfil K1 dan el mismo gcode que Orca 2.4.2 | hecha (2026-11-04): binario de 52 MB, cubo de 20 mm → 100 capas, 16 min 38 s, 3,62 g. Falta comparar contra la GUI de Orca 2.4.2 con el mismo STL 🧪 |
| S1 | Binario `pinocchio-slicer`: entrada JSON (3MF/STL de la placa + perfiles máquina/proceso/filamento), progreso en líneas JSON por stdout, salida `.gcode` + resumen (tiempo, filamento, tiempo por tipo) | hecha (STL por pieza, un perfil ya unido; el largo por tipo lo saca `parse_gcode`) |
| S2 | Perfiles K1 / K1C / K1 Max copiados de `resources/profiles/Creality` (resolver `inherits`); procesos 0.12/0.16/0.20/0.24; filamentos genéricos PLA/PETG/ABS/TPU + Hyper PLA. Enlazar con `PrinterProfile`/`MaterialProfile` y `FabTolerances` | hecha: boquilla 0.4, 5 calidades (0.08–0.24), 13 filamentos (K1C: 60). La impresora del laminador se propone por el nombre de la de Fabricar; el material de Fabricar no elige filamento todavía |
| S3 | Crate Rust + comando Tauri `slice_plate`: toma las placas de Preparar (mm, Z arriba, ya orientadas y distribuidas), lanza el proceso, progreso y cancelar. «Preparar todo» puede terminar laminando | hecha (con vaciado y compensación como al exportar); falta que «Preparar todo» termine laminando |
| S4 | UI «Laminar» en `Print3DPanel`: calidad (preset), paredes, relleno % y patrón, soportes (no / normal / árbol; solo donde hay voladizo), brim, temperaturas, velocidades; «Avanzado» para el resto. Se guarda en el proyecto. Catálogo `en.print.ts` | hecha; falta probarla en la app abierta 🧪 |
| S5 | Vista previa en el visor: líneas por tipo (pared, relleno, soporte, puente, viaje), deslizador de capas, color por tipo/velocidad/ancho, tiempo y filamento por tipo | hecha (2026-11-04): `lib/gcodeView.ts` (cintas del ancho de línea, color por tipo, cama con cuadrícula), capa con deslizador, viajes, tipos ocultables, % y gramos por tipo. Sin color por velocidad/ancho todavía; probado en Chromium headless, falta en la app 🧪 |
| S6 | Miniaturas renderizadas por nosotros e insertadas en el gcode; guardar `.gcode` | hecha: `thumbnail.rs` (rasterizador propio, color del filamento) → el motor las escribe en PNG (96×96 y 300×300 de la K1). Falta ver que la pantalla de la K1 las muestre 🧪 |
| S7 | Enviar a la K1: Moonraker (subir + imprimir, estado y progreso). Sin root: protocolo propio de Creality | hecha en código (2026-11-05), falta probar con la K1 🧪: `apps/desktop/src/moonraker.rs` (`moonraker_info`, `moonraker_send` sube el último gcode con `print=true` opcional, `moonraker_status` con tiempo restante por la estimación del laminador, `moonraker_action` pausar/reanudar/cancelar; multipart a mano, puerto 7125 por defecto; 11 pruebas con un Moonraker falso). UI: pestaña «Impresora» del laminador (`PrinterSection.tsx`: conexión, estado cada 3 s, controles; cancelar con `ConfirmDialog`) y «Enviar e imprimir» / «Solo enviar» en el resumen. La dirección va en los ajustes de la app (`settings.moonraker`), no en el proyecto. Después: cámara, descubrir por mDNS, herramienta MCP |
| S8 | CI/release: deps de Orca en caché (como OCCT), binario como sidecar de Tauri en Linux; Windows después | hecha en los workflows (sin correr todavía): `slicer-engine.yml` (reutilizable, también a mano) compila en Ubuntu 24.04 con caché de dependencias y del binario; `release.yml` lo embebe comprimido dentro del ejecutable (`PINOCCHIO_SLICER_EMBED` al compilar; se extrae a `~/.cache/pinocchio/slicer/<id>/` la primera vez), así el binario suelto y el AppImage son un solo archivo. Antes iba como sidecar y suelto, pero el `-slicer` descargado quedaba sin permiso de ejecución (os error 13). **Windows y macOS (2026-11-05, sin correr todavía)**: matriz por plataforma en `slicer-engine.yml` (artefacto `slicer-engine-<plataforma>`), el script compila nativo como el CI de Orca (Windows: MSVC + generador de Visual Studio, carpeta corta `<unidad>:/o`; macOS arm64, destino 11.3, firma ad-hoc tras `strip`). En Windows Orca compila OCCT compartido: el motor va con sus DLL (recorridas con `dumpbin`, más la runtime de MSVC) y el embebido lleva la carpeta entera. Argumentos en UTF-8 (`boost::nowide::args`) y sin ventana de consola (`CREATE_NO_WINDOW`) |

## Trampas encontradas al compilar

- Windows/macOS: el CMake de Orca exige OpenSSL, CURL y GLFW aunque no haya GUI (libslic3r usa OpenSSL); en Linux salen del sistema, allá se compilan con las demás (`has_target` salta las que ese sistema no define). OpenSSL se configura con el perl de Strawberry, no el de Git Bash.
- Las dependencias de Orca no compilan con el GCC 16 de Arch: por eso el contenedor Ubuntu 24.04.
- Podman sin root no puede limitar CPUs (`--cpuset-cpus` falla): el paralelismo se limita con
  `ORCA_DEPS_JOBS` (parche en `deps/CMakeLists.txt`).
- La implementación de nanosvg la pone la GUI de Orca: va en `engine/nanosvg_impl.cpp`.
- El perfil se lee en una configuración vacía y se aplica sobre `full_print_config()`: leerlo
  directo sobre la completa se cae (las listas de enums por defecto no traen su tabla).

## Riesgos

- **S0 es lo pesado**: el superbuild de deps de Orca es grande (OpenCV, OpenVDB,
  CGAL, Boost). Primera compilación larga; después sale de caché.
- Tamaño: el motor embebido suma ~22 MB comprimido (52 MB sin comprimir) al ejecutable.
- Calidad a validar en la impresora física (🧪), comparando contra Orca 2.4.2
  con el mismo STL y perfil.

## Dónde está cada cosa

- `libs/slicer/engine/`: `main.cpp` (el motor), su `CMakeLists.txt` y el
  `Containerfile` (Ubuntu 24.04) donde se compila.
- `scripts/build-orca-engine.sh`: baja Orca, compila sus dependencias (solo las
  de libslic3r) y el motor dentro del contenedor (podman) y deja el binario en
  `libs/slicer/bin/` (ignorado por git). Trabajo en `~/.cargo-target/orca` (~15 GB).
- `libs/slicer/` (crate `pinocchio-slicer`): perfiles embebidos (`presets/`,
  generados con `tools/flatten_presets.py`), `build_config` (perfiles + cambios),
  `run` (lanza el motor, progreso, cancelar), `parse_gcode` (vista previa) y una
  prueba que lamina un cubo con el motor real (se salta si no está).
- `apps/desktop/src/slicer.rs`: comandos `slicer_presets`, `slicer_config`,
  `slice_plate`, `slicer_cancel`, `slicer_preview` (binario), `save_gcode`.
- `apps/web/src/components/panels/SlicerSection.tsx`: sección «Laminar» en
  Fabricar; lo elegido se guarda en `PrepSettings.slicer` (va en el proyecto).

## Abierto

- Respondido: la K1 está rooteada → S7 por Moonraker.
