# Plan: conector MCP para cualquier IA

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta.

Objetivo: que cualquier IA que hable MCP (Claude Code, Claude Desktop, Cursor, Codex,
Gemini CLI, LM Studio…) pueda usar **toda la app**: diseñar piezas, ensamblar, hacer
planos, importar y convertir mallas, repararlas, remallarlas, prepararlas para imprimir,
poner esqueleto y pesos, posar, animar, renderizar y exportar. Funciona en **dos modos**
con las mismas herramientas:

- **Sin ventana** (por defecto): el servidor MCP lleva su propio motor; abre y guarda
  proyectos `.pinocchio` y escribe archivos.
- **Con la app abierta**: si la app está abierta y lo permite, el servidor le manda los
  comandos a ella y los cambios se ven en vivo en la ventana.

## Qué lo hace «muy claro» para una IA

1. **Pocas herramientas, por tarea**, no los 235 comandos internos: unas 60, agrupadas
   por prefijo (`project_`, `cad_`, `sketch_`, `asm_`, `drawing_`, `mesh_`, `print_`,
   `rig_`, `anim_`, `render_`, `scan_`). Cada una hace algo completo («hacer una
   extrusión de este sketch 10 mm»), no un paso suelto del frontend.
2. **Descripción con todo lo necesario**: qué hace, cuándo usarla, unidades (mm, grados,
   Z arriba), qué devuelve y un ejemplo de argumentos. Esquema JSON estricto
   (`additionalProperties: false`, `enum` en las opciones, mínimos y máximos).
3. **Nombres y descripciones en inglés**: es lo que mejor entiende cualquier modelo. Los
   textos que el usuario ve (nombres de piezas, operaciones) siguen en el idioma de la app.
4. **Referencias por nombre**, no solo por número: pestaña «Soporte», operación
   «Extrusión 1», cara `top` / «la cara de arriba de Extrusión 1». Cada resultado dice
   los ids y nombres nuevos para poder seguir.
5. **Cada resultado trae un resumen en texto** («Pieza Soporte: 42×20×10 mm, 8,4 cm³,
   sin errores») **más los datos en `structuredContent`**. Los errores vuelven con
   `isError` y una pista de cómo arreglarlo («el sketch no está cerrado: faltan unir los
   puntos 3 y 7»).
6. **Ver lo que hizo**: `view` devuelve una imagen PNG de la pieza, ensamble, malla o
   pose desde la vista pedida, para que la IA compruebe su trabajo.
7. **Deshacer**: `undo` / `redo` en los dos modos.
8. **Guía incluida**: recurso `pinocchio://guide` (flujo de trabajo, convenciones,
   ejemplos completos) y `prompts` armados («diseñar una pieza», «preparar para
   imprimir», «riggear un personaje»); hoy hay `get_started`, el resto en M9. `instructions` del `initialize` resume lo básico.
9. **Seguro por defecto**: no escribe fuera de las rutas que se le piden, no pisa un
   archivo sin `overwrite: true`, y el modo en vivo se activa a mano en la app.

## Arquitectura

- El conector vive en la app (`apps/desktop/src/mcp/`): `Pinocchio --mcp` habla MCP por
  stdio sin ventana. `apps/mcp` queda como binario suelto de desarrollo y sus pruebas.
  Protocolo (JSON-RPC 2.0, versiones 2025-06-18, 2025-03-26 y 2024-11-05) escrito a mano
  con `serde_json`: no hay SDK de MCP sin conexión y son pocos mensajes. Registro a stderr.
- **Motor**: `Backend::call` → `cad::bridge::try_dispatch` con el `AppState` de la app.
  Un comando que la herramienta necesita y que pide `AppHandle` se separa en
  `*_impl(&AppState, …)` y el comando de Tauri queda como envoltorio; se hace por fase,
  cuando hace falta, no todos de una vez.
- **Modo en vivo** (M8, `mcp/live.rs`): la app escucha en 127.0.0.1 con clave y deja
  `mcp-live.json`; el conector (`Auto`) lo busca en cada herramienta y, si la app está,
  le pide turno y le manda los comandos; si no, usa su propio motor.
- Las herramientas se describen en una tabla (`mcp/tools/`: nombre, descripción, esquema,
  `Effect`) para que la lista, la guía y las pruebas salgan del mismo lugar.

## Hoja de ruta

