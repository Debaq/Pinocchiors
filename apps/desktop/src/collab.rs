//! Colaboración en tiempo real (C0): puente entre la sala P2P
//! (`pinocchio-collab`) y la ventana. La ventana manda por comandos y recibe
//! todo lo que llega por el evento `collab://event`.

use std::sync::Arc;

use pinocchio_collab::{Event, Message, Options, Room};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::sync::Mutex;

pub const EV_EVENT: &str = "collab://event";

#[derive(Default)]
pub struct Collab {
    room: Mutex<Option<Arc<Room>>>,
}

#[derive(Serialize)]
pub struct CollabStatus {
    pub on: bool,
    pub host: bool,
    pub ticket: Option<String>,
    pub my_id: Option<String>,
}

fn status(c: &Collab) -> CollabStatus {
    match &*c.room.lock().unwrap() {
        Some(r) => CollabStatus { on: true, host: r.is_host(), ticket: Some(r.ticket().to_string()), my_id: Some(r.my_id().to_string()) },
        None => CollabStatus { on: false, host: false, ticket: None, my_id: None },
    }
}

fn event_json(e: Event) -> Value {
    match e {
        Event::PeerJoined { id, name, color } => json!({ "kind": "joined", "id": id, "name": name, "color": color }),
        Event::PeerLeft { id } => json!({ "kind": "left", "id": id }),
        Event::Message { from, msg } => json!({ "kind": "message", "from": from, "msg": msg }),
        Event::Light { from, bytes } => json!({ "kind": "light", "from": from, "bytes": bytes }),
    }
}

/// Deja la sala como la sala activa y empieza a pasar sus eventos a la ventana
fn adopt(app: AppHandle, c: &Collab, room: Room) -> CollabStatus {
    let room = Arc::new(room);
    *c.room.lock().unwrap() = Some(room.clone());
    let pump = room.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(e) = pump.recv().await {
            let _ = app.emit(EV_EVENT, event_json(e));
        }
    });
    status(c)
}

async fn close_current(c: &Collab) {
    let old = c.room.lock().unwrap().take();
    if let Some(r) = old {
        r.close().await;
    }
}

fn options(name: String, color: String) -> Options {
    Options::new(name.trim(), &color)
}

#[tauri::command]
pub async fn collab_host(app: AppHandle, c: State<'_, Collab>, name: String, color: String) -> Result<CollabStatus, String> {
    close_current(&c).await;
    let room = Room::host(options(name, color)).await.map_err(|e| e.to_string())?;
    Ok(adopt(app, &c, room))
}

#[tauri::command]
pub async fn collab_join(app: AppHandle, c: State<'_, Collab>, ticket: String, name: String, color: String) -> Result<CollabStatus, String> {
    close_current(&c).await;
    let room = Room::join(&ticket, options(name, color)).await.map_err(|e| e.to_string())?;
    Ok(adopt(app, &c, room))
}

#[tauri::command]
pub async fn collab_leave(c: State<'_, Collab>) -> Result<CollabStatus, String> {
    close_current(&c).await;
    Ok(status(&c))
}

#[tauri::command]
pub fn collab_status(c: State<'_, Collab>) -> CollabStatus {
    status(&c)
}

fn current(c: &Collab) -> Result<Arc<Room>, String> {
    c.room.lock().unwrap().clone().ok_or_else(|| "No hay una sala abierta".to_string())
}

/// Mensaje fiable (chat o carga libre). Anfitrión: a todos (o a uno / menos uno);
/// invitado: al anfitrión.
#[tauri::command]
pub async fn collab_send(c: State<'_, Collab>, msg: Message, except: Option<String>, only: Option<String>) -> Result<(), String> {
    let room = current(&c)?;
    match (only, except) {
        (Some(id), _) => room.send_to(&id, msg).await,
        (None, Some(id)) => room.send_except(&id, msg).await,
        (None, None) => room.send(msg).await,
    }
    .map_err(|e| e.to_string())
}

/// Datagrama sin garantía (presencia): se descarta si no cabe o no hay nadie
#[tauri::command]
pub async fn collab_send_light(c: State<'_, Collab>, bytes: Vec<u8>) -> Result<(), String> {
    current(&c)?.send_light(&bytes).await;
    Ok(())
}
