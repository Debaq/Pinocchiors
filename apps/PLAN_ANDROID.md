# Plan: Pinocchio en Android

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta.

Objetivo: que el APK sirva para trabajar de verdad en el teléfono o la tablet, sobre todo
el **CAD**. En Android los archivos no tienen rutas normales: el selector entrega
`content://…` que el backend en Rust no puede abrir. Por eso los proyectos viven
**dentro de la app** (como en Shapr3D, Nomad o Onshape móvil) y solo se cruza hacia
afuera en dos puntos: **traer** un archivo (importar) y **sacar** uno (exportar o compartir).

Escritorio no cambia: todo lo de este plan va detrás de `IS_ANDROID`
(`apps/web/src/lib/platform.ts`) o de `#[cfg(target_os = "android")]`.

## Cómo queda para el usuario

1. Abre la app → **Mis proyectos**: tarjetas con miniatura, nombre y fecha. «Nuevo
   proyecto» pide un nombre y abre el trabajo vacío (Diseñar, Animar o Importar).
2. Trabaja; se **guarda solo** dentro del proyecto (también cuando la app pasa a segundo
   plano, porque Android puede cerrarla sin avisar). «Guardar» no pregunta dónde.
3. **Importar** (STL, STEP, OBJ, 3MF, nubes, imágenes): el selector de Android elige el
   archivo y la app lo **copia** dentro del proyecto; desde ahí se importa como siempre.
4. **Exportar** (STEP, STL, 3MF, PDF del plano, render): queda en el proyecto y aparece
   «Compartir» (WhatsApp, correo, slicer) o «Guardar en…» (Descargas, Drive).
5. Más adelante: **«Abrir con Pinocchio»** desde otra app o desde Descargas.

## Arquitectura

- **Carpeta de proyectos** (Rust): `app_data_dir()/Proyectos/<nombre>/` con
  - `<nombre>.pinocchio` (el mismo formato de siempre, `project.rs`),
  - `miniatura.png`,
  - `importados/` (copias de lo que se trajo de afuera),
  - `exportados/` (lo que se sacó, para compartirlo de nuevo).
  El backend sigue trabajando con **rutas normales**: no se toca `import_model`,
  `save_project`, `open_project` ni los exportadores.
- **Cruce hacia afuera** en un solo módulo Rust (`apps/desktop/src/android_files.rs`):
  - traer: `content://` → copia en `importados/` (lectura con `tauri-plugin-fs`, que
    abre `content://` en Android) y devuelve la ruta local;
  - sacar: ruta local → `content://` elegido con «Guardar en…» o el menú de compartir.
- **Un solo envoltorio en el frontend** (`apps/web/src/lib/files.ts`) para los ~29
  diálogos de hoy (`open`/`save` de `@tauri-apps/plugin-dialog`, 16 en `App.tsx`, el
  resto en `DesignStep`, `DrawingView`, `ScanEditor`, `RenderStep`, `FlatPatternView`,
  `CalibrationWizard`, `designActions`):
  - `pickOpen(opts)`: en escritorio es el `open` de siempre; en Android abre el
    selector, copia cada archivo al proyecto y devuelve rutas locales;
  - `pickSave(nombre, filtros)`: en escritorio es el `save` de siempre; en Android
    devuelve `exportados/<nombre>` sin diálogo y, después de escribir,
    `offerExport(ruta)` muestra «Compartir» o «Guardar en…».
  Así cada llamada cambia solo el import, no su lógica.
- **Proyecto Gradle**: hoy `gen/android` se genera en cada build de CI. Cuando haga
  falta tocar el `AndroidManifest` (compartir con `FileProvider`, «Abrir con»,
  permisos), se versiona `apps/desktop/gen/android` y CI deja de correr `android init`.

## Fases

### A0 — APK base ✅ 2026-10-29
- `.github/workflows/android.yml` (manual): OpenCASCADE con el NDK, APK arm64 firmado,
  pre-release propia `android.<fecha>.<sha>` que no toca la última del escritorio.
- `IS_ANDROID` oculta lo que no funciona: actualización, conector MCP y chat, GIMP,
  controles de ventana, cámara en vivo, Orizon3D (espacio y botón de la bienvenida).

### A1 — Clave fija para firmar ✅ 2026-11-12 (falta comprobar con dos APK seguidos)
Clave creada en `~/.tauri/pinocchio-android.jks` (alias `pinocchio`, contraseña en
`~/.tauri/pinocchio-android.jks.password`, SHA-256 `0C:FA:AF:BE:…:A1:B3`) y subida a los
secretos de GitHub. El usuario debe respaldarla fuera del PC.
Desinstalar la app **borra sus proyectos**. Con la clave de prueba cada APK nuevo
obliga a desinstalar, así que sin esto se pierden los proyectos.
- `keytool -genkeypair -keystore pinocchio-android.jks -alias pinocchio -keyalg RSA -keysize 2048 -validity 10000`
- Secretos: `ANDROID_KEYSTORE` (base64 del .jks), `ANDROID_KEYSTORE_PASSWORD`,
  `ANDROID_KEY_ALIAS`. Guardar el .jks fuera del repo y con respaldo: sin él no se
  puede actualizar la app nunca más.
