//! Conector MCP: cualquier IA que hable MCP usa Pinocchio (CAD, ensambles,
//! planos…). La misma app es el conector: `pinocchio --mcp` habla MCP por
//! stdio sin abrir ventana (lo registra Configuración ▸ Conector para IA).
//! Plan en `apps/PLAN_MCP.md`.

pub mod backend;
pub mod chat;
pub mod clients;
pub mod guide;
pub mod live;
pub mod protocol;
pub mod render;
pub mod schema;
pub mod server;
pub mod session;
pub mod tools;

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use backend::Auto;
use server::{Sender, Server};
use session::Session;

/// Argumento de la línea de comandos que pone la app en modo conector
pub const FLAG: &str = "--mcp";

const USAGE: &str = "\
Pinocchio MCP server (CAD parts, assemblies, 2D drawings) over stdin/stdout.

Usage: pinocchio --mcp            speak MCP (what AI clients launch)
       pinocchio --mcp --guide    print the guide for agents (Markdown)
       pinocchio --mcp --help     this text

Connect it from Pinocchio: Settings ▸ AI connector.";

/// Corre el conector; devuelve el código de salida. `args`: lo que va después de `--mcp`.
pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            println!("{USAGE}");
            0
        }
        Some("--version" | "-V") => {
            println!("pinocchio-mcp {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("--guide") => {
            print!("{}", guide::agent_markdown());
            0
        }
        Some(other) => {
            eprintln!("argumento desconocido: {other}\n\n{USAGE}");
            2
        }
        None => {
            serve_stdio();
            0
        }
    }
}

/// Atiende MCP por stdin/stdout hasta que el cliente cierra la entrada
fn serve_stdio() {
    let server = Arc::new(Server::new(Session::new(Box::new(Auto::new()))));
    let stdout = Arc::new(Mutex::new(std::io::stdout()));
    let send: Arc<Sender<'static>> = Arc::new(move |msg| {
        let mut out = stdout.lock().unwrap();
        let _ = writeln!(out, "{msg}");
        let _ = out.flush();
    });
    eprintln!("pinocchio-mcp {} listo (la app abierta con el modo en vivo, o motor propio)", env!("CARGO_PKG_VERSION"));

    // Las herramientas van a una cola con un solo hilo: corren en el orden en
    // que llegan, y mientras tanto se atienden ping y cancelaciones
    let (queue, jobs) = std::sync::mpsc::channel::<protocol::Incoming>();
    let worker = {
        let (server, send) = (server.clone(), send.clone());
        std::thread::spawn(move || {
            for msg in jobs {
                server.handle(msg, &*send);
            }
        })
    };
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        match protocol::parse(&line) {
            Err(reply) => send(reply),
            Ok(None) => {}
            Ok(Some(msg)) if msg.method == "tools/call" => {
                server.queued(&msg);
                let _ = queue.send(msg);
            }
            Ok(Some(msg)) => server.handle(msg, &*send),
        }
    }
    // El cliente cerró la entrada: lo que quedó en la cola termina y responde
    drop(queue);
    let _ = worker.join();
}
