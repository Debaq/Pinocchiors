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
    let guest = Room::join(host.ticket(), local("Beto")).await.unwrap();

    // Se ven mutuamente
    match next(&host).await {
        Event::PeerJoined { name, .. } => assert_eq!(name, "Beto"),
        e => panic!("{e:?}"),
    }
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
