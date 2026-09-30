//! The conversation with installerd, held here for every page at once.
//!
//! installerd owns the installation and asks its questions a page at a time
//! (MSIP, PGSS §3). This process is the one it is talking to, however many
//! browsers are looking: it takes each page installerd sends and makes it the
//! state every browser is shown, and it answers installerd when someone at a
//! browser presses something.
//!
//! One thread holds all of it. What installerd says and what browsers ask for
//! arrive on one queue and are taken in turn, so the page, the disk chosen on
//! it and the answer on its way are never looked at by two things at once.
//!
//! Not everything pressed is sent on. The conversation is installerd's and
//! shared, so an answer moves everyone along, and an action that leads to a
//! page not drawn here yet would move them to a page nobody can see. Those
//! are held back until their page exists (`view::unbuilt`).

use std::io::ErrorKind;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use msip::element::types;
use msip::frame::{FrameError, MsgType, read_msg, write_msg};
use msip::msg::{Hello, Start};
use msip::surface::{Event, Session};
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::sync::watch;

use crate::view::{self, Link, Page, View};

/// The kind of conversation this is a front end to.
const KIND: &str = "install";
/// What this program calls itself to installerd, which only records it.
const SURFACE: &str = concat!("installer-gxwi/", env!("CARGO_PKG_VERSION"));
/// How long after installerd was lost it is tried again.
const AGAIN: Duration = Duration::from_secs(2);

/// What reaches the thread that holds the conversation.
pub enum Heard {
    /// installerd said something, on the connection of this number.
    Said(u64, MsgType, Value),
    /// That connection is over, and why.
    Gone(u64, String),
    /// Someone at a browser asked for something.
    Asked(Asked),
}

/// What a browser asks for. `seq` is the page it was looking at when it
/// asked, as `View::seq` told it.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Asked {
    pub seq: u64,
    /// Press this action of the page.
    #[serde(default)]
    pub press: Option<String>,
    /// Choose this disk, on the page that asks for one.
    #[serde(default)]
    pub choose: Option<String>,
}

/// Holds the conversation for as long as the process runs, and keeps `view`
/// as it stands. installerd not being there is said and tried again: it is
/// started by the system, and may simply not be up yet.
///
/// `inbox` is where everything arrives, and `tell` is the way into it, which
/// each connection's listener is given a copy of.
pub fn keep(socket: &Path, view: &watch::Sender<View>, tell: &mpsc::Sender<Heard>, inbox: &mpsc::Receiver<Heard>) {
    let mut connection = 0;
    loop {
        connection += 1;
        view.send_modify(|view| {
            view.link = Link::Connecting;
            view.page = None;
            view.without_a_turn();
            view.trail.clear();
            view.did("$", "installer-gxwi");
            view.did("connect", socket.display().to_string());
        });
        match converse(socket, view, tell, inbox, connection) {
            // The conversation ended as conversations do. What it ended with
            // stays on the page.
            Ok(()) => return,
            Err(why) => {
                eprintln!("installer-gxwi: {why}; trying again");
                view.send_modify(|view| {
                    view.link = Link::Lost { why };
                    view.page = None;
                    view.without_a_turn();
                });
            }
        }
        // Whatever is asked for while installerd is away is asked of nobody.
        let again = Instant::now() + AGAIN;
        while let Some(left) = again.checked_duration_since(Instant::now()) {
            let _ = inbox.recv_timeout(left);
        }
    }
}

/// One connection to installerd, from its opening to the end of the
/// conversation. An error is why the connection was given up.
fn converse(
    socket: &Path,
    view: &watch::Sender<View>,
    tell: &mpsc::Sender<Heard>,
    inbox: &mpsc::Receiver<Heard>,
    connection: u64,
) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket).map_err(|e| format!("connect {}: {e}", socket.display()))?;
    let hello = Hello {
        surface: Some(SURFACE.into()),
        element_types: types::ALL.iter().map(|name| name.to_string()).collect(),
    };
    write_msg(&mut stream, MsgType::Hello, &hello).map_err(|e| format!("hello: {e}"))?;
    view.send_modify(|view| view.did("hello", format!("surface={SURFACE}")));

    let listener = stream.try_clone().map_err(|e| format!("socket: {e}"))?;
    let tell = tell.clone();
    std::thread::Builder::new()
        .name("installerd".into())
        .spawn(move || listen(listener, connection, &tell))
        .map_err(|e| format!("listener thread: {e}"))?;
    let outcome = talk(&mut stream, view, inbox, connection);
    // Whichever way it went, the listener is let go: it is reading, and this
    // is what ends its read.
    let _ = stream.shutdown(Shutdown::Both);
    outcome
}

/// Reads what installerd says on one connection and puts it in the queue,
/// until the connection is over.
fn listen(mut stream: UnixStream, connection: u64, tell: &mpsc::Sender<Heard>) {
    loop {
        let heard = match read_msg(&mut stream) {
            Ok((kind, body)) => Heard::Said(connection, kind, body),
            // The ordinary way for it to go: stopped, or restarted.
            Err(FrameError::Io(e)) if e.kind() == ErrorKind::UnexpectedEof => {
                Heard::Gone(connection, "installerd closed the connection".into())
            }
            Err(e) => Heard::Gone(connection, format!("installerd went away: {e}")),
        };
        let over = matches!(heard, Heard::Gone(..));
        if tell.send(heard).is_err() || over {
            return;
        }
    }
}

