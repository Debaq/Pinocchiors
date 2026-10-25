//! `pinocchio-mcp`: el conector MCP suelto (para desarrollo y pruebas). En la
//! app instalada el conector es la misma app: `pinocchio --mcp`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(pinocchio_app_lib::mcp::run(&args));
}
