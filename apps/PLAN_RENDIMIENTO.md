# Plan: frontend más ágil (rendimiento)

2026-11-08. Sale de apps/ESTUDIO_RENDIMIENTO.md (léelo para el detalle y los
archivos). Empieza por el diseño CAD, que es lo que más trabajo de más hace en
cada cambio. Regla: en el PC se compilan y prueban solo los motores (crates
`libs/`); lo de la app (`apps/desktop`, `apps/web`) lo compila GitHub con la
release.

## Fases

- **R1 Huella sin copiar** (libs/cad/model). `Document::geometry_hash` clonaba
  el documento entero (con sus versiones guardadas, que son documentos
  completos) y lo pasaba a un texto JSON en cada llamada. Ahora se escribe
  directo al hash desde una vista prestada, sin copia ni texto.
- **R2 Evaluar sin rearmar el resultado** (apps/desktop/src/cad.rs). Número de
  versión del documento como clave rápida; `ensure_evaluated` separado de armar
  el `CadResult` (27 comandos lo armaban y lo botaban); la malla de la vista
  guardada por versión (hoy se re-tesela en cada `cad_mesh`).
- **R3 Una llamada por cambio** (cad.ts + cad.rs). `set_document` → `cad_mesh`
  → `cad_tool_mesh` en serie: juntar en una respuesta, o al menos en paralelo.
- **R4 Caras y aristas en lote**. `cad_face_refs` / `cad_edge_refs` /
  `cad_face_edge_refs` con listas; designActions.ts y CadView.tsx los usan.
- **R5 Pasar el mouse por el árbol sin backend**. La operación de cada cara
  viaja con la malla; el resaltado se resuelve en el front.
- **R6 Vecinos e influencia en Rust**. `neighborsOf` (Viewer3D.ts) e
  `estimateInfluence` (boneInfluence.ts) como comandos binarios con rayon.
- **R7 Datos binarios**. Zona de la piel (máscara binaria + resumen), texturas
  solo las que cambiaron, `cad_write_bytes` como cuerpo crudo.
- **R8 Ensambles y planos**. Explosión en el visor; caché por vista en planos.
- **R9 Visor**. Pincel de pesos con BVH; posar con skinning en GPU; nubes
  grandes con recoloreo incremental.
- **R10 three.js al día**. De la 0.170 a la 0.187 (la que eligió el usuario).
- **R11 Pendientes chicos** del estudio y de R3–R9.
- **R12 Aceleración GPU (WebGPU) como opción** del visor principal.

## Avance

- 2026-11-08: plan escrito.
- R1 escrito: `geometry_hash` con vista prestada + escritura directa al hash
  (libs/cad/model/src/document.rs); prueba `geometry_hash_follows_geometry_only`
  en tests/cache.rs (corriendo en el PC; resultado en
  ~/.cargo-target/bench/mallas/tests_r1.txt).
- R2 escrito (app, sin compilar en el PC: lo compila la release):
  `refresh_cache` devuelve la caché tomada; `ensure_evaluated` /
  `ensure_committed` reemplazan 27 `evaluate(..)?;` que botaban el resultado;
  `VIEW_MESH` guarda la malla de la vista por (huella, escala).
- R1 PROBADO en el PC: cache.rs 3 OK (incluye la nueva) + tabs.rs 18 OK.
- R3 escrito (cad.ts `sendNow`): malla y herramienta en paralelo
  (`Promise.all`), no en serie. La fusión en una sola respuesta queda para
  después si hace falta.
- R4 escrito: comandos `cad_face_refs` / `cad_edge_refs` (cad.rs, registrados
  en lib.rs); en cad.ts `faceRef` / `edgeRef` pasan por `batchedRefs`, que
  junta los pedidos del mismo instante en una llamada (los 35 lugares que los
  usan no cambian). `cad_face_ref` / `cad_edge_ref` de a uno siguen (los usan
  e2e y el conector MCP).
- R2–R4 sin compilar en el PC (app): los revisa la release.
- R5 escrito (versión simple): `featureFaces` en cad.ts recuerda la respuesta
  por (versión del diseño, operación); pasar el mouse otra vez por la misma
  fila no llama al backend. Mandar la operación de cada cara con la malla
  (sin backend del todo) queda para después.