- Hecho: M0, M2, M3, M4 (2026-10-24: CAD completo — diseño, ver y medir, ensambles y
  planos), M8 modo en vivo, activación desde la app y chat dentro de la app (2026-10-25),
  M5 mallas e impresión y M7 render y escaneos (2026-10-25). M1 en lo que el CAD, las
  mallas y la impresión necesitan.
- M4 completo (2026-10-25): láminas de la app, cortes, detalles, cotas puntuales y
  relaciones de engranes/tornillo.
- **Pausado aquí (2026-10-25, decisión del usuario).** Claude Code ya carga las
  herramientas del conector a pedido (búsqueda de herramientas diferidas), así que con
  él el tamaño de la lista no pesa. Queda para después: M6 (solo en vivo, ver su fase),
  M9, cancelar a mitad un comando largo y el ahorro de tokens de abajo.

### Ahorro de tokens (trabajo futuro, medido 2026-10-25)

- `tools/list`: 52 herramientas, ~84 KB ≈ 21 000 tokens. Las más pesadas:
  `drawing_export` y `drawing_sheet` (~1 300 cada una, repiten casi los mismos
  argumentos), `sketch_create` (~870), `cad_primitive` (~790), `sketch_add` (~725),
  `print_prepare` (~710), `cad_extrude` (~660), `view` (~640).
- Donde más cuesta: el chat de la app con proveedores por API (`mcp/agent.rs`) manda las
  52 funciones en cada paso (hasta 60 por respuesta). La caché de prefijo de algunos
  proveedores lo abarata, pero igual cuenta. Otros clientes que no difieren
  herramientas (Cursor, Claude Desktop…) pagan la lista entera en cada pedido.
- Ideas, de más a menos rendimiento:
  1. Herramientas por área en `agent.rs`: las básicas siempre y las de CAD, mallas o
     impresión cuando la tarea las pide (una herramienta que «abre» un área). De
     ~21 000 a ~5 000–8 000 tokens por paso.
  2. Descripciones más cortas: los ejemplos largos a la guía (`help`), y
     `drawing_sheet`/`drawing_export` con un esquema común más corto. ~30 % menos.
  3. Imágenes más chicas por defecto: `view` 800×600 ≈ 600 tokens; 640×480 la mitad.
  4. No mandar el resultado dos veces (texto y `structuredContent`) a clientes que le
     pasan los dos al modelo.

### Próximo (orden propuesto, 2026-10-25)

1. ✅ **Pestaña de la IA a la vista en vivo** (2026-10-25): cada herramienta activa su
   pestaña en el motor y la ventana la sigue (suelta la selección, como un clic en otra);
   al terminar (fin de la respuesta del chat, o 30 s sin pasos con un cliente de afuera)
   vuelve a la última que eligió el usuario, si después del último paso no se movió, no
   tocó la ventana (fuera del chat) ni está editando (`mcpLive.ts`).
2. ✅ **Errores de la IA juntados** (2026-10-25, `mcp/errors.rs`): el servidor (chat,
   proveedores y clientes de afuera) anota en `mcp-errors.json`, junto a `mcp-live.json`,
   cada herramienta que falla por lo que pidió la IA (esquema o pedido imposible; no el
   turno negado ni la cancelación), agrupado por herramienta y mensaje sin números ni lo
   que va entre comillas, con cuenta, fechas, último ejemplo y argumentos. Configuración ▸
   Conector para IA ▸ Errores de la IA: los más repetidos, Copiar (tabla Markdown para
   revisarlos) y Borrar. Cada uno se arregla en la herramienta, no en el prompt (ver M9).
3. ✅ **Avance real de los comandos largos** (2026-10-25, M1): `try_dispatch_with` le da a
   los comandos con `Channel` (importar, pieza a malla, remallar) uno que reenvía su avance
   (el comando corre en otro hilo con `thread::scope`, porque el `Channel` pide `'static`);
   `Backend::call_with` lo lleva a `notifications/progress`. En vivo el puente manda
   líneas `{"progress", "message"}` antes de la respuesta y la ventana lo muestra en el
   indicador («IA: Importando… 42 %»). Falta, con M5: cancelar a mitad un comando largo
   (hoy la cancelación se ve entre comandos).