- Hecho cuando: dos APK seguidos se instalan uno encima del otro sin desinstalar.

### Abrir y guardar archivos 🟡 2026-11-12 (en código; falta probar el APK en el teléfono)
Pedido del usuario: lo primero es poder abrir un STL y guardar. Hecho como una versión
corta de A2, A3, A4, A5 y A6 (sin Compartir, miniaturas ni renombrar/duplicar):
- `apps/desktop/src/android_files.rs` (pruebas con `cargo test -p pinocchio-app --lib android_files`):
  - traer: `android_import_uri` abre el `content://` con `tauri-plugin-fs` (solo Android),
    copia por bloques a `importados/<tanda>/` (un `.pinocchio` va a `Proyectos/`). Nombre:
    el de la URI, si no el de `/proc/self/fd/N`, si no por el contenido (STL, STEP, PLY,
    OBJ, GLB/glTF, 3MF, proyecto, imágenes). Revisa el tipo contra los filtros del diálogo.
    **Sin plugin Kotlin.**
  - sacar: `android_export_begin` recibe el `content://` de «Guardar en…»
    (`ACTION_CREATE_DOCUMENT` del plugin de diálogos) y devuelve una ruta de paso; un hilo
    espera a que el archivo deje de crecer (1,5 s quieto, máximo 30 min), lo copia al
    destino y avisa con el evento `android-export`.
  - `android_copy_to_uri` (copia de un proyecto), `android_output_dir` (carpetas),
    `android_projects_list`, `android_project_path`, `android_project_delete`.
- `apps/web/src/lib/files.ts`: `open`/`save` con la misma firma que el plugin; los 14
  archivos que usaban el diálogo solo cambiaron el import. En escritorio no cambia nada.
- `AppProjectsDialog.tsx` («Mis proyectos»): abrir, guardar una copia en el teléfono,
  borrar, «Traer desde el teléfono…». Se abre desde Abrir proyecto y desde la bienvenida.
- Guardar en Android: pide un nombre (`ConfirmDialog` con campo de texto) y guarda en
  `Proyectos/`; menú Archivo → «Guardar una copia en el teléfono…». Guardado automático
  prendido por defecto (cada 2 min) y al pasar a segundo plano.
- Compartir (2026-11-12): plugin propio `apps/desktop/android-plugin` (crate
  `tauri-plugin-pinocchio-android`, Kotlin `PinocchioPlugin.shareFile` + `FileProvider` en
  su manifiesto, solo la caché): al exportar se elige «Guardar en el teléfono…» o
  «Compartir»; proyectos: «Compartir proyecto…» y botón en Mis proyectos
  (`android_share` copia a la caché). Lo compartido se borra al día siguiente.
- Falta: barra de progreso al copiar, limpiar `importados/`, miniaturas, renombrar y duplicar.

### A2 — Carpeta de proyectos (backend) 🟡 (lista, ruta nueva y borrar hechos; falta renombrar, duplicar, miniatura)
- `projects_dir()` (`app_data_dir()/Proyectos`, se crea si falta).
- Comandos: `projects_list` (nombre, fecha, tamaño, ¿tiene miniatura?),
  `project_create(nombre)` → ruta del `.pinocchio`, `project_rename`,
  `project_duplicate`, `project_delete`, `project_thumbnail_write(png)`.
