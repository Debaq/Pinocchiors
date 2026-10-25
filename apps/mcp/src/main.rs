//! `pinocchio-mcp`: servidor MCP por stdio (un mensaje JSON por línea).
//! Lo que escribe en stderr es el registro; stdout es solo del protocolo.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use pinocchio_mcp::backend::Local;
use pinocchio_mcp::protocol;
use pinocchio_mcp::server::{Sender, Server};
use pinocchio_mcp::session::Session;

const USAGE: &str = "\
pinocchio-mcp — MCP server for Pinocchio 3D (CAD, meshes, 3D printing, rigging, animation, rendering)

Usage: pinocchio-mcp            speak MCP over stdin/stdout (what AI clients launch)
       pinocchio-mcp --help     this text
       pinocchio-mcp --version

Add it to an AI client, e.g. Claude Code:
  claude mcp add pinocchio -- /path/to/pinocchio-mcp
See apps/mcp/README.md for Claude Desktop, Cursor and other clients.";

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--help" | "-h") => return println!("{USAGE}"),
        Some("--version" | "-V") => return println!("pinocchio-mcp {}", env!("CARGO_PKG_VERSION")),
        Some(other) => {
            eprintln!("argumento desconocido: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
        None => {}
    }

    let server = Arc::new(Server::new(Session::new(Box::new(Local::new()))));
    let stdout = Arc::new(Mutex::new(std::io::stdout()));
    let send: Arc<Sender<'static>> = Arc::new(move |msg| {
        let mut out = stdout.lock().unwrap();
        let _ = writeln!(out, "{msg}");
        let _ = out.flush();
    });
    eprintln!("pinocchio-mcp {} listo (motor propio, sin ventana)", env!("CARGO_PKG_VERSION"));

    let mut working = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        match protocol::parse(&line) {
            Err(reply) => send(reply),
            Ok(None) => {}
            // Las herramientas corren aparte: mientras tanto se atienden ping y cancelaciones
            Ok(Some(msg)) if msg.method == "tools/call" => {
                let (server, send) = (server.clone(), send.clone());
                working.retain(|h: &std::thread::JoinHandle<()>| !h.is_finished());
                working.push(std::thread::spawn(move || server.handle(msg, &*send)));
            }
            Ok(Some(msg)) => server.handle(msg, &*send),
        }
    }
    // El cliente cerró la entrada: las herramientas en curso terminan y responden
    for h in working {
        let _ = h.join();
    }
}