- R6 escrito. Influencia de huesos: `estimate_influence` en
  libs/pinocchio/attachment/src/estimate.rs (rayon; 3 pruebas OK en el PC) y
  comando `estimate_bone_influence` (cuerpo binario crudo, commands.rs). En el
  visor la estimación llega asíncrona (`estimateInfluenceAsync`; sin backend
  usa la de TS) y mientras se ve la anterior. Vecinos del pincel: quedaron en
  TS pero con arreglos planos (`meshNeighbors` en weightPaint.ts, soldado por
  bits de la posición): mismo resultado que antes en 504 100 vértices y
  1 088 ms → 168 ms; no valía la pena mandar la malla al backend.
- R7 escrito. Zona de la piel: `ZoneInfo` trae solo el resumen (`count`,
  `protected_count`, `revision` = firma de las caras); las caras van en
  binario por `skin_zone_faces` y el front (`applySkinZone` en App.tsx) las
  pide solo si cambió la firma: pintar intensidad ya no baja las caras. El
  conector usa `zone["count"]`. Texturas: `TextureInfo.hash` (firma rápida
  del contenido, structure.rs) y `loadSceneAppearance` guarda lo bajado y
  decodificado por firma: cada cambio de malla o guardado de GIMP baja solo
  las que cambiaron (las de la piel retopologizada, `get_skin_texture`, siguen
  como antes). PNG de planos e imágenes del chat: `writeBytes`
  (lib/writeBytes.ts) con cuerpo crudo por `write_render_image`, no lista
  JSON. `cad_write_bytes` queda para cad_http.
- R8 escrito. Ensambles: `assembly_view` (cad.rs) guarda malla y rangos del
  ensamble armado por (huella, ensamble, escala) y los dos pedidos
  (`cad_assembly_mesh` + `cad_assembly_faces`) comparten el cálculo; los
  rangos traen también las aristas de cada instancia. El deslizador de
  explosión ya no va al backend: `explodeAssemblyMesh` (cad.ts) corre las
  piezas en el front. `cad_assembly_mesh` con `explode` > 0 sigue para e2e y
  el conector. Planos: DrawingView recuerda cada vista (y la imagen de las
  sombreadas) por (versión del diseño y del ensamble, pedido) y pide solo las
  que cambiaron. Las vistas siguen viajando en JSON (el binario no hizo falta
  con la caché).
- R9 escrito. Pincel de pesos y de intensidad: `VertexGrid` (weightPaint.ts,
  rejilla armada una vez por malla y al moverse los vértices) en vez de
  recorrer todos; mismo resultado y 7× más rápido con pinceles de tamaño
  normal (500 k vértices). Posar: se quedó en CPU a propósito (elegir, pintar
  y medir leen la malla posada; en GPU habría que rehacer todo eso):
  `skinPose` (poseSkin.ts) con una matriz por hueso, mismo resultado exacto,
  ~20 % más rápido (V8 ya optimizaba bien el bucle viejo). Nubes: el pincel
  usa `ScreenGrid` (cloudBrush.ts) sobre la proyección del trazo y recolorea
  solo lo que cambió, subiendo a la GPU solo ese tramo (3 M de puntos: 25 →
  3,7 ms por tramo); borrar o conservar la selección va en binario
  (`scan_cloud_edit_selection`). Queda en JSON lo que devuelve «solo
  seleccionar» de los filtros de ruido.
- R9 terminado: posar con vista previa en GPU. Mientras se arrastra una
  articulación, `showPosePreview` (Viewer3D.ts) muestra una `SkinnedMesh` que
  comparte los atributos de la malla visible (o de la vista de pesos) con las
  posiciones de reposo; cada hueso suelto lleva la matriz de su segmento
  (`DetachedBindMode`, sin pasar el peso de los que no deforman, igual que en
  CPU). 200 ms después del último movimiento, al confirmar el giro o al
  soltar el gizmo, `settlePose` calcula una vez en CPU (`skinPose`) y vuelve
  a la malla de siempre: elegir, pintar y medir siguen leyendo la malla
  posada. Mientras se arrastra no se ve el alambre.
