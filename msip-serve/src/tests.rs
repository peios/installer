use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;

use msip::element::{Element, types};
use msip::frame::{read_msg, write_msg};

use super::*;

struct Ending {
    began: mpsc::Sender<()>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl Flow for Ending {
    fn needs(&self) -> &'static [&'static str] {
        &[types::ACTION]
    }
    fn open(&mut self) -> TurnSpec {
        TurnSpec {
            elements: vec![Element::new("done", types::ACTION)],
            ..Default::default()
        }
    }
    fn advance(&mut self, _: &ValidAnswer) -> Step {
        Step::End(Outcome::Complete, None)
    }
    fn finished(&mut self, _: Outcome) {
        self.began.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
    }
}

fn connect(server: Arc<Server>) -> (UnixStream, thread::JoinHandle<()>) {
    let (socket, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let worker = thread::spawn(move || connection(socket, server).unwrap());
    write_msg(
        &mut peer,
        MsgType::Hello,
        &Hello {
            surface: None,
            element_types: vec![types::ACTION.into()],
        },
    )
    .unwrap();
    assert_eq!(read_msg(&mut peer).unwrap().0, MsgType::Welcome);
    (peer, worker)
}

#[test]
fn retirement_flushes_end_and_temporarily_refuses_new_conversations() {
    let (began, started) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let wait = Arc::new(Mutex::new(wait));
    let server = Server::new("test", "test", move || {
        Box::new(Ending {
            began: began.clone(),
            release: wait.clone(),
        })
    });
    let (mut first, first_worker) = connect(server.clone());
    write_msg(
        &mut first,
        MsgType::Start,
        &Start {
            kind: "test".into(),
        },
    )
    .unwrap();
    let (_, bound) = read_msg(&mut first).unwrap();
    let (_, turn) = read_msg(&mut first).unwrap();
    write_msg(&mut first,MsgType::Answer,&json!({"conversation":bound["conversation"],"seq":turn["seq"],"values":{},"action":"done"})).unwrap();
    assert_eq!(read_msg(&mut first).unwrap().0, MsgType::End);
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let (mut second, second_worker) = connect(server.clone());
    write_msg(
        &mut second,
        MsgType::Start,
        &Start {
            kind: "test".into(),
        },
    )
    .unwrap();
    let (kind, body) = read_msg(&mut second).unwrap();
    assert_eq!(kind, MsgType::Refused);
    let refused: Refused = serde_json::from_value(body).unwrap();
    assert_eq!(refused.reason, RefusedReason::Unavailable);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.finishing.load(Ordering::Relaxed) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    write_msg(
        &mut second,
        MsgType::Start,
        &Start {
            kind: "test".into(),
        },
    )
    .unwrap();
    assert_eq!(read_msg(&mut second).unwrap().0, MsgType::Bound);
    assert_eq!(read_msg(&mut second).unwrap().0, MsgType::Turn);
    drop(first);
    drop(second);
    first_worker.join().unwrap();
    second_worker.join().unwrap();
}