4. ✅ **Prueba real** (2026-10-25): app Tauri (debug, X11) en vivo con Claude Code como
   cliente. Visto: la ventana sigue la pestaña nueva de la IA y a los 30 s vuelve a la del
   usuario; si el usuario toca la ventana entre pasos (Ctrl+Z deshace el paso de la IA) se
   queda donde está; errores forzados con el binario nuevo quedan agrupados en
   Configuración, que los muestra con su mensaje real, y Borrar los quita. Sin probar a
   mano: el regreso al terminar una respuesta del chat de la app (gasta una respuesta de
   la IA) y el avance en vivo (ninguna herramienta usa todavía un comando largo; lo
   cubren las pruebas).

## Activación desde la app ✅ 2026-10-25

- El conector vive en la app (`apps/desktop/src/mcp/`): `Pinocchio --mcp` habla MCP por
  stdio sin ventana; no hace falta empaquetar otro binario (`apps/mcp` queda como binario
  suelto de desarrollo y sus pruebas). `--mcp --guide` imprime la guía para agentes.
- Configuración ▸ **Conector para IA**: el comando que lanzan los clientes (la app o el
  AppImage), conectar/desconectar Claude Code (`~/.claude.json`), Claude Desktop, Cursor,
  Gemini CLI (JSON `mcpServers`) y Codex (`[mcp_servers.pinocchio]` en TOML), con copia
  `.pinocchio.bak` antes del primer cambio, escritura atómica y sin tocar un archivo
  ilegible; aviso si quedó registrada otra versión de la app. Descargables: `mcp.json`,
  la guía en Markdown y la skill de Claude Code (instalar en `~/.claude/skills/pinocchio`
  o guardar).
- Pruebas: JSON conserva lo demás y respalda; JSON roto no se toca; bloque de Codex se
  agrega, reemplaza y quita; la guía lista todas las herramientas.

## Chat dentro de la app ✅ 2026-10-25

- Panel «Chat con IA» acoplado a la izquierda del visor (botón de varita en la cabecera,
  junto al indicador de IA en vivo y Exportar; ancho arrastrable y recordado).
- Usa el Claude Code instalado (`claude -p`, con la cuenta del usuario, sin clave de
  API) con el conector en vivo, en una carpeta propia y solo con la configuración de ese
  proyecto (`--setting-sources project`, `--strict-mcp-config`): sin hooks ni servidores
  del usuario; solo puede usar las herramientas de Pinocchio. `mcp/chat.rs` traduce los
  eventos `stream-json` a mensajes (texto, herramienta con su etiqueta, resultado,
  imágenes de `view`, modelo y costo estimado); continúa la conversación con `--resume`.
- Modelo (Fable, Opus, Sonnet, Haiku o el de Claude Code) y esfuerzo (bajo…máximo),
  recordados. La conversación queda guardada (sigue la sesión con `--resume`), con
  tokens y costo por respuesta y total; copiar cada línea, descargar imágenes, guardar
  la conversación en Markdown, adjuntar o pegar imágenes (van por `--input-format
  stream-json`).
- **Proveedores con API** (`mcp/agent.rs`): cualquier API compatible con OpenAI —
  DeepSeek, OpenAI, OpenRouter, Ollama y LM Studio locales, u otra—. El agente vive en
  la app: manda la conversación con las herramientas del conector como funciones,
  ejecuta lo que pide el modelo con el mismo servidor MCP (en vivo, por turnos, con el
  deshacer de la ventana) y repite (hasta 60 pasos). Con modelos que ven, las imágenes
  de `view` y las adjuntas van como `image_url`. Se configuran en Configuración ▸
  Conector para IA ▸ Proveedores para el chat (clave en `providers.json`, 0600; Probar
  manda un mensaje mínimo); en el chat se elige el «Motor». HTTPS con `reqwest` +
  rustls + `ring` (sin aws-lc) y los certificados del sistema. Pruebas: API falsa
  (pide una herramienta, recibe el resultado, contesta; tokens) y, a mano, HTTPS real
  (`--ignored`).
- Arreglos de ese día: guardar con un ensamble activo (antes «nada que guardar», y al
  cerrar no avisaba del trabajo sin guardar); la IA tiene su propio foco de pestaña y
  escribe en la pestaña nombrada (`cad_set_tab_document`), y la ventana no cambia de
  pestaña durante su turno.

## Fases

### M0 — Protocolo y esqueleto ✅ 2026-10-24
- Crate `apps/mcp`, bucle stdio, `initialize` con `instructions`, `tools/list`,
  `tools/call`, `ping`, errores JSON-RPC, cancelación y avance (`notifications/progress`).
