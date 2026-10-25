// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `pinocchio --mcp`: la app como conector MCP para una IA (stdio, sin ventana)
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some(pinocchio_app_lib::mcp::FLAG) {
        std::process::exit(pinocchio_app_lib::mcp::run(&args[1..]));
    }
    pinocchio_app_lib::run()
}