- Nombres: sin `/`, `\`, `:` ni vacíos; si ya existe, «Nombre (2)».
- Pruebas: `cargo test` en una carpeta temporal (sin Android).

### A3 — Pantalla «Mis proyectos» (Android) 🟡 (ventana simple hecha; faltan tarjetas con miniatura)
- Reemplaza la bienvenida cuando no hay trabajo abierto: tarjetas con miniatura, más
  reciente primero; «Nuevo proyecto» (nombre en un diálogo propio, nunca uno nativo).
- Mantener pulsado (o «⋯») → Renombrar, Duplicar, Eliminar (con `ConfirmDialog`).
- Sigue ofreciendo la recuperación del guardado automático.
- Volver a la lista desde el menú Archivo («Mis proyectos»), guardando antes.
- Miniatura: captura del visor al guardar (PNG chico, ~256 px).

### A4 — Guardar dentro del proyecto 🟡 (hecho en código, falta probar)
- `handleSaveProject` en Android: con proyecto abierto guarda en su `.pinocchio`; sin
  proyecto (trabajo empezado desde «Importar») pide un nombre y lo crea.
  «Guardar como» = Duplicar.
- Guardado automático **activado por defecto** en Android y además al pasar a segundo
  plano (`visibilitychange` → `hidden`), con el hash de siempre para no reescribir sin
  cambios.
- «Abrir proyecto» en Android = volver a Mis proyectos (no el selector).

### A5 — Importar copiando 🟡 (hecho en código, sin barra de progreso; falta probar)
- `lib/files.ts` con `pickOpen`; en Android, sin filtros por extensión (Android filtra
  por MIME y `.stl`, `.step` o `.pinocchio` no tienen MIME fijo: con filtro no se ve
  nada), y se valida la extensión al copiar.
- `android_files.rs`: `import_uri(uri, destino)` copia en `importados/` conservando el
  **nombre con extensión** (el formato se detecta por la extensión). Si `tauri-plugin-fs`
  no da el nombre visible, se pide con un plugin Kotlin chico (`OpenableColumns.DISPLAY_NAME`).
- Primero los diálogos principales: importar modelo (`handleLoad`), imagen de textura,
  STEP o malla en Diseñar, nube de puntos en «Desde el escaneo». Los demás después.
- Un `.pinocchio` traído de afuera se copia como proyecto nuevo en Mis proyectos.
- Archivos grandes: copia por bloques en Rust (sin pasar los bytes por el WebView) y
  barra de progreso.

### A6 — Exportar, compartir y «Guardar en…» 🟡 (hecho en código sin versionar `gen/android`; falta probar)
- `pickSave` → `exportados/<nombre>` sin diálogo; al terminar, aviso con «Compartir»
  y «Guardar en…».
- «Guardar en…»: selector de Android (`ACTION_CREATE_DOCUMENT`) → copia de la ruta
  local al `content://` elegido (escritura con `tauri-plugin-fs`).
- «Compartir»: menú de Android con `FileProvider` (plugin Kotlin propio o
  `tauri-plugin-sharesheet`); es el primer cambio que necesita el `AndroidManifest`, así
  que aquí se versiona `gen/android`.
- Lista de lo exportado en la tarjeta del proyecto, para volver a compartirlo.

### A7 — «Abrir con Pinocchio» ❌
- Filtros `VIEW` y `SEND` para `.pinocchio`, `.stl`, `.step`/`.stp`, `.obj`, `.3mf`,
  `.ply`, `.glb`: el archivo llega como `content://`, se copia (A5) y se ofrece abrir
  como proyecto nuevo o importarlo en el abierto.

### A9 — Laminador en Android ❌ (el usuario lo quiere; después de abrir/guardar)
- Compilar el motor `pinocchio-slicer` (libslic3r de Orca 2.4.2) con el NDK: Boost,
  TBB, CGAL, GMP/MPFR, OpenVDB, OpenCV, Draco, NLopt, Qhull y OCCT. Lo que no compile se
  quita junto con la función que lo usa.
- Va dentro del APK como `lib…so` en las librerías nativas (única carpeta desde donde
  Android deja ejecutar un programa) y se lanza como proceso aparte, igual que en escritorio.
- Envío a la impresora por Moonraker (HTTP en la red local): igual que en escritorio.
- Riesgo: memoria en teléfonos modestos con piezas grandes.

### A8 — Uso táctil ❌ (después de probarlo en el teléfono)
- No se planifica a ciegas: se hace una lista probando el APK (selección de caras y
  aristas, cotas, menús que dependen de pasar el mouse, paneles angostos, teclado en
  pantalla en los campos numéricos).

## Pruebas

- Backend (A2, A5 sin Android): `cargo test` de los comandos con carpetas temporales.
- Frontend: `npx tsc --noEmit`; la lógica de `files.ts` con `IS_ANDROID` simulado en la
  app web sin Tauri (mock IPC).
- APK: solo en CI (`gh workflow run Android`), nunca compilando en el PC; cada fase
  cierra probando el APK publicado en un teléfono real.

## Riesgos

- **Desinstalar borra todo**: A1 antes que nada; además «Exportar proyecto» (A6)
  permite sacar el `.pinocchio` como respaldo.
- **Android cierra la app en segundo plano**: guardado al pasar a `hidden` (A4).
- **`content://` sin nombre ni extensión**: el plugin Kotlin chico lo resuelve (A5).
- **`tauri-plugin-fs` en Android**: confirmar en A5 que abre `content://` desde Rust
  para leer y en A6 para escribir; si no, lectura y escritura en el plugin Kotlin.
- **Copias grandes** (escaneos de cientos de MB): copia por bloques y espacio libre
  revisado antes de copiar.
- **Respaldo de Android** (`allowBackup`) solo guarda 25 MB por app: no reemplaza
  exportar.
