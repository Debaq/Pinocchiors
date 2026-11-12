//! Colaboración en tiempo real (C0): transporte P2P con iroh.
//!
//! Una persona es **anfitrión** y crea una sala; el resultado es un *ticket*
//! (texto corto que se comparte). Las demás se unen con ese ticket y quedan
//! conectadas directo al anfitrión (hole punching; relay solo si no hay
//! camino). Todo va cifrado de extremo a extremo.
//!
//! Dos canales por conexión:
//! - **fiable**: un stream bidireccional con mensajes JSON con prefijo de
//!   largo (`Message`): hola, chat, comandos…
//! - **liviano**: datagramas sin garantía (cursores, clics); se pueden perder.

use std::collections::HashMap;
use std::sync::Arc;

use iroh::endpoint::{presets, Connection};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

/// Identifica el protocolo; subir el sufijo si cambia de forma incompatible.
pub const ALPN: &[u8] = b"pinocchio/collab/0";
/// Tope de un mensaje fiable (el snapshot va por trozos, no cabe todo aquí)
pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("red: {0}")]
    Net(String),
    #[error("ticket inválido: {0}")]
    Ticket(String),
    #[error("protocolo: {0}")]
    Protocol(String),
    #[error("la sala está cerrada")]
    Closed,
}

fn net<E: std::fmt::Display>(e: E) -> Error {
    Error::Net(e.to_string())
}

/// Mensajes del canal fiable
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Message {
    /// Primer mensaje de cada lado: quién es
    Hello { name: String, color: String },
    Ping { n: u64 },
    Pong { n: u64 },
    Chat { text: String },
    /// Carga libre para las fases siguientes (comandos, comentarios…)
    Data { kind: String, value: serde_json::Value },
}