- Herramientas `status` (modo, versión, proyecto abierto) y `help` (guía por tema).
- Pruebas: saludo completo y llamada de ejemplo con un cliente de prueba en Rust.
- `README` con la configuración para Claude Code (`claude mcp add`), Claude Desktop,
  Cursor y un cliente genérico.
- Hecho: `protocol.rs` (versiones 2025-06-18, 2025-03-26 y 2024-11-05; a los clientes
  viejos los datos les van como texto), `schema.rs` (esquemas y verificación con errores
  que nombran el campo y las opciones), `server.rs` (una herramienta a la vez en su hilo;
  `ping` y cancelación mientras tanto; un cambio que falla o entra en pánico se revierte),
  `guide.rs` (instrucciones, temas, recurso `pinocchio://guide`, prompt `get_started`),
  `README.md` (Claude Code, Claude Desktop, Cursor, Gemini CLI, Codex). Pruebas
  `tests/protocol.rs`: saludo y lista completa (nombres, descripciones, esquemas cerrados),
  clientes viejos, errores con pista, guardar/abrir/deshacer, recursos y el binario real
  por stdio.

### M1 — Anfitrión sin ventana 🟡 2026-10-24
- Hecho: `Backend` (`Local` con el `AppState` de la app; `Live` en M8) sobre
  `cad::bridge::try_dispatch` (el `dispatch` de siempre, que ahora dice si el comando no
  existe). El `dispatch` ya cubría CAD, importar/exportar, remallar, impresión, objetos y
  copias; se suman `new_project`, `open_project`, `save_project` (`*_impl` sin
  `AppHandle`) y `workspace_summary`. Deshacer del conector: copias del estado entero
  (`project::snapshot_state` / `restore_state`), 20 pasos; abrir y nuevo también se
  deshacen y devuelven la ruta. Herramientas `project_new`, `project_open`,
  `project_save` (no pisa otro archivo sin `overwrite`), `undo`, `redo`.
- Falta: el resto de los comandos sin `AppHandle` se separa en cada fase, cuando la
  herramienta lo necesita (rig, pesos, UV, texturas, render, nube), y el avance real de
  los comandos largos (hecho 2026-10-25, Próximo 3).

### M2 — Diseño CAD ✅ 2026-10-24
- Hecho: `cad_tab_create` / `cad_tab_rename` / `cad_tab_delete`; `cad_design` (operaciones
  con estado y valores principales, sketches con su estado y regiones, parámetros,
  fórmulas, piezas; `detail` da el JSON de cada operación); `cad_geometry` (caras y
  aristas numeradas con origen —«Extrusión 1 · end»—, tipo, normal, centro, área, largo,
  radio; filtros `feature`, `part`, `facing`); `sketch_create` / `sketch_add` (figuras
  rectángulo, círculo, línea, polilínea, arco, polígono, ranura y punto; restricciones con
  nombres de esta llamada —«s0.top», «s1.center», «origin»— en vez de ids; informa ids,
  si está definido y el perfil que se extruye vs. regiones dentro de agujeros);
  `cad_extrude`, `cad_revolve`, `cad_primitive` (caja, cilindro, cono, esfera, toro,
  tornillo, tuerca, arandela), `cad_fillet`, `cad_chamfer`, `cad_shell`;
  `cad_add_feature` (cualquier operación en JSON con `{"$face": n}` / `{"$edge": n}` /
  `{"$vertex": [x,y,z]}`), `cad_edit_feature` (renombrar, suprimir, mezclar campos),
  `cad_delete_feature` (con o sin dependientes), `cad_parameters` (parámetros y
  fórmulas por ruta de campo), `cad_export` (STEP, STL, 3MF, OBJ, PLY, GLB).
- Guía: temas `cad`, `sketch` (cada restricción con ejemplo) y `features` (cada tipo de
  operación con ejemplo); una prueba verifica que todos los ejemplos son JSON válido.
- App: comando `cad_geometry` (caras y aristas del sólido con pieza y origen).
- Las herramientas corren de a una y en el orden en que llegan (cola con un hilo); una
  cancelación alcanza también a lo que espera en la cola.
- Pruebas `tests/cad.rs`: placa con agujero totalmente definida por nombres, extrusión con
  el volumen exacto, cara de arriba por `facing`, redondeo, redondeo imposible que no deja
  nada, parámetro con fórmula, corte con primitiva, borrar con dependientes, editar por
  JSON y deshacer, exportar STEP sin pisar; JSON crudo (simetría, vaciado por número);
  pestañas por nombre; ejemplos de la guía.