- R10: three 0.170 → 0.187.0 y @types/three 0.187.0 (package.json y
  package-lock con `--package-lock-only`; npm 12 pidió `--allow-remote=all`
  por un tarball de Tailwind del registro oficial). De la guía de migración
  solo tocó `PCFSoftShadowMap` → `PCFShadowMap` (renderStill.ts; desde r182
  ya es suave). Revisado contra el código de la 0.187: los chunks que
  parchean dqSkinning.ts y autoSmooth.ts no cambiaron, `Skeleton.update` lee
  la matriz de los huesos tal cual; `setViewport`/`setScissor` solo cambian
  con un render target puesto (ViewCube dibuja en el lienzo). Cambios
  visuales esperables: reflejos PMREM (r181, r187) y `RoomEnvironment`
  (r183) un poco distintos.
- R11 escrito:
  - `VertexGrid.nearest` (vértice más cercano por anillos de celdas, mismo
    resultado que recorrer todo): `nearestNormal` (girar huesos hacia la
    normal) y el imán a vértices del modelo propio usan la rejilla (antes
    recorrían la malla, el imán en cada movimiento del gizmo y salteando
    vértices en mallas grandes; ahora exacto). El imán a otros objetos sigue
    como antes.
  - `boundsAfter` (placement.ts) sin un vector por vértice; no se pasó a Rust
    (son acciones de un clic y mandar la malla costaría más).
  - `shadeView` (drawingShade.ts): `Math.max(...posiciones)` podía romper con
    mallas grandes (demasiados argumentos); ahora un bucle, y el color por
    triángulo sin crear arreglos.
  - Texturas de la piel retopologizada: `get_skin_texture_hashes` y caché por
    firma en `refreshSkin` (como las de la escena en R7).
  - «Solo seleccionar» de los filtros de la nube: `scan_cloud_select` en
    binario (cuántos, nota y los índices).
  - Árbol del CAD: `cad_face_tags` trae las operaciones de todas las caras
    en una llamada por versión; `featureFaces` resuelve cualquier fila en el
    front (R5 completo).
  - No se hizo (no valía): `recordableArgs` (solo copia ajustes chicos de
    unos pocos comandos) y fusionar malla + herramienta en una respuesta (ya
    van en paralelo; ahorraría ~1 ms y obliga a tocar `apply`).
- R12 (propuesta): WebKitGTK 2.54 trae WebGPU compilado como función
  experimental, apagado. Opción «Aceleración GPU (experimental)» en
  Preferencias que encienda la función en el WebView (Linux) y use
  `WebGPURenderer` de three (con WebGL2 de respaldo). Requiere: reescribir en
  TSL los parches de shader (dqSkinning, autoSmooth, xray y otros
  `onBeforeCompile`), pasar el posproceso (EffectComposer, GTAOPass,
  UnrealBloomPass) al de nodos, y `Line2` a su versión de nodos. Windows
  (WebView2) y macOS ya traen WebGPU encendido.
