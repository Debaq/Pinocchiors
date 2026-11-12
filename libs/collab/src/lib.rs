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
use iroh::{Endpoint, EndpointAddr, RelayMode};
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
    Hello {
        name: String,
        color: String,
        /// Clave de la sala (el invitado la trae en el ticket); el anfitrión rechaza si no coincide
        #[serde(default)]
        key: String,
    },
    Ping { n: u64 },
    Pong { n: u64 },
    Chat {
        text: String,
        /// Quién lo escribió, cuando el anfitrión reparte el mensaje de un invitado
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
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
    /// No se pudo entrar (clave incorrecta, expulsado, anfitrión caído)
    Rejected { reason: String },
    /// Archivo grande completo (el proyecto al entrar a la sala)
    Blob { from: String, name: String, bytes: Vec<u8> },
}

/// Cuánto se espera el saludo de quien se conecta (después se corta)
pub const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Tope de un archivo por la sala
pub const MAX_BLOB: usize = 1 << 30;

#[derive(Debug, Clone)]
pub struct Options {
    pub name: String,
    pub color: String,
    /// `true` usa los relays y el descubrimiento por defecto (internet);
    /// `false` solo conexión directa (pruebas, red local)
    pub internet: bool,
    /// Anfitrión: quien entra queda silenciado (no recibe ni reparte nada)
    /// hasta que se lo admite con `set_muted(id, false)`
    pub approval: bool,
}

impl Options {
    pub fn new(name: &str, color: &str) -> Self {
        Self { name: name.into(), color: color.into(), internet: true, approval: false }
    }
}

/// Lo que el anfitrión comparte con `serve` para dejar entrar silenciados
#[derive(Clone)]
struct Gate {
    muted: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    approval: Arc<std::sync::atomic::AtomicBool>,
}

type Peers = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Message>>>>;