/// Lo que llega a quien usa la sala
#[derive(Debug, Clone)]
pub enum Event {
    PeerJoined { id: String, name: String, color: String },
    PeerLeft { id: String },
    Message { from: String, msg: Message },
    /// Datagrama sin garantía (presencia)
    Light { from: String, bytes: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct Options {
    pub name: String,
    pub color: String,
    /// `true` usa los relays y el descubrimiento por defecto (internet);
    /// `false` solo conexión directa (pruebas, red local)
    pub internet: bool,
}

impl Options {
    pub fn new(name: &str, color: &str) -> Self {
        Self { name: name.into(), color: color.into(), internet: true }
    }
}

type Peers = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Message>>>>;

/// Una sala abierta (como anfitrión o como invitado)
pub struct Room {
    endpoint: Endpoint,
    ticket: String,
    host: bool,
    my_id: String,
    peers: Peers,
    /// Conexiones vivas, para los datagramas
    conns: Arc<Mutex<HashMap<String, Connection>>>,
    events: Mutex<mpsc::UnboundedReceiver<Event>>,
}

impl Room {
    /// Crea una sala y empieza a aceptar invitados
    pub async fn host(opts: Options) -> Result<Room, Error> {
        let endpoint = bind(&opts).await?;
        if opts.internet {
            endpoint.online().await;
        }
        let ticket = encode_ticket(&endpoint.addr())?;
        let (tx, events) = mpsc::unbounded_channel();
        let peers: Peers = Default::default();
        let conns: Arc<Mutex<HashMap<String, Connection>>> = Default::default();
        let (ep, p, c, o) = (endpoint.clone(), peers.clone(), conns.clone(), opts.clone());
        tokio::spawn(async move {
            while let Some(incoming) = ep.accept().await {
                let (p, c, tx, o) = (p.clone(), c.clone(), tx.clone(), o.clone());
                tokio::spawn(async move {
                    if let Ok(conn) = incoming.await {
                        let _ = serve(conn, true, o, p, c, tx).await;
                    }
                });
            }
        });
        Ok(Room { my_id: endpoint.id().to_string(), endpoint, ticket, host: true, peers, conns, events: Mutex::new(events) })
    }

    /// Se une a la sala de un anfitrión
    pub async fn join(ticket: &str, opts: Options) -> Result<Room, Error> {
        let addr = decode_ticket(ticket)?;
        let endpoint = bind(&opts).await?;
        let conn = endpoint.connect(addr, ALPN).await.map_err(net)?;
        let (tx, events) = mpsc::unbounded_channel();
        let peers: Peers = Default::default();
        let conns: Arc<Mutex<HashMap<String, Connection>>> = Default::default();
        let (p, c, o) = (peers.clone(), conns.clone(), opts.clone());
        tokio::spawn(async move {
            let _ = serve(conn, false, o, p, c, tx).await;
        });
        Ok(Room {
            my_id: endpoint.id().to_string(),
            endpoint,
            ticket: ticket.to_string(),
            host: false,
            peers,
            conns,
            events: Mutex::new(events),
        })
    }

    pub fn ticket(&self) -> &str {
        &self.ticket
    }
    pub fn is_host(&self) -> bool {
        self.host
    }
    pub fn my_id(&self) -> &str {
        &self.my_id
    }

    /// Siguiente evento; `None` si la sala se cerró
    pub async fn recv(&self) -> Option<Event> {
        self.events.lock().await.recv().await
    }

    /// Anfitrión: a todos. Invitado: al anfitrión.
    pub async fn send(&self, msg: Message) -> Result<(), Error> {
        let peers = self.peers.lock().await;
        if peers.is_empty() {
            return Err(Error::Closed);
        }
        for tx in peers.values() {
            let _ = tx.send(msg.clone());
        }
        Ok(())
    }

    /// Anfitrión: a todos menos a uno (reparte lo que mandó esa persona)
    pub async fn send_except(&self, except: &str, msg: Message) -> Result<(), Error> {
        for (id, tx) in self.peers.lock().await.iter() {
            if id != except {
                let _ = tx.send(msg.clone());
            }
        }
        Ok(())
    }

    /// Mensaje a una sola persona (solo anfitrión)
    pub async fn send_to(&self, id: &str, msg: Message) -> Result<(), Error> {
        let peers = self.peers.lock().await;
        peers.get(id).ok_or(Error::Closed)?.send(msg).map_err(|_| Error::Closed)
    }

    /// Datagrama sin garantía a todos los conectados (se descarta si no cabe)
    pub async fn send_light(&self, bytes: &[u8]) {
        for conn in self.conns.lock().await.values() {
            let _ = conn.send_datagram(bytes::Bytes::copy_from_slice(bytes));
        }
    }

    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

async fn bind(opts: &Options) -> Result<Endpoint, Error> {
    let b = if opts.internet {
        Endpoint::builder(presets::N0)
    } else {
        Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
    };
    b.alpns(vec![ALPN.to_vec()]).bind().await.map_err(net)
}

/// Ticket = dirección completa del anfitrión en JSON + base32 (sin relleno).
/// Se acepta también un id suelto (64 hex): con internet se resuelve solo.
pub fn encode_ticket(addr: &EndpointAddr) -> Result<String, Error> {
    let json = serde_json::to_vec(addr).map_err(|e| Error::Ticket(e.to_string()))?;
    Ok(data_encoding::BASE32_NOPAD.encode(&json))
}

pub fn decode_ticket(s: &str) -> Result<EndpointAddr, Error> {
    let s = s.trim();
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let id: EndpointId = s.parse().map_err(|e: <EndpointId as std::str::FromStr>::Err| Error::Ticket(e.to_string()))?;
        return Ok(EndpointAddr::new(id));
    }
    let bytes = data_encoding::BASE32_NOPAD
        .decode(s.to_ascii_uppercase().as_bytes())
        .map_err(|e| Error::Ticket(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::Ticket(e.to_string()))
}

/// Mensaje con prefijo de largo (u32 big endian)
async fn write_msg(w: &mut iroh::endpoint::SendStream, msg: &Message) -> Result<(), Error> {
    let body = serde_json::to_vec(msg).map_err(net)?;
    if body.len() > MAX_MESSAGE {
        return Err(Error::Protocol("mensaje demasiado grande".into()));
    }
    w.write_all(&(body.len() as u32).to_be_bytes()).await.map_err(net)?;
    w.write_all(&body).await.map_err(net)
}

async fn read_msg(r: &mut iroh::endpoint::RecvStream) -> Result<Message, Error> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await.map_err(net)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_MESSAGE {
        return Err(Error::Protocol("mensaje demasiado grande".into()));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await.map_err(net)?;
    serde_json::from_slice(&body).map_err(|e| Error::Protocol(e.to_string()))
}

/// Atiende una conexión hasta que se cae. El invitado abre el stream y dice
/// «hola» primero; el anfitrión contesta con el suyo.
async fn serve(
    conn: Connection,
    host: bool,
    opts: Options,
    peers: Peers,
    conns: Arc<Mutex<HashMap<String, Connection>>>,
    events: mpsc::UnboundedSender<Event>,
) -> Result<(), Error> {
    let id = conn.remote_id().to_string();
    let (mut send, mut recv) = if host {
        conn.accept_bi().await.map_err(net)?
    } else {
        conn.open_bi().await.map_err(net)?
    };
    write_msg(&mut send, &Message::Hello { name: opts.name.clone(), color: opts.color.clone() }).await?;
    let (name, color) = match read_msg(&mut recv).await? {
        Message::Hello { name, color } => (name, color),
        _ => return Err(Error::Protocol("se esperaba hola".into())),
    };
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    peers.lock().await.insert(id.clone(), tx);
    conns.lock().await.insert(id.clone(), conn.clone());
    let _ = events.send(Event::PeerJoined { id: id.clone(), name, color });

    // Escritor
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write_msg(&mut send, &m).await.is_err() {
                break;
            }
        }
    });
    // Datagramas
    let (c2, e2, id2) = (conn.clone(), events.clone(), id.clone());
    let light = tokio::spawn(async move {
        while let Ok(d) = c2.read_datagram().await {
            let _ = e2.send(Event::Light { from: id2.clone(), bytes: d.to_vec() });
        }
    });
    // Lector (termina cuando se cae la conexión)
    while let Ok(msg) = read_msg(&mut recv).await {
        let _ = events.send(Event::Message { from: id.clone(), msg });
    }
    writer.abort();
    light.abort();
    peers.lock().await.remove(&id);
    conns.lock().await.remove(&id);
    let _ = events.send(Event::PeerLeft { id });
    Ok(())
}
