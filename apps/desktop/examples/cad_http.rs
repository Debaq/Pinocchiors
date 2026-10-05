//! Servidor HTTP mínimo con los comandos CAD reales, para probar el frontend
//! en un navegador sin Tauri: `POST /invoke/<comando>` con los argumentos en
//! JSON; responde JSON o binario. Solo escucha en 127.0.0.1.
//!
//! `cargo run -p pinocchio-app --example cad_http -- [puerto]`

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

use pinocchio_app_lib::cad::bridge::{dispatch, Reply};
use pinocchio_app_lib::AppState;

fn main() {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(8766);
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("puerto ocupado");
    eprintln!("cad_http en http://127.0.0.1:{port}");
    let state = AppState::new();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            continue;
        }
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        let _ = reader.read_exact(&mut body);
        let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
        let cors = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\n";
        if request_line.starts_with("OPTIONS") {
            let _ = write!(stream, "HTTP/1.1 204 No Content\r\n{cors}Content-Length: 0\r\n\r\n");
            continue;
        }
        let cmd = path.trim_start_matches("/invoke/");
        let args: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
        let started = std::time::Instant::now();
        let (status, kind, payload) = match dispatch(&state, cmd, &args) {
            Ok(Reply::Json(v)) => ("200 OK", "application/json", serde_json::to_vec(&v).unwrap()),
            Ok(Reply::Bytes(b)) => ("200 OK", "application/octet-stream", b),
            Err(e) => ("500 Internal Server Error", "application/json", serde_json::to_vec(&e).unwrap()),
        };
        eprintln!("{cmd} → {status} ({} B, {:.0} ms)", payload.len(), started.elapsed().as_secs_f64() * 1e3);
        let _ = write!(stream, "HTTP/1.1 {status}\r\n{cors}Content-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
        let _ = stream.write_all(&payload);
    }
}
