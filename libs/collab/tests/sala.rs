use std::time::Duration;

use pinocchio_collab::{Event, Message, Options, Room};
use tokio::time::timeout;

async fn next(room: &Room) -> Event {
    timeout(Duration::from_secs(10), room.recv()).await.expect("tiempo").expect("sala cerrada")
}

fn local(name: &str) -> Options {
    Options { internet: false, ..Options::new(name, "#ff8800") }
}

#[tokio::test]
async fn anfitrion_e_invitado_se_hablan() {
    let host = Room::host(local("Ana")).await.unwrap();
    let guest = Room::join(&host.ticket(), local("Beto")).await.unwrap();

    // Se ven mutuamente
    let peer_of_guest = match next(&host).await {
        Event::PeerJoined { name, id, .. } => {
            assert_eq!(name, "Beto");
            id
        }
        e => panic!("{e:?}"),
    };
    match next(&guest).await {
        Event::PeerJoined { name, .. } => assert_eq!(name, "Ana"),
        e => panic!("{e:?}"),
    }

    // Fiable: invitado -> anfitrión y de vuelta
    guest.send(Message::Ping { n: 7 }).await.unwrap();
    match next(&host).await {
        Event::Message { msg: Message::Ping { n }, from } => {
            assert_eq!(n, 7);
            host.send_to(&from, Message::Pong { n }).await.unwrap();
        }
        e => panic!("{e:?}"),
    }
    match next(&guest).await {
        Event::Message { msg: Message::Pong { n }, .. } => assert_eq!(n, 7),
        e => panic!("{e:?}"),
    }

    // Anfitrión a todos
    host.send(Message::Chat { text: "hola".into() }).await.unwrap();
    match next(&guest).await {
        Event::Message { msg: Message::Chat { text }, .. } => assert_eq!(text, "hola"),
        e => panic!("{e:?}"),
    }

    // Liviano (datagrama)
    guest.send_light(b"cursor:1,2,3").await;
    match next(&host).await {
        Event::Light { bytes, .. } => assert_eq!(bytes, b"cursor:1,2,3"),
        e => panic!("{e:?}"),
    }

    // Archivo grande (3 MB) del anfitrión al invitado
    let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    host.send_blob(&peer_of_guest, "project", &big).await.unwrap();
    match next(&guest).await {
        Event::Blob { name, bytes, .. } => {
            assert_eq!(name, "project");
            assert_eq!(bytes, big);
        }
        e => panic!("{e:?}"),
    }

    // Se va el invitado
    guest.close().await;
    match next(&host).await {
        Event::PeerLeft { .. } => {}
        e => panic!("{e:?}"),
    }
}

#[test]
fn ticket_invalido_se_rechaza() {
    assert!(pinocchio_collab::decode_ticket("esto no es un ticket").is_err());
}

#[tokio::test]
async fn sin_la_clave_no_se_entra() {
    let host = Room::host(local("Ana")).await.unwrap();
    // Mismo anfitrión, otra clave: se arma el ticket a mano
    let (addr, _) = pinocchio_collab::decode_ticket(&host.ticket()).unwrap();
    let falso = pinocchio_collab::encode_ticket(&addr, "0123456789abcdef0123456789abcdef").unwrap();
    let intruso = Room::join(&falso, local("Intruso")).await.unwrap();
    match next(&intruso).await {
        Event::Rejected { .. } => {}
        e => panic!("{e:?}"),
    }
    // El anfitrión no vio entrar a nadie
    assert!(timeout(Duration::from_millis(500), host.recv()).await.is_err());
}

#[tokio::test]
async fn expulsar_y_cambiar_la_clave() {
    let host = Room::host(local("Ana")).await.unwrap();
    let viejo = host.ticket();
    let guest = Room::join(&viejo, local("Beto")).await.unwrap();
    let id = match next(&host).await {
        Event::PeerJoined { id, .. } => id,
        e => panic!("{e:?}"),
    };
    let _ = next(&guest).await; // su PeerJoined (el anfitrión)

    // Expulsado: los dos lados ven que se fue
    host.kick(&id).await;
    match next(&host).await {
        Event::PeerLeft { .. } => {}
        e => panic!("{e:?}"),
    }
    match next(&guest).await {
        Event::PeerLeft { .. } => {}
        e => panic!("{e:?}"),
    }

    // Con la clave cambiada el ticket viejo ya no sirve y el nuevo sí
    let nuevo = host.rotate_key();
    assert_ne!(viejo, nuevo);
    let tarde = Room::join(&viejo, local("Beto otra vez")).await.unwrap();
    match next(&tarde).await {
        Event::Rejected { .. } => {}
        e => panic!("{e:?}"),
    }
    let bien = Room::join(&nuevo, local("Carla")).await.unwrap();
    match next(&host).await {
        Event::PeerJoined { name, .. } => assert_eq!(name, "Carla"),
        e => panic!("{e:?}"),
    }
    drop(bien);
}

#[tokio::test]
async fn con_aprobacion_el_invitado_no_recibe_hasta_ser_admitido() {
    let host = Room::host(Options { approval: true, ..local("Ana") }).await.unwrap();
    let guest = Room::join(&host.ticket(), local("Beto")).await.unwrap();
    let id = match next(&host).await {
        Event::PeerJoined { id, .. } => id,
        e => panic!("{e:?}"),
    };
    let _ = next(&guest).await;
    // Sin admitir: lo que el anfitrión reparte no le llega
    host.send(Message::Chat { text: "secreto".into() }).await.unwrap();
    assert!(timeout(Duration::from_millis(400), guest.recv()).await.is_err());
    // Admitido: ahora sí
    host.set_muted(&id, false);
    host.send(Message::Chat { text: "ya puedes".into() }).await.unwrap();
    match next(&guest).await {
        Event::Message { msg: Message::Chat { text }, .. } => assert_eq!(text, "ya puedes"),
        e => panic!("{e:?}"),
    }
}