/// Una sala abierta (como anfitrión o como invitado)
pub struct Room {
    endpoint: Endpoint,
    ticket: String,
    /// Clave de la sala (la del anfitrión se puede rotar)
    key: Arc<std::sync::Mutex<String>>,
    host: bool,
    /// Anfitrión: sin admitir (no reciben lo que se reparte, no se reparte lo suyo)
    muted: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    approval: Arc<std::sync::atomic::AtomicBool>,
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
        let key = Arc::new(std::sync::Mutex::new(new_key()));
        let ticket = encode_ticket(&endpoint.addr(), &key.lock().unwrap())?;
        let (tx, events) = mpsc::unbounded_channel();
        let peers: Peers = Default::default();
        let conns: Arc<Mutex<HashMap<String, Connection>>> = Default::default();
        let muted: Arc<std::sync::Mutex<std::collections::HashSet<String>>> = Default::default();
        let approval = Arc::new(std::sync::atomic::AtomicBool::new(opts.approval));
        let gate = Gate { muted: muted.clone(), approval: approval.clone() };
        let (ep, p, c, o, k) = (endpoint.clone(), peers.clone(), conns.clone(), opts.clone(), key.clone());
        tokio::spawn(async move {
            while let Some(incoming) = ep.accept().await {
                let (p, c, tx, o, k, g) = (p.clone(), c.clone(), tx.clone(), o.clone(), k.clone(), gate.clone());
                tokio::spawn(async move {
                    if let Ok(conn) = incoming.await {
                        let _ = serve(conn, Some(k), Some(g), String::new(), o, p, c, tx).await;
                    }
                });
            }
        });
        Ok(Room { my_id: endpoint.id().to_string(), endpoint, ticket, key, host: true, muted, approval, peers, conns, events: Mutex::new(events) })
    }

    /// Se une a la sala de un anfitrión
    pub async fn join(ticket: &str, opts: Options) -> Result<Room, Error> {
        let (addr, key) = decode_ticket(ticket)?;
        let endpoint = bind(&opts).await?;
        let conn = endpoint.connect(addr, ALPN).await.map_err(net)?;
        let (tx, events) = mpsc::unbounded_channel();
        let peers: Peers = Default::default();
        let conns: Arc<Mutex<HashMap<String, Connection>>> = Default::default();
        let (p, c, o) = (peers.clone(), conns.clone(), opts.clone());
        tokio::spawn(async move {
            let _ = serve(conn, None, None, key, o, p, c, tx).await;
        });
        Ok(Room {
            my_id: endpoint.id().to_string(),
            endpoint,
            ticket: ticket.to_string(),
            key: Default::default(),
            host: false,
            muted: Default::default(),
            approval: Default::default(),
            peers,
            conns,
            events: Mutex::new(events),
        })
    }

    /// El ticket para compartir (el del anfitrión refleja la clave actual)
    pub fn ticket(&self) -> String {
        if self.host {
            encode_ticket(&self.endpoint.addr(), &self.key.lock().unwrap()).unwrap_or_else(|_| self.ticket.clone())
        } else {
            self.ticket.clone()
        }
    }

    /// Anfitrión: cambia la clave. El ticket anterior deja de servir para
    /// entrar; quienes ya están dentro siguen.
    pub fn rotate_key(&self) -> String {
        *self.key.lock().unwrap() = new_key();
        self.ticket()
    }

    /// Anfitrión: pedir aprobación a los que entren desde ahora
    pub fn set_approval(&self, on: bool) {
        self.approval.store(on, std::sync::atomic::Ordering::SeqCst);
    }

    /// Anfitrión: silencia (o admite) a una persona
    pub fn set_muted(&self, id: &str, muted: bool) {
        let mut set = self.muted.lock().unwrap();
        if muted {
            set.insert(id.to_string());
        } else {
            set.remove(id);
        }
    }

    pub fn is_muted(&self, id: &str) -> bool {
        self.muted.lock().unwrap().contains(id)
    }

    /// Anfitrión: saca a una persona de la sala
    pub async fn kick(&self, id: &str) {
        if let Some(conn) = self.conns.lock().await.get(id) {
            conn.close(2u32.into(), "expulsado".as_bytes());
        }
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
        for (id, tx) in peers.iter() {
            if !self.is_muted(id) {
                let _ = tx.send(msg.clone());
            }
        }
        Ok(())
    }

    /// Anfitrión: a todos menos a uno (reparte lo que mandó esa persona)
    pub async fn send_except(&self, except: &str, msg: Message) -> Result<(), Error> {
        for (id, tx) in self.peers.lock().await.iter() {
            if id != except && !self.is_muted(id) {
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
        for (id, conn) in self.conns.lock().await.iter() {
            if !self.is_muted(id) {
                let _ = conn.send_datagram(bytes::Bytes::copy_from_slice(bytes));
            }
        }
    }

    /// Manda un archivo a una persona por su propio stream (QUIC lo reparte
    /// sin trocearlo a mano y sin frenar al chat). Llega como `Event::Blob`.
    pub async fn send_blob(&self, to: &str, name: &str, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > MAX_BLOB {
            return Err(Error::Protocol("archivo demasiado grande".into()));
        }
        let conn = self.conns.lock().await.get(to).cloned().ok_or(Error::Closed)?;
        let mut w = conn.open_uni().await.map_err(net)?;
        let name = name.as_bytes();
        w.write_all(&(name.len() as u32).to_be_bytes()).await.map_err(net)?;
        w.write_all(name).await.map_err(net)?;
        w.write_all(bytes).await.map_err(net)?;
        w.finish().map_err(net)?;
        // Esperar a que lo reciban antes de soltar la conexión
        let _ = w.stopped().await;
        Ok(())
    }

    /// Datagrama a todos menos a uno (el anfitrión reparte la presencia de un invitado)
    pub async fn send_light_except(&self, except: &str, bytes: &[u8]) {
        for (id, conn) in self.conns.lock().await.iter() {
            if id != except && !self.is_muted(id) {
                let _ = conn.send_datagram(bytes::Bytes::copy_from_slice(bytes));
            }
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

/// Clave nueva de sala: 128 bits al azar, en hexadecimal
fn new_key() -> String {
    let mut b = [0u8; 16];
    let sk = iroh::SecretKey::generate();
    b.copy_from_slice(&sk.to_bytes()[..16]);
    data_encoding::HEXLOWER.encode(&b)
}

#[derive(Serialize, Deserialize)]
struct Ticket {
    a: EndpointAddr,
    k: String,
}

/// Ticket = dirección completa del anfitrión y clave de la sala, en JSON +
/// base32 (sin relleno). Sin la clave el anfitrión no deja entrar.
pub fn encode_ticket(addr: &EndpointAddr, key: &str) -> Result<String, Error> {
    let json = serde_json::to_vec(&Ticket { a: addr.clone(), k: key.to_string() }).map_err(|e| Error::Ticket(e.to_string()))?;
    Ok(data_encoding::BASE32_NOPAD.encode(&json))
}

pub fn decode_ticket(s: &str) -> Result<(EndpointAddr, String), Error> {
    let bytes = data_encoding::BASE32_NOPAD
        .decode(s.trim().to_ascii_uppercase().as_bytes())
        .map_err(|e| Error::Ticket(e.to_string()))?;
    let t: Ticket = serde_json::from_slice(&bytes).map_err(|e| Error::Ticket(e.to_string()))?;
    Ok((t.a, t.k))
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
    // Anfitrión: la clave vigente de la sala; invitado: `None`
    room_key: Option<Arc<std::sync::Mutex<String>>>,
    gate: Option<Gate>,
    // Invitado: la clave que trae el ticket
    my_key: String,
    opts: Options,
    peers: Peers,
    conns: Arc<Mutex<HashMap<String, Connection>>>,
    events: mpsc::UnboundedSender<Event>,
) -> Result<(), Error> {
    let host = room_key.is_some();
    let id = conn.remote_id().to_string();
    // Un fallo antes de quedar dentro (clave mala, anfitrión caído) se avisa
    let handshake = async {
        let (mut send, mut recv) = if host {
            conn.accept_bi().await.map_err(net)?
        } else {
            conn.open_bi().await.map_err(net)?
        };
        write_msg(&mut send, &Message::Hello { name: opts.name.clone(), color: opts.color.clone(), key: my_key.clone() }).await?;
        match read_msg(&mut recv).await? {
            Message::Hello { name, color, key } => {
                if let Some(k) = &room_key {
                    if key != *k.lock().unwrap() {
                        conn.close(1u32.into(), "clave incorrecta".as_bytes());
                        return Err(Error::Protocol("clave incorrecta".into()));
                    }
                }
                Ok((send, recv, name, color))
            }
            _ => Err(Error::Protocol("se esperaba hola".into())),
        }
    };
    let handshake = async {
        match tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake).await {
            Ok(r) => r,
            Err(_) => {
                conn.close(3u32.into(), "sin saludo".as_bytes());
                Err(Error::Protocol("no saludó a tiempo".into()))
            }
        }
    };
    let (mut send, mut recv, name, color) = match handshake.await {
        Ok(v) => v,
        Err(e) => {
            if !host {
                let reason = match conn.close_reason() {
                    Some(r) => r.to_string(),
                    None => e.to_string(),
                };
                let _ = events.send(Event::Rejected { reason });
            }
            return Err(e);
        }
    };
    // Con aprobación, entra silenciado hasta que el anfitrión lo admita
    if let Some(g) = &gate {
        if g.approval.load(std::sync::atomic::Ordering::SeqCst) {
            g.muted.lock().unwrap().insert(id.clone());
        }
    }
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
    // Archivos grandes: solo los recibe el invitado, del anfitrión. El anfitrión
    // no acepta archivos de nadie (si no, un invitado podría mandarle un proyecto)
    let (c3, e3, id3) = (conn.clone(), events.clone(), id.clone());
    let blobs = tokio::spawn(async move {
        if host {
            return;
        }
        while let Ok(mut r) = c3.accept_uni().await {
            let (e, id) = (e3.clone(), id3.clone());
            tokio::spawn(async move {
                let mut len = [0u8; 4];
                if r.read_exact(&mut len).await.is_err() {
                    return;
                }
                let len = u32::from_be_bytes(len) as usize;
                if len > 1024 {
                    return;
                }
                let mut name = vec![0u8; len];
                if r.read_exact(&mut name).await.is_err() {
                    return;
                }
                if let Ok(bytes) = r.read_to_end(MAX_BLOB).await {
                    let _ = e.send(Event::Blob { from: id, name: String::from_utf8_lossy(&name).into_owned(), bytes });
                }
            });
        }
    });
    // Lector (termina cuando se cae la conexión)
    while let Ok(msg) = read_msg(&mut recv).await {
        let _ = events.send(Event::Message { from: id.clone(), msg });
    }
    writer.abort();
    light.abort();
    blobs.abort();
    peers.lock().await.remove(&id);
    conns.lock().await.remove(&id);
    let _ = events.send(Event::PeerLeft { id });
    Ok(())
}