### M3 — Ver y medir ✅ 2026-10-24 (diseños; mallas y poses en M5/M6)
- `view`: PNG del diseño con un rasterizador propio (`render.rs`, sin ventana ni GPU):
  proyección ortográfica, supermuestreo 2×, sombreado, aristas visibles con prueba de
  profundidad, un color por pieza, triedro X/Y/Z y el **número de cada cara visible**
  (en su punto más adentro, por transformada de distancia, cerca del centro) o de cada
  arista visible; `highlight_faces` / `highlight_edges` en naranja para confirmar antes de
  usar. Vistas iso, iso_back, iso_below, top, bottom, front, back, left, right, o
  azimut/elevación. Los números son los de `cad_geometry` (prueba).
- `measure`: una o dos cosas (cara, arista, punto): tipo, área, largo, radio, centro,
  dirección; distancia mínima con sus puntos y ΔX/ΔY/ΔZ, distancia entre centros y ángulo
  (sobre `cad_measure`).
- `describe_faces` quedó en `cad_geometry` (M2).
- Pruebas: cubo en iso numera sus 3 caras visibles y en planta 1; arista oculta sin
  número; con OCCT, la vista superior numera justo la cara `facing +Z` y las cuentas de
  caras/aristas coinciden con `cad_geometry`; medidas entre caras y desde un punto.

### M4 — Ensambles y planos ✅ 2026-10-24 (completo 2026-10-25)
- `asm_insert` (piezas de una pestaña de diseño, o un ensamble entero como subensamble;
  posición, giro eje+ángulo, copias, fija, configuración; nombres por pestaña: «Base 1»),
  `asm_mate` (8 tipos; conectores desde caras del ensamble: plana → centro y normal,
  cilíndrica → eje; si no se cumple con las demás, falla y no deja nada), `asm_instance`
  (mover, fijar, renombrar, configuración, quitar con sus relaciones), `asm_mate_edit`
  (ángulo/distancia impuestos, flip, tipo, quitar), `asm_check` (relaciones, grados libres,
  choques, holguras, avisos de engranes y lista de materiales).
- `cad_design`, `cad_geometry`, `view` y `measure` aceptan pestañas de ensamble (caras
  numeradas de todo el ensamble, instancia por instancia).
- `drawing_export`: lámina propia en Rust (no los planos editables de la app): vistas
  frente/superior/lateral alineadas en primer (ISO) o tercer ángulo (ASME) e iso, líneas
  ocultas exactas a trazos (`cad_drawing`), cotas generales, escala normalizada que entra
  (o la pedida), cajetín con símbolo de proyección, A4/A3/A2; en ensambles, lista de
  materiales y globos repartidos alrededor de la iso. PDF (`cad_write_pdf`) o SVG, con
  vista previa PNG (resvg) para que la IA la mire.
- App: `cad_assembly_geometry`; `cad_measure` en un ensamble mide las instancias en su
  lugar (antes medía el cuerpo de diseño sin colocar).
- Completado 2026-10-25: la lámina del conector entiende lo mismo que la de la app —
  seis vistas e iso ubicadas por el diedro, corte A-A del frente (`section`), cortes por
  una línea simple o escalonada sobre una vista (`cuts`, rayados; prisma y dirección
  portados de `drawingCuts.ts`), detalles ampliados recortados al círculo (`details`),
  cotas puntuales (`dims`: largo, diámetro, radio, distancia, ángulo; como
  `userDimensions`, por puntos del modelo o números de arista) y hoja Carta.
  `drawing_sheet` crea, cambia y borra las láminas de la app en el documento
  (`drawing.sheets`, la forma de `drawingDoc.ts`: refs y centros en coordenadas de la
  vista, lo oculto se conserva) y `drawing_export {drawing}` dibuja una de ellas.
  `asm_relation`: engranes (razón), piñón-cremallera, tornillo y `auto` (las que sugiere
  `suggest_gear_relations`), con los avisos de ejes de engranes.
- Falta: vistas personalizadas, medios cortes, roturas y vistas sombreadas de la app no
  se dibujan en el PDF del conector (quedan en la lámina); lo dibujado a mano tampoco.