- R12 escrito (primer paso: visor principal; el CAD sigue en WebGL).
  Preferencias → Visor → Pantalla → «Aceleración GPU (experimental)»,
  apagada por defecto; apagada, todo queda como antes (ni se carga
  `three/webgpu`, va en un pedazo aparte). Encendida:
  - `startGpu` (lib/gpuRenderer.ts) corre antes de montar la interfaz
    (index.tsx): si no hay `navigator.gpu`, pide `enable_webgpu` (lib.rs:
    en Linux busca con `dlsym` las funciones de WebKitGTK ≥ 2.42 para
    encender la función «WebGPU»; con uno más viejo no hace nada) y recarga
    una vez; después carga `three/webgpu` (comparte `three.core.js` con
    `three`, sin clases duplicadas).
  - `createViewRenderer` da `WebGPURenderer` (WebGPU, o su respaldo WebGL2
    si el sistema no lo trae) con la forma que usa el visor: no dibuja ni
    limpia hasta que arranca (`render` antes de `init` tira error) y avisa
    para el primer cuadro; `capabilities.getMaxAnisotropy` y
    `forceContextLoss` adaptados.
  - Reflejos: `environmentFromScene` espera a que arranque y usa el
    `PMREMGenerator` de nodos (LightRig redibuja al llegar).
  - Líneas del laminador: `thickLineMaterial` / `thickLineSegments` dan
    `Line2NodeMaterial` y la `LineSegments2` de webgpu.
  - Render de imagen: un `WebGLRenderer` aparte con sus propios reflejos
    (renderStill usa render targets, sombras y posproceso de WebGL).
  - Sin halo de selección (posproceso de WebGL). Los parches de shader
    propios (suavizado automático, cuaterniones duales) no se aplican.
  - Con WebGPU de verdad, los puntos de las nubes se ven de 1 px (WebGPU no
    tiene puntos más grandes; con el respaldo WebGL2 sí).
  - El panel muestra con qué dibuja («Ahora: WebGPU» o «sobre WebGL2») o por
    qué no se pudo.
  - Siguiente paso posible: el CAD (CadViewer: líneas gruesas, tapas de
    corte con stencil) y reescribir en TSL el suavizado automático.
- Herramienta `viewer` del conector (modo en vivo), para probar todo esto en
  la app real: `diagnostics` (dibujador, FPS, lo cargado y los tiempos de
  cada llamada al backend y de cada acción pesada del visor: lib/perfLog.ts
  envuelve el `invoke` interno de Tauri y el visor anota pinceladas, pose,
  rejilla, vecinos, influencia y cuadros), `capture` (PNG del visor tal como
  se ve), `gpu` (prender/apagar la aceleración GPU, recarga), `benchmark`
  (`frame`, `brush`, `influence`, `pose`; no cambia el trabajo) y `explode`
  (deslizador del ensamble). Pregunta `viewer` en live.rs → mcpLive.ts →
  `liveViewerAction` en App.tsx.
- Todas las fases R1–R12 escritas. R1 y el motor de R6 probados en el PC; lo
  de la app (R2–R12) sin compilar: lo revisa la próxima release.
- Siguiente: commit, release y probar R2–R12 en la app (lista en «Para
  probar»).

## Para probar en la app

- Diseño CAD: editar, pasar el mouse por el árbol, elegir varias caras (R2–R5).
- Esqueleto sin pesos con «ver pesos»: la vista previa aparece y sigue al
  mover articulaciones (R6).
- Pincel de pesos y de intensidad en modo suavizar, con una malla grande
  (R6, R9); posar un hueso con pesos: arrastrar fluido, al soltar la malla
  queda igual y se puede pintar/elegir sobre ella; con la vista de pesos
  encendida también (R9).
- Todo el visor tras subir three (R10): materiales, reflejos, sombras del
  render, gizmos, líneas gruesas, animación con cuaterniones duales.
- Fabricar → piel: elegir zona, proteger, ampliar, invertir, pintar
  intensidad; el panel muestra las caras y el visor las resalta (R7).
- Modelo con texturas: cambiar la malla y editar en GIMP (R7).
- Plano: guardar PNG de una vista sombreada; chat: guardar imagen y
  conversación (R7).
- Ensamble: mover el deslizador de explosión; elegir caras de una instancia
  explotada; planos de un ensamble (R8).
- Planos: cambiar una vista, ocultar otra, mover el corte (R8).
- Nube de puntos: pincel, borrar y recortar a la selección (R9); «solo
  seleccionar» de quitar ruido, puntos sueltos, fragmentos y mesa (R11).
- Girar huesos hacia la normal; imán a vértices con el gizmo (R11).
- Retopología con texturas: cambiar algo y que las texturas sigan bien (R11).
- Plano con vista sombreada de una pieza grande (R11).
- Aceleración GPU (R12): encenderla, «Recargar ahora», ver qué dice el panel
  (WebGPU o WebGL2); con ella: abrir modelos pesados, texturas, pesos y
  pincel, posar, animar, nubes de puntos, vista previa del laminador (líneas),
  render de imagen (sale como siempre), capturas/miniaturas, vistas cuádruples
  y el cubo de vista. Apagarla y comprobar que todo vuelve a lo de antes.
