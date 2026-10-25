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

### M2 — Diseño CAD ❌
- `cad_tabs` (listar, crear, renombrar, borrar pestañas), `cad_parameters`,
  `cad_variables`.
- `sketch_create` (plano o cara), `sketch_add` (líneas, rectángulos, círculos, arcos,
  polígonos, ranuras, texto), `sketch_constrain` (restricciones y cotas),
  `sketch_status` (grados libres, conflictos, si está cerrado).
- `cad_feature` (extrusión, revolución, barrido, recubrimiento, agujero, rosca,
  redondeo, chaflán, vaciado, patrón, simetría, chapa, engranes), `cad_edit_feature`,
  `cad_delete_feature`, `cad_part_info` (volumen, caja, masa, errores), `cad_export`
  (STEP, STL, 3MF, IGES, OBJ).

### M3 — Ver y medir ❌
- `view` (PNG de la pestaña, pieza, ensamble, malla o pose; vistas estándar o ángulo),
  `measure` (distancia, ángulo, radio entre caras, aristas y puntos), `describe_faces`
  (caras con nombre, normal, área, para poder referirlas).

### M4 — Ensambles y planos ❌
- `asm_insert`, `asm_mate`, `asm_move`, `asm_check` (interferencias, holguras),
  `asm_bom`; `drawing_create`, `drawing_add_view`, `drawing_dimension`, `drawing_export`
  (PDF, DXF, SVG).

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

### M8 — Modo en vivo ❌
- Puente en la app (puerto, token, archivo `mcp-live.json`), interruptor en
  Configuración, aviso `mcp://changed` y recarga en el frontend con paso de deshacer.
- `status` dice en qué modo está; si la app se cierra a mitad, el servidor vuelve a su
  propio motor y lo avisa.

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
