# Roadmap de emergencia — abrir, guardar y cerrar proyectos `.pinocchio` (2026-11-03)

Síntoma reportado: los proyectos no cargan bien. A veces se abre otra cosa, a
veces no se abre nada, y al abrirlo de nuevo sí funciona: nunca abre bien a la
primera.

## Causa principal (arreglada en `005e530`)

Al abrir, `handleOpenProject` (apps/web/src/App.tsx) restauraba la interfaz y
pasaba a **Diseñar antes de `cad.reload()`**. En ese momento el frontend todavía
no tenía documento, así que se disparaba el efecto «Entrar a Diseñar sin diseño
crea uno vacío». Ese efecto llama a `cad_new`, que en el backend **reemplaza
todas las pestañas** del proyecto recién abierto por un diseño vacío. El
resultado dependía de cuál terminaba primero: proyecto vacío, mezcla o bien. La
segunda vez ya se estaba en Diseñar con documento y el efecto no corría.

Reproducido con `~/rack.pinocchio` (ensamble activo): 1ª apertura → «Diseño 1»
vacío; 2ª → bien. Con el arreglo abren bien a la primera `rack`, `caja
reductora`, `pieza 2` y `visor`, uno tras otro.

Arreglo:
- El efecto no crea el diseño vacío mientras `switching()`.
- `cad.reload()` corre antes de `restoreProjectUi`; después se llama
  `syncDesignObjects` para sumar las piezas que falten en proyectos viejos.

**Riesgo de datos:** si se guardó (Ctrl+S o guardado automático) justo después
de una apertura que se veía vacía, el archivo quedó sin el diseño. Las copias
revisadas (`rack`, `caja reductora`, `pieza 2`, `visor`) estaban bien.

## Restos del proyecto anterior (arreglados en `535c2b8`)

| # | Problema | Arreglo |
|---|---|---|
| 1 | `ghostCache` nunca se vaciaba: los objetos en gris y la referencia de escaneo mostraban mallas del proyecto anterior (la clave `id:rev:activo` se repite entre proyectos) | Se vacía al abrir/nuevo, junto con `hiddenGhosts`, `extraObjects`, `jointsReport`, `lastJointConfig`, `textureEditor` |
| 2 | Piezas CAD fantasma o renombradas con nombres del proyecto anterior (`syncCadObjects` corría con el recálculo viejo y los objetos nuevos) | Cubierto por el reordenamiento de `005e530` |
| 3 | Las imágenes de pieles imprimibles del proyecto A se sumaban al B y se guardaban en B | `restore_model(.., replace_library)`: abrir reemplaza; cambiar de objeto/deshacer suma |
| 4 | `print3d_pieces_plain` no se guardaba ni se limpiaba: las uniones se ponían sobre piezas de otro proyecto/objeto, o fallaban tras reabrir | Se guarda en `ProjectState` (y en las copias de deshacer); `restore_model` suelta `remesh_preview` y `scene_edits` |
| 5 | Sin modelo, el árbol de escena y los materiales seguían mostrando el archivo anterior | `restoreProjectUi` los vacía cuando `model === false` |
| 6 | Recién abierto marcaba «cambios sin guardar» (la interfaz rearmada no es letra por letra el `ui.json`) | Comando `project_mark_saved` al terminar de abrir |
| 7 | Guardar y abrir corrían en hilos separados sin candado: un guardado automático a mitad de una apertura podía mezclar dos proyectos en el archivo | `AppState::project_io`: guardar, abrir, nuevo, cambiar de objeto, `project_changed` y el estado del MCP van de a uno |

Verificado: 10 pruebas de `project::` (incluida la nueva
`opening_replaces_library_and_keeps_plain_pieces` y los proyectos v1) y `tsc`.

## Pendiente

- [ ] **Probar `535c2b8` en la interfaz** (la corrida en navegador quedó sin
      hacer): abrir A → B y revisar objetos en gris, árbol de escena, título sin
      «•», uniones tras reabrir un proyecto con piezas.
- [ ] Probar en la app Tauri real (o pre-release que incluya `535c2b8`).
- [ ] Arnés e2e roto: `LiveAiBadge.tsx:27` revienta porque el puente HTTP
      (`cad_http`) devuelve `null` para `mcp_live_status`/`mcp_live_set`. Toda la
      suite e2e de Diseñar falla al arrancar. Arreglar en el puente (devolver un
      `LiveStatus`) o en el arnés.
- [ ] Revisar si hay archivos `.pinocchio` guardados vacíos por la falla (ver
      «Riesgo de datos»).
- [ ] Menor: los nodos ocultos del archivo solo se reinician si cambia
      `fileName` (dos modelos con el mismo nombre heredan los ocultos).
- [ ] Menor: apareció `~/caja reductora.pinocchio.pinocchio` (doble extensión);
      revisar de dónde sale el nombre al guardar.

## Cómo reproducir

1. `cargo build -p pinocchio-app --example cad_http` y levantarlo en un puerto
   propio (`cad_http 8777`); Vite propio (`npx vite --port 5199 --strictPort` en
   apps/web).
2. Arnés `e2e/cad-harness.html?bridge=8777` con el driver `e2e/cdp.mjs`.
   Mientras el puente devuelva `null` en `mcp_live_*`, envolver
   `__TAURI_INTERNALS__.invoke` con `Page.addScriptToEvaluateOnNewDocument`.
3. Llamar `new_project` en el puente antes de cada corrida: si queda un diseño
   de la corrida anterior, Ctrl+O se queda esperando en el diálogo de cambios
   sin guardar.
4. Abrir con `window.__nextPath = <ruta>` + Ctrl+O y leer `window.__cadStore`
   (pestañas, `doc().features`, `result().parts`).
