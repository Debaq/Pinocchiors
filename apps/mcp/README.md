# pinocchio-mcp

Conector MCP de Pinocchio: cualquier IA que hable MCP (Claude Code, Claude Desktop, Cursor,
Codex, Gemini CLI, LM Studio…) usa la app. Plan y avance en [`../PLAN_MCP.md`](../PLAN_MCP.md).

Habla MCP por stdio (un mensaje JSON por línea). Por ahora trabaja **sin ventana**, con su
propio motor: abre y guarda proyectos `.pinocchio` que la app abre igual que los suyos. El
modo en vivo (la IA maneja la app abierta) llega en la fase M8.

## Compilar

```sh
cargo build --release -p pinocchio-mcp
```

El binario queda en `$CARGO_TARGET_DIR/release/pinocchio-mcp` (en este equipo,
`~/.cargo-target/release/pinocchio-mcp`). `pinocchio-mcp --help` muestra el uso.

## Conectarlo a una IA

En todos los casos va la **ruta absoluta** del binario.

**Claude Code**

```sh
claude mcp add pinocchio -- ~/.cargo-target/release/pinocchio-mcp
```

**Claude Desktop** (`~/.config/Claude/claude_desktop_config.json`), **Cursor**
(`~/.cursor/mcp.json`), **Gemini CLI** (`~/.gemini/settings.json`) y la mayoría de los
clientes usan la misma forma:

```json
{
  "mcpServers": {
    "pinocchio": { "command": "/home/USUARIO/.cargo-target/release/pinocchio-mcp" }
  }
}
```

**Codex CLI** (`~/.codex/config.toml`):

```toml
[mcp_servers.pinocchio]
command = "/home/USUARIO/.cargo-target/release/pinocchio-mcp"
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
| `help` | Guía por tema (`overview`, `conventions`, `projects`) |
| `undo` / `redo` | Deshacer y rehacer los últimos 20 cambios |
| `project_new` / `project_open` / `project_save` | Proyectos `.pinocchio` |

Las herramientas de CAD, mallas, impresión, esqueleto, animación y render se suman por
fases (M2–M7).

## Probar

```sh
cargo test -p pinocchio-mcp
```

Las pruebas hablan con el servidor en el mismo proceso y lanzan el binario real por
stdin/stdout.