fn talk(
    stream: &mut UnixStream,
    view: &watch::Sender<View>,
    inbox: &mpsc::Receiver<Heard>,
    connection: u64,
) -> Result<(), String> {
    let mut session = Session::new();
    // The disk chosen on the page that asks for one. It is this process's to
    // hold: installerd hears of it only when the page is answered.
    let mut chosen: Option<String> = None;
    loop {
        let (kind, body) = match inbox.recv().map_err(|_| "nothing is listening to installerd".to_string())? {
            Heard::Said(from, kind, body) if from == connection => (kind, body),
            Heard::Gone(from, why) if from == connection => return Err(why),
            // The last words of a connection already given up.
            Heard::Said(..) | Heard::Gone(..) => continue,
            Heard::Asked(asked) => {
                ask(stream, &session, &mut chosen, view, asked)?;
                continue;
            }
        };
        match session.handle(kind, body).map_err(|e| format!("installerd said something unreadable: {e}"))? {
            Event::Welcome(welcome) => {
                let daemon = welcome.daemon.unwrap_or_else(|| "installerd".into());
                write_msg(stream, MsgType::Start, &Start { kind: KIND.into() }).map_err(|e| format!("start: {e}"))?;
                view.send_modify(|view| {
                    view.did("welcome", daemon.clone());
                    view.link = Link::Connected { daemon };
                });
            }
            Event::Bound(bound) => view.send_modify(|view| {
                let how = if bound.attached { "joined" } else { "opened" };
                view.did(how, format!("conversation {}", bound.conversation));
            }),
            Event::Refused(refused) => {
                let message = refused.message.unwrap_or_default();
                return Err(format!("installerd refused the conversation ({:?}) {message}", refused.reason));
            }
            event @ (Event::NewTurn | Event::Updated) => {
                let Some(page) = session.page() else { continue };
                let turn = &page.turn;
                if matches!(event, Event::NewTurn) {
                    // Nothing, but on a page come back to, which arrives
                    // with the disk that was chosen on it.
                    chosen = view::assumed(turn);
                } else {
                    // A rescan may have taken the chosen disk away.
                    chosen = chosen.filter(|disk| view::can_choose(turn, disk));
                }
                view.send_modify(|view| {
                    if matches!(event, Event::NewTurn) {
                        let class = if turn.class.is_empty() { String::new() } else { format!("  class={}", turn.class.join(",")) };
                        view.did("turn", format!("{}{class}", turn.id.as_deref().unwrap_or("?")));
                    }
                    view.seq = turn.seq;
                    // Either is installerd's answer to whatever was pressed.
                    view.waiting = None;
                    view.page = Some(Page::of(turn, chosen.as_deref()));
                });
            }
            Event::Ended(end) => {
                view.send_modify(|view| {
                    view.without_a_turn();
                    view.page = Some(Page::ended(&end));
                });
                return Ok(());
            }
            Event::ProtocolError(error) => {
                let message = error.message.unwrap_or_default();
                // Before there is a page, it is about how this program opened
                // the conversation, and there is no going on from that.
                if session.page().is_none() {
                    return Err(format!("installerd says this program broke the protocol ({:?}) {message}", error.code));
                }
                // With one, it is installerd turning down what was pressed.
                // The page stands as it was.
                eprintln!("installer-gxwi: installerd turned an answer down ({:?}) {message}", error.code);
                view.send_modify(|view| view.waiting = None);
            }
            // This program never asks for a listing.
            Event::Listing(_) => {}
        }
    }
}

/// Does what a browser asked for, if it still makes sense: the page it was
/// looking at may have gone, and what it pressed may not be there to press.
/// Whatever does not make sense is dropped; the browser is sent the state as
/// it is and draws that.
fn ask(
    stream: &mut UnixStream,
    session: &Session,
    chosen: &mut Option<String>,
    view: &watch::Sender<View>,
    asked: Asked,
) -> Result<(), String> {
    let Some(page) = session.page() else { return Ok(()) };
    let turn = &page.turn;
    // One answer at a time: while one is on its way the page is as good as
    // gone, since installerd is about to say what becomes of it.
    if asked.seq != turn.seq || view.borrow().waiting.is_some() {
        return Ok(());
    }
    if let Some(disk) = asked.choose
        && view::can_choose(turn, &disk)
        && chosen.as_deref() != Some(disk.as_str())
    {
        *chosen = Some(disk);
        view.send_modify(|view| view.page = Some(Page::of(turn, chosen.as_deref())));
    }
    if let Some(action) = asked.press {
        let Some(element) = page.element(&action).filter(|e| e.is_action() && e.enabled) else {
            return Ok(());
        };
        if view::unbuilt(turn, &action) {
            return Ok(());
        }
        // An action that does not validate is one that leaves the page or
        // refreshes it, and carries nothing of what was filled in.
        let values = if element.validates() { view::values(turn, chosen.as_deref()) } else { Map::new() };
        let Some(answer) = session.answer(Some(&action), values) else { return Ok(()) };
        write_msg(stream, MsgType::Answer, &answer).map_err(|e| format!("answer: {e}"))?;
        view.send_modify(|view| view.waiting = Some(action));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_browser_asks_for_is_read_whichever_it_is() {
        let press: Asked = serde_json::from_str(r#"{ "seq": 3, "press": "nav.back" }"#).unwrap();
        assert_eq!(press, Asked { seq: 3, press: Some("nav.back".into()), choose: None });
        let choose: Asked = serde_json::from_str(r#"{ "seq": 3, "choose": "/dev/sda" }"#).unwrap();
        assert_eq!(choose, Asked { seq: 3, press: None, choose: Some("/dev/sda".into()) });
        // It must say which page it was looking at.
        assert!(serde_json::from_str::<Asked>(r#"{ "press": "nav.back" }"#).is_err());
        assert!(serde_json::from_str::<Asked>("press nav.back").is_err());
    }
}