- Pruebas: eje en placa (cilíndrica → 2 grados, centrado → 1), misma instancia
  rechazada, choques y lista, medir en el ensamble (0 mm entre eje y agujero), copias y
  quitar, PDF del ensamble con lista, SVG en tercer ángulo 2:1 con ocultas, sin pisar.

### M5 — Mallas, conversión e impresión ✅ 2026-10-25
- Trabajan sobre el modelo del **objeto activo** de la escena. Importar, un escaneo o
  una pieza del diseño dejan un **objeto nuevo** (los demás quedan): sin ventana el
  conector lleva la lista de objetos en la interfaz del proyecto como la app (casillero
  con la interfaz y el historial del que deja de estar activo), así el `.pinocchio`
  guardado se abre con todos; en vivo lo hace la ventana (`importModel`, `showAiPart`).
- `mesh_import` (GLB/glTF, OBJ+MTL, STL, PLY, texturas extra), `mesh_from_part` (pieza
  de una pestaña de diseño → objeto `cad` con su huella; otra vez actualiza el mismo),
  `mesh_info` (medidas Z arriba, volumen, cerrado, arista media, objetos; `diagnose`
  cuenta defectos), `mesh_repair`, `mesh_remesh` (simplify, smooth, isotropic, voxel,
  quads), `mesh_uv` (desplegar y hornear), `mesh_export` (GLB/glTF con Draco y
  simplificado, OBJ, STL, PLY, 3MF, USDZ/USDA; no pisa sin `overwrite`).
- `print_check` (impresora y material por nombre o medidas: estanco, paredes, voladizos
  y soporte, si cabe y cuántas piezas, gramos), `print_scale` (factor, caja, por eje o
  volumen), `print_orient` (apoya en la mejor cara: `placement_floor` en Rust, como
  `floorMatrix`), `print_cut` (grid, optimal, zlayers, manual con planos),
  `print_joints` (7 formas, holgura por ajuste), `print_plates` (camas, vaciado con
  drenaje, contracción) y `print_prepare` (todo en un paso con informe por paso).
  `print_hollow` quedó dentro de `print_plates`/`print_prepare` (`hollow_wall`).
- `view` con `show: "model"` (aristas vivas, sin números) o `"pieces"` (un color por
  pieza): `mcp_model_mesh` arma la malla en el formato de `cad_mesh`.
- App: `analyze_mesh`, `repair_mesh` y `unwrap_original_mesh` con su `*_impl` sin
  `AppHandle`; al dispatch se suman reparar, diagnósticos, UV, impresión (verificar,
  orientar, uniones, material, placas, todo en uno), `placement_floor` y
  `workspace_summary` con el objeto activo y si hay retopología.
- Efecto nuevo `Model`: sin ventana se deshace como `Change`; en vivo la app copia el
  modelo al empezar el turno (`take_snapshot`) y al terminar la ventana lo recarga y
  pone el paso en su deshacer (`adoptAiModelChange`); si falla, vuelve la copia. El
  deshacer del conector guarda también la interfaz del proyecto de antes (`Before`).
- Pendiente: `mesh_texture` (editar texturas) y las pieles imprimibles; elegir otro
  objeto activo por nombre; los pasos de la IA no se rehacen solos si cambia la pieza
  del diseño de la que sale el objeto (la app sí rehace los suyos).
- Pruebas `tests/mesh.rs`: cubo que no cabe → piezas, uniones, placas sin pisar y
  deshacer; malla abierta → diagnóstico, reparar, remallar, vista, exportar; escalar y
  apoyar; cada importación es un objeto y `undo` lo quita; el proyecto guardado conserva
  los objetos; render; nube de puntos → malla; pieza CAD → objeto (con OCCT).

### M6 — Esqueleto, pesos, pose y animación ❌ (trabajo futuro)
- `rig_skeleton` (plantilla o cuerpo por partes), `rig_fit`, `rig_weights`,
  `rig_pose` (por hueso, IK), `anim_keys`, `anim_export` (GLB, USDZ; el conversor no
  exporta FBX) y `view` con la pose.
- Decidido (2026-10-25): **solo con la app abierta (modo en vivo)**. Las herramientas le
  piden a la ventana que corra sus propias acciones (`handleSkeletonChange`,
  `handleAutoFit`, `handleAutorig`, pose, `handleInsertKey`, exportar), como `import`
  y `use_part`: mismo código que a mano, deshacer de la app, la animación sigue en la
  interfaz y `view` captura el visor con el skinning de la GPU. Sin ventana devuelven
  un error con la pista de prender el modo en vivo. Evita separar el rig de `AppHandle`
  en `commands.rs`. Falta: adaptadores en `App.tsx` que devuelvan errores en vez de
  avisar en la barra, pose y claves por nombre de hueso, exportar a una ruta sin
  diálogo, y pruebas en Chromium o Tauri real.

