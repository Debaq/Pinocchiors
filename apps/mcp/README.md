# pinocchio-mcp

Conector MCP de Pinocchio: cualquier IA que hable MCP (Claude Code, Claude Desktop, Cursor,
Codex, Gemini CLI, LM Studio…) usa la app. Plan y avance en [`../PLAN_MCP.md`](../PLAN_MCP.md).

Habla MCP por stdio (un mensaje JSON por línea) y trabaja **sin ventana**, con su propio
motor: abre y guarda proyectos `.pinocchio` que la app abre igual que los suyos.

**La app instalada es el conector**: `Pinocchio --mcp` (o el AppImage con `--mcp`). Se
activa desde la app, en **Configuración ▸ Conector para IA**: un botón por cliente (Claude
Code, Claude Desktop, Cursor, Gemini CLI, Codex) agrega el conector a su configuración
(antes guarda `<archivo>.pinocchio.bak`), y ahí mismo están los descargables para el
agente: la configuración `mcp.json`, la guía en Markdown (`--mcp --guide`) y la skill de
Claude Code. El código vive en `apps/desktop/src/mcp/`; este crate es el binario suelto
`pinocchio-mcp` (desarrollo) y las pruebas.

## Modo en vivo

Con **«Permitir que una IA maneje esta ventana»** prendido, la IA trabaja en la app
abierta y se ve cada cambio. Va por turnos (si estás editando, espera), cada cambio suyo es
un paso de Ctrl+Z, se la puede pausar desde el indicador de arriba, y con `selection` ve lo
que el usuario eligió con el mouse. Solo escucha en 127.0.0.1, con clave.

## A mano

```sh
cargo build --release -p pinocchio-mcp        # binario suelto, o usar la app con --mcp
claude mcp add pinocchio -- /ruta/Pinocchio.AppImage --mcp
```

Los demás clientes usan la misma forma (`claude_desktop_config.json`, `~/.cursor/mcp.json`,
`~/.gemini/settings.json`):

```json
{ "mcpServers": { "pinocchio": { "command": "/ruta/Pinocchio.AppImage", "args": ["--mcp"] } } }
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.pinocchio]
command = "/ruta/Pinocchio.AppImage"
args = ["--mcp"]
```

## Qué ve la IA

- `initialize` le da las convenciones (mm, grados, Z arriba, todo se deshace).
- Herramientas por tarea, con descripción, unidades, ejemplo y esquema estricto. Cada
  resultado trae un resumen en texto y los datos en `structuredContent`; un error trae
  una pista de cómo arreglarlo, y una herramienta que falla no cambia nada.
- Recursos `pinocchio://guide` (la guía entera) y `pinocchio://guide/<tema>`; prompt
  `get_started`.

| Herramienta | Qué hace |
|---|---|
| `status` | Modo, proyecto, pestañas CAD, malla, esqueleto e historial |
| `help` | Guía por tema (`overview`, `conventions`, `cad`, `assembly`, `sketch`, `features`, `projects`) |
| `undo` / `redo` | Deshacer y rehacer los últimos 20 cambios |
| `project_new` / `project_open` / `project_save` | Proyectos `.pinocchio` |
| `cad_tab_create` / `cad_tab_rename` / `cad_tab_delete` | Pestañas de diseño y ensamble |
| `view` | Imagen PNG del diseño con los números de caras o aristas, resaltados y ejes |
| `measure` | Medidas, distancias y ángulos entre caras, aristas y puntos |
| `cad_design` | Operaciones, estado, parámetros y piezas de un diseño |
| `cad_geometry` | Caras y aristas numeradas para elegirlas |
| `sketch_create` / `sketch_add` | Sketches con figuras y restricciones |
| `cad_extrude` / `cad_revolve` / `cad_primitive` | Sólidos |
| `cad_fillet` / `cad_chamfer` / `cad_shell` | Redondeo, chaflán, vaciado |
| `cad_add_feature` / `cad_edit_feature` / `cad_delete_feature` | Cualquier operación en JSON |
| `cad_parameters` | Parámetros y fórmulas |
| `cad_export` | STEP, STL, 3MF, OBJ, PLY, GLB |
| `asm_insert` / `asm_mate` / `asm_instance` / `asm_mate_edit` | Ensambles: poner piezas, unirlas, moverlas |
| `asm_check` | Relaciones, grados libres, choques, holguras y lista de materiales |
| `drawing_export` | Lámina 2D (PDF/SVG) de un diseño o ensamble, con vista previa |

Mallas, impresión, esqueleto, animación y render se suman por fases (M5–M7).

## Probar

```sh
cargo test -p pinocchio-mcp
```

Las pruebas hablan con el servidor en el mismo proceso y lanzan el binario real por
stdin/stdout.
