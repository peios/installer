use std::sync::mpsc;

use axum::Router;
use axum::routing::any;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as ClientMessage;

use super::*;
use crate::Conversation;
use crate::input::MAX_MESSAGE_BYTES;
use crate::release::Release;
use crate::setup::Said;

struct Fixture {
    address: std::net::SocketAddr,
    view: watch::Sender<View>,
    inbox: mpsc::Receiver<Heard>,
    admission: Admission,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    async fn new() -> Self {
        let mut initial = View::starting(Conversation::Install, Release::default(), "test".into());
        initial.generation = 1;
        initial.seq = 1;
        let (view, receive) = watch::channel(initial);
        let (tell, inbox) = mpsc::channel();
        let admission = Admission::default();
        let served = Served {
            view: receive,
            tell,
            said: Said::default(),
            admission: admission.clone(),
        };
        let router = Router::new()
            .route("/live", any(crate::page::live))
            .with_state(served);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            address,
            view,
            inbox,
            admission,
            server,
        }
    }

    async fn connect(
        &self,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://{}/live", self.address))
                .await
                .unwrap();
        let initial = socket.next().await.unwrap().unwrap();
        assert!(initial.is_text());
        assert!(!initial.to_text().unwrap().contains("generation"));
        socket
    }

    async fn fill(&self) -> Vec<BrowserInput> {
        let mut held = Vec::new();
        for _ in 0..4 {
            held.push(
                self.admission
                    .admit("x".repeat(MAX_MESSAGE_BYTES - 128), 1)
                    .await
                    .unwrap(),
            );
        }
        held
    }
}

async fn receive(
    socket: &mut (
             impl futures_util::Stream<
        Item = Result<ClientMessage, tokio_tungstenite::tungstenite::Error>,
    > + Unpin
         ),
) -> ClientMessage {
    tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn full_budget_still_publishes_updates_and_cancels_on_generation_change() {
    let fixture = Fixture::new().await;
    let held = fixture.fill().await;
    let mut socket = fixture.connect().await;
    socket
        .send(ClientMessage::Text(
            r#"{"seq":1,"press":"nav.next"}"#.into(),
        ))
        .await
        .unwrap();
    socket
        .send(ClientMessage::Text(
            r#"{"seq":1,"press":"buffered-old-action"}"#.into(),
        ))
        .await
        .unwrap();
    fixture.view.send_modify(|view| view.seq = 2);
    let update = receive(&mut socket).await;
    assert_eq!(
        serde_json::from_str::<Value>(update.to_text().unwrap()).unwrap()["seq"],
        2
    );
    fixture.view.send_modify(|view| {
        view.generation = 2;
        view.seq = 1;
    });
    let ClientMessage::Close(Some(close)) = receive(&mut socket).await else {
        panic!("not closed")
    };
    assert_eq!(u16::from(close.code), 1012);
    drop(held);
    assert!(fixture.inbox.try_recv().is_err());
    let mut replacement = fixture.connect().await;
    replacement
        .send(ClientMessage::Text(r#"{"seq":1}"#.into()))
        .await
        .unwrap();
    let input = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(Heard::Asked(input)) = fixture.inbox.try_recv() {
                break input;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(input.decode(1).is_none());
    assert_eq!(input.decode(2).unwrap().seq, 1);
}

#[tokio::test]
async fn observed_close_while_full_discards_the_waiting_request() {
    let fixture = Fixture::new().await;
    let held = fixture.fill().await;
    let mut socket = fixture.connect().await;
    socket
        .feed(ClientMessage::Text(
            r#"{"seq":1,"press":"nav.next"}"#.into(),
        ))
        .await
        .unwrap();
    socket.feed(ClientMessage::Close(None)).await.unwrap();
    socket.flush().await.unwrap();
    // Flush both frames into the server while it cannot admit the first.
    tokio::time::sleep(Duration::from_millis(30)).await;
    drop(held);
    // Peer close is observed and the pending input is discarded. A best-effort
    // close reply may race TCP teardown; do not require a completed handshake.
    let closed = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap();
    assert!(!matches!(closed, Some(Ok(ClientMessage::Text(_)))));
    assert!(fixture.inbox.try_recv().is_err());
}

#[tokio::test]
async fn buffered_actions_keep_order_and_local_oversize_refusal_closes_only_that_browser() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.connect().await;
    for seq in [1, 2] {
        socket
            .send(ClientMessage::Text(format!("{{\"seq\":{seq}}}").into()))
            .await
            .unwrap();
    }
    for expected in [1, 2] {
        let mut input = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(Heard::Asked(input)) = fixture.inbox.try_recv() {
                    break input;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(input.decode(1).unwrap().seq, expected);
        if expected == 2 {
            input.reject_oversize();
        }
    }
    let ClientMessage::Close(Some(close)) = receive(&mut socket).await else {
        panic!("not closed")
    };
    assert_eq!(u16::from(close.code), 1009);
    assert_eq!(fixture.view.borrow().generation, 1);
}

#[tokio::test]
async fn oversized_websocket_message_never_enters_the_admitted_queue() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.connect().await;
    socket
        .send(ClientMessage::Text(
            "x".repeat(MAX_MESSAGE_BYTES + 1).into(),
        ))
        .await
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap();
    assert!(!matches!(result, Some(Ok(ClientMessage::Text(_)))));
    assert!(fixture.inbox.try_recv().is_err());
}

#[tokio::test]
async fn non_reading_browser_writes_are_bounded_and_drop_their_future() {
    struct Guard(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let guard = Guard(dropped.clone());
    let blocked = async {
        let _guard = guard;
        std::future::pending::<Result<(), ()>>().await
    };
    assert!(!bounded(blocked, Duration::from_millis(10)).await);
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
}

use serde_json::Value;