### M7 — Render y escáner ✅ 2026-10-25
- `render_image`: GeppettoRay sobre el modelo activo, de una vez (`render_view_impl`,
  sin el trabajo en segundo plano de la app), cámara que encuadra todo desde una vista o
  azimut/elevación, fondo de estudio, blanco, transparente o de color, entorno HDR
  incluido (studio, loft, sky, overcast, sunset), calidad draft/normal/high; devuelve la
  imagen y, con `path`, la guarda. Avanza por pasadas.
- `scan_import`: nube PLY guardada → malla (MLS o Poisson, tapa agujeros, color horneado)
  como objeto nuevo (`mesh_cloud_file_impl`; en vivo, comando `scan_mesh_file` desde la
  ventana). El control del escáner físico queda fuera.

### M8 — Modo en vivo ✅ 2026-10-25
- Puente en la app (`mcp/live.rs`): con «Permitir que una IA maneje esta ventana»
  (Configuración ▸ Conector para IA, se recuerda) escucha en 127.0.0.1 con clave y deja
  `mcp-live.json` en la carpeta de datos. El conector (`Auto`) lo busca al empezar cada
  herramienta: si la app está, le manda los comandos; si no, usa su propio motor.
- **Por turnos, el usuario primero**: cada herramienta pide turno; la ventana lo niega si
  hay un sketch o un diálogo abiertos, una versión a la vista o si el usuario pausó la IA
  (la IA recibe «ocupado, espera y reintenta»). Durante el turno la ventana bloquea las
  ediciones (se puede seguir mirando); una sola IA a la vez.
- Al terminar: si la herramienta falló, las pestañas vuelven a como estaban; si cambió el
  trabajo, la ventana recibe cada pestaña cambiada con su documento de antes y lo pone en
  su deshacer (Ctrl+Z deshace el paso de la IA). `undo`/`redo` de la IA solo tocan sus
  propios pasos. Herramientas de proyecto deshabilitadas en vivo (los archivos son del
  usuario).
- `selection`: lo que el usuario eligió con el mouse (caras, aristas, vértices,
  sketches…) con la numeración de las herramientas — «redondea esta arista».
- Indicador arriba al centro: «IA en vivo» / «IA: Redondeando aristas…» / «IA en pausa»,
  con Pausar/Reanudar.
- Pruebas `tests/live.rs`: con una ventana de prueba, lo de la IA queda en la app, la
  ventana recibe el cambio con el documento de antes, ocupado y pausado niegan sin tocar
  nada, un fallo revierte, la selección llega, los proyectos están bloqueados y al apagar
  el conector vuelve a su motor.
- Probado con la app Tauri real y Claude Code (2026-10-25, Próximo 4).

### M9 — Guía, prompts y pulido ❌
- Recurso `pinocchio://guide` y uno por área; `prompts` de tareas completas.
- Revisión de cada descripción con un modelo chico (si un modelo chico la usa bien,
  cualquiera puede): una prueba por herramienta con un pedido en lenguaje natural.
- Pruebas de punta a punta: un cliente MCP que diseña una pieza, la exporta, importa
  una malla, la repara y la prepara para imprimir.
- Errores que la IA repite (salen del uso del chat): cada uno es una descripción poco
  clara o una herramienta que debería aceptar más casos. Se arreglan en la herramienta,
  no en el prompt. Arreglados: `asm_check` sobre una pestaña de diseño (2026-10-25:
  ahora revisa las piezas entre sí). Por revisar: lo que junte Configuración ▸ Errores de
  la IA tras usar el chat un tiempo.

## Riesgos

- Separar comandos de `AppHandle` toca código compartido con la otra sesión: se hace por
  archivo, cuando una herramienta lo necesita, y con commits chicos.
- Las operaciones largas (rig, remallado, render) necesitan avance y cancelación para
  que la IA no crea que se colgó: el avance está (Próximo 3); falta cancelar a mitad.
- ~~Modo en vivo con el frontend como dueño del deshacer~~: resuelto en M8 (la ventana
  recibe el documento de antes y lo pone en su deshacer).
