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
   imprimir», «riggear un personaje»). `instructions` del `initialize` resume lo básico.
9. **Seguro por defecto**: no escribe fuera de las rutas que se le piden, no pisa un
   archivo sin `overwrite: true`, y el modo en vivo se activa a mano en la app.

## Arquitectura

- Crate nuevo `apps/mcp` (binario `pinocchio-mcp`) en el workspace. Protocolo MCP
  (JSON-RPC 2.0 por stdio, versión 2025-06-18) escrito a mano con `serde_json`: no hay
  SDK de MCP sin conexión y son pocos mensajes (`initialize`, `tools/list`,
  `tools/call`, `resources/*`, `prompts/*`, `ping`, avisos de avance). Registro a stderr.
- **Anfitrión sin Tauri** (`Host`): hoy unos 140 comandos reciben `AppHandle` solo para
  leer `AppState` y avisar el avance. Se separa cada uno en `*_impl(&AppState,
  &dyn Progress, args)` y el comando de Tauri queda como envoltorio de una línea. Se
  generaliza `cad::bridge::dispatch` (que ya cubre 87 comandos CAD) a toda la app.
  `cad_http` y el e2e ganan lo mismo.
- **Modo en vivo**: la app, con «Permitir que una IA controle la app» encendido
  (Configuración, apagado por defecto), escucha en 127.0.0.1 en un puerto libre y
  escribe `~/.local/share/pinocchio/mcp-live.json` (`port`, `token`, `pid`). El servidor
  MCP lo lee al empezar cada llamada: si la app está viva, le reenvía el comando con el
  token; si no, usa su propio motor. Tras cada cambio la app avisa al frontend
  (`mcp://changed`) para que recargue lo tocado y deje un paso de deshacer «Cambio de la
  IA».
- Las herramientas se describen en una tabla (`tools.rs`: nombre, descripción, esquema,
  función) para que la lista, la guía y las pruebas salgan del mismo lugar.

## Hoja de ruta

Hecho (2026-10-24): M0, M2, M3 y M4 (CAD completo: diseño, ver y medir, ensambles y
planos), M1 en lo que el CAD necesita. **Para después** (decisión del usuario, 2026-10-25):
M5 mallas e impresión, M6 esqueleto y animación, M7 render y escáner y M9 pulido
(M8, el modo en vivo, se hizo el 2026-10-25); el resto de M1 se hace dentro de cada una.

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
  los comandos largos (hoy el `Channel` del `dispatch` lo descarta).

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

### M4 — Ensambles y planos ✅ 2026-10-24
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
- Falta: editar las láminas de la app (formato de `drawing` del frontend), cotas de
  detalle, cortes y relaciones de engranes/tornillo desde el conector.
- Pruebas: eje en placa (cilíndrica → 2 grados, centrado → 1), misma instancia
  rechazada, choques y lista, medir en el ensamble (0 mm entre eje y agujero), copias y
  quitar, PDF del ensamble con lista, SVG en tercer ángulo 2:1 con ocultas, sin pisar.

### M5 — Mallas, conversión e impresión ❌
- `mesh_import`, `mesh_export` (todos los formatos del conversor), `mesh_info`,
  `mesh_repair`, `mesh_remesh` (simplificar, isótropo, vóxeles, quads, suavizar),
  `mesh_uv`, `mesh_texture`.
- `print_check`, `print_orient`, `print_cut`, `print_joints`, `print_hollow`,
  `print_plates` (exportar placas).

### M6 — Esqueleto, pesos, pose y animación ❌
- `rig_skeleton` (plantilla o cuerpo por partes), `rig_fit`, `rig_weights`,
  `rig_pose` (por hueso, IK), `anim_keys`, `anim_export` (GLB, FBX vía conversor, USDZ).

### M7 — Render y escáner ❌
- `render_image` (GeppettoRay: cámara, luces, materiales, tamaño), `scan_import`
  (nubes y escaneos guardados; el control del escáner físico queda fuera).

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
- Falta: probarlo con la app Tauri real y un cliente de IA de verdad.

### M9 — Guía, prompts y pulido ❌
- Recurso `pinocchio://guide` y uno por área; `prompts` de tareas completas.
- Revisión de cada descripción con un modelo chico (si un modelo chico la usa bien,
  cualquiera puede): una prueba por herramienta con un pedido en lenguaje natural.
- Pruebas de punta a punta: un cliente MCP que diseña una pieza, la exporta, importa
  una malla, la repara y la prepara para imprimir.

## Riesgos

- Separar ~140 comandos de `AppHandle` toca mucho código compartido con la otra
  sesión: se hace por archivo y con commits chicos.
- Las operaciones largas (rig, remallado, render) necesitan avance y cancelación para
  que la IA no crea que se colgó.
- Modo en vivo con el frontend como dueño del deshacer: los cambios de la IA tienen que
  entrar por el mismo camino que los del usuario.
