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
//!
//! A job that fails ends the conversation, and what it ended with stays on
//! the page until someone at a browser asks to start again; then another is
//! opened in its place. A job that finishes goes on to a page of
//! installerd's that offers to restart the machine, and a restart ends the
//! conversation too: the machine goes down, this process with it, and the
//! page is left to wait for what comes back (`View::restarting`).

use std::io::ErrorKind;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use msip::element::types;
use msip::frame::{FrameError, MsgType, read_msg, write_msg};
use msip::msg::{Hello, Outcome, Start};
use msip::surface::{Event, Session};
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::sync::watch;

use crate::Conversation;
use crate::view::{self, Link, Page, View};

/// How long after installerd was lost it is tried again.
const AGAIN: Duration = Duration::from_secs(2);
/// What is under way (`View::waiting`) while a conversation asked for after
/// an ending is opened.
const STARTING_AGAIN: &str = "again";
/// installerd's action on the page after a finished job that restarts the
/// machine.
const REBOOT: &str = "act.reboot";

/// Everything the job under way has said of itself, a line each: what is
/// served as a file, where a page is sent only the last of it. Empty when no
/// job is under way or ended.
pub type Said = Arc<Mutex<Vec<String>>>;

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
    /// What was filled in on the page, by ref, sent with the action that
    /// takes it. Unlike the disk chosen this is the browser's own until
    /// then: what one person is typing is nobody else's to see.
    #[serde(default)]
    pub values: Map<String, Value>,
    /// Open another conversation, the last having ended.
    #[serde(default)]
    pub again: bool,
}

/// Holds the conversation for as long as the process runs, and keeps `view`
/// as it stands, and `said` with it. installerd not being there is said and
/// tried again: it is started by the system, and may simply not be up yet.
///
/// `inbox` is where everything arrives, and `tell` is the way into it, which
/// each connection's listener is given a copy of.
///
/// oobed's conversation is held the same way. Everything said here of
/// installerd is true of oobed there, but for jobs and a restart, which
/// oobed's pages have none of.
pub fn keep(
    conversation: Conversation,
    socket: &Path,
    view: &watch::Sender<View>,
    said: &Said,
    tell: &mpsc::Sender<Heard>,
    inbox: &mpsc::Receiver<Heard>,
) {
    let mut connection = 0;
    // Whether this is a conversation asked for after an ending. The ending
    // then stays on the page until the new conversation's first page takes
    // its place, so that nobody is shown a moment of nothing between them.
    let mut again = false;
    loop {
        connection += 1;
        view.send_modify(|view| {
            view.link = Link::Connecting;
            view.restarting = false;
            view.without_a_turn();
            if again {
                view.waiting = Some(STARTING_AGAIN.into());
            } else {
                view.page = None;
            }
            view.trail.clear();
            view.did("$", conversation.program());
            view.did("connect", socket.display().to_string());
        });
        again = false;
        match converse(conversation, socket, view, said, tell, inbox, connection) {
            // The conversation ended as conversations do. What it ended with
            // stays on the page until someone asks to start again.
            Ok(()) => {
                loop {
                    match inbox.recv() {
                        Ok(Heard::Asked(Asked { seq: 0, again: true, .. })) => break,
                        // Anything else is asked of a page that has gone, or
                        // is the last of a connection that is over.
                        Ok(_) => {}
                        // Nothing can ask any more: the process is going.
                        Err(_) => return,
                    }
                }
                again = true;
                continue;
            }
            Err(why) => {
                eprintln!("{}: {why}; trying again", conversation.program());
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
    conversation: Conversation,
    socket: &Path,
    view: &watch::Sender<View>,
    said: &Said,
    tell: &mpsc::Sender<Heard>,
    inbox: &mpsc::Receiver<Heard>,
    connection: u64,
) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket).map_err(|e| format!("connect {}: {e}", socket.display()))?;
    // What this program calls itself to the daemon, which only records it.
    let surface = format!("{}/{}", conversation.program(), env!("CARGO_PKG_VERSION"));
    let hello = Hello {
        surface: Some(surface.clone()),
        element_types: types::ALL.iter().map(|name| name.to_string()).collect(),
    };
    write_msg(&mut stream, MsgType::Hello, &hello).map_err(|e| format!("hello: {e}"))?;
    view.send_modify(|view| view.did("hello", format!("surface={surface}")));

    let listener = stream.try_clone().map_err(|e| format!("socket: {e}"))?;
    let tell = tell.clone();
    let daemon = conversation.daemon();
    std::thread::Builder::new()
        .name(daemon.into())
        .spawn(move || listen(listener, daemon, connection, &tell))
        .map_err(|e| format!("listener thread: {e}"))?;
    let outcome = talk(conversation, &mut stream, view, said, inbox, connection);
    // Whichever way it went, the listener is let go: it is reading, and this
    // is what ends its read.
    let _ = stream.shutdown(Shutdown::Both);
    outcome
}

/// Reads what installerd says on one connection and puts it in the queue,
/// until the connection is over.
fn listen(mut stream: UnixStream, daemon: &str, connection: u64, tell: &mpsc::Sender<Heard>) {
    loop {
        let heard = match read_msg(&mut stream) {
            Ok((kind, body)) => Heard::Said(connection, kind, body),
            // The ordinary way for it to go: stopped, or restarted.
            Err(FrameError::Io(e)) if e.kind() == ErrorKind::UnexpectedEof => {
                Heard::Gone(connection, format!("{daemon} closed the connection"))
            }
            Err(e) => Heard::Gone(connection, format!("{daemon} went away: {e}")),
        };
        let over = matches!(heard, Heard::Gone(..));
        if tell.send(heard).is_err() || over {
            return;
        }
    }
}

fn talk(
    conversation: Conversation,
    stream: &mut UnixStream,
    view: &watch::Sender<View>,
    said: &Said,
    inbox: &mpsc::Receiver<Heard>,
    connection: u64,
) -> Result<(), String> {
    let daemon = conversation.daemon();
    let mut session = Session::new();
    // The disk chosen on the page that asks for one. It is this process's to
    // hold: installerd hears of it only when the page is answered.
    let mut chosen: Option<String> = None;
    // The name oobed's account page was last answered with, for the naming
    // page it leads to, which oobed sends without it.
    let mut account: Option<String> = None;
    loop {
        let (kind, body) = match inbox.recv().map_err(|_| format!("nothing is listening to {daemon}"))? {
            Heard::Said(from, kind, body) if from == connection => (kind, body),
            Heard::Gone(from, why) if from == connection => return Err(why),
            // The last words of a connection already given up.
            Heard::Said(..) | Heard::Gone(..) => continue,
            Heard::Asked(asked) => {
                ask(stream, &session, &mut chosen, &mut account, view, asked)?;
                continue;
            }
        };
        match session.handle(kind, body).map_err(|e| format!("{daemon} said something unreadable: {e}"))? {
            Event::Welcome(welcome) => {
                let daemon = welcome.daemon.unwrap_or_else(|| daemon.into());
                let start = Start { kind: conversation.kind().into() };
                write_msg(stream, MsgType::Start, &start).map_err(|e| format!("start: {e}"))?;
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
                return Err(format!("{daemon} refused the conversation ({:?}) {message}", refused.reason));
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
                // What the job said stays, to be saved, on the page it goes
                // on to once it has finished.
                let done = view::is_done(turn);
                if !done {
                    hear(said, turn, matches!(event, Event::NewTurn));
                }
                view.send_modify(|view| {
                    if matches!(event, Event::NewTurn) {
                        let class = if turn.class.is_empty() { String::new() } else { format!("  class={}", turn.class.join(",")) };
                        view.did("turn", format!("{}{class}", turn.id.as_deref().unwrap_or("?")));
                    }
                    view.seq = turn.seq;
                    // Either is installerd's answer to whatever was pressed.
                    view.waiting = None;
                    view.page = Some(if done {
                        Page::done(view.page.take(), turn)
                    } else {
                        Page::of(turn, chosen.as_deref()).for_account(account.as_deref())
                    });
                });
            }
            Event::Ended(end) => {
                view.send_modify(|view| {
                    // installerd ends the conversation as its answer to the
                    // restart asked for: the machine is on its way down. The
                    // page it was asked on stays as it was.
                    if view.waiting.as_deref() == Some(REBOOT) && end.outcome == Outcome::Complete {
                        view.restarting = true;
                    } else {
                        view.page = Some(Page::ended(view.page.take(), &end));
                    }
                    view.without_a_turn();
                });
                return Ok(());
            }
            Event::ProtocolError(error) => {
                let message = error.message.unwrap_or_default();
                // Before there is a page, it is about how this program opened
                // the conversation, and there is no going on from that.
                if session.page().is_none() {
                    return Err(format!("{daemon} says this program broke the protocol ({:?}) {message}", error.code));
                }
                // With one, it is installerd turning down what was pressed.
                // The page stands as it was.
                eprintln!("{}: {daemon} turned an answer down ({:?}) {message}", conversation.program(), error.code);
                view.send_modify(|view| view.waiting = None);
            }
            // This program never asks for a listing.
            Event::Listing(_) => {}
        }
    }
}

/// Keeps what the job on `turn` has said of itself. A page's log only grows,
/// so only what is new is taken; another page starts it afresh, and one that
/// is no job's leaves it empty.
fn hear(said: &Said, turn: &msip::msg::Turn, another: bool) {
    let lines = view::said(turn);
    let mut said = said.lock().unwrap_or_else(|e| e.into_inner());
    if another || lines.len() < said.len() {
        said.clear();
    }
    let new = lines[said.len()..].iter().map(|line| line.as_str().unwrap_or_default().to_string());
    said.extend(new);
}

/// Does what a browser asked for, if it still makes sense: the page it was
/// looking at may have gone, and what it pressed may not be there to press.
/// Whatever does not make sense is dropped; the browser is sent the state as
/// it is and draws that.
fn ask(
    stream: &mut UnixStream,
    session: &Session,
    chosen: &mut Option<String>,
    account: &mut Option<String>,
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
        let values = if element.validates() {
            let mut values = view::values(turn, chosen.as_deref());
            values.extend(view::filled(turn, asked.values));
            values
        } else {
            Map::new()
        };
        if let Some(name) = answered_account(turn, &values) {
            *account = Some(name);
        }
        let Some(answer) = session.answer(Some(&action), values) else { return Ok(()) };
        write_msg(stream, MsgType::Answer, &answer).map_err(|e| format!("answer: {e}"))?;
        view.send_modify(|view| view.waiting = Some(action));
    }
    Ok(())
}

/// The account's name, as oobed takes it (trimmed), where `values` answer
/// oobed's account page. oobed goes on to the naming page only once it has
/// taken the name, so the last one answered is the account's.
fn answered_account(turn: &msip::msg::Turn, values: &Map<String, Value>) -> Option<String> {
    if turn.id.as_deref() != Some("oobe.account") {
        return None;
    }
    values.get("account.name").and_then(Value::as_str).map(|name| name.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_answered_is_kept_for_the_naming_page() {
        let turn = |id: &str| -> msip::msg::Turn {
            serde_json::from_value(serde_json::json!({
                "seq": 4, "id": id, "elements": [{ "ref": "account.name", "type": "string", "name": "User name" }],
            }))
            .unwrap()
        };
        let values = serde_json::json!({ "account.name": "  Jack Palfrey ", "account.password": "one" });
        let values = values.as_object().unwrap();
        assert_eq!(answered_account(&turn("oobe.account"), values).as_deref(), Some("Jack Palfrey"));
        // Another page's field of the same name is not the account.
        assert_eq!(answered_account(&turn("oobe.naming"), values), None);
        assert_eq!(answered_account(&turn("oobe.account"), &Map::new()), None);
    }

    #[test]
    fn what_a_browser_asks_for_is_read_whichever_it_is() {
        let none = Map::new();
        let press: Asked = serde_json::from_str(r#"{ "seq": 3, "press": "nav.back" }"#).unwrap();
        assert_eq!(press, Asked { seq: 3, press: Some("nav.back".into()), choose: None, values: none.clone(), again: false });
        let choose: Asked = serde_json::from_str(r#"{ "seq": 3, "choose": "/dev/sda" }"#).unwrap();
        assert_eq!(choose, Asked { seq: 3, press: None, choose: Some("/dev/sda".into()), values: none.clone(), again: false });
        // What was filled in goes with the action that takes it.
        let filled: Asked = serde_json::from_str(r#"{ "seq": 3, "press": "manual.save", "values": { "manual.address": "10.0.0.5/24" } }"#).unwrap();
        assert_eq!(filled.values["manual.address"], "10.0.0.5/24");
        // Starting again is asked of no page: the conversation is over.
        let again: Asked = serde_json::from_str(r#"{ "seq": 0, "again": true }"#).unwrap();
        assert_eq!(again, Asked { seq: 0, press: None, choose: None, values: none, again: true });
        // It must say which page it was looking at.
        assert!(serde_json::from_str::<Asked>(r#"{ "press": "nav.back" }"#).is_err());
        assert!(serde_json::from_str::<Asked>("press nav.back").is_err());
    }

    #[test]
    fn what_a_job_says_is_kept_whole_and_only_what_is_new_is_taken() {
        let page = |id: &str, lines: &[&str]| -> msip::msg::Turn {
            serde_json::from_value(serde_json::json!({
                "seq": 4, "id": id, "elements": [{ "ref": "out", "type": "log", "lines": lines }],
            }))
            .unwrap()
        };
        let said = Said::default();
        let kept = || said.lock().unwrap().clone();
        hear(&said, &page("install.progress", &["one"]), true);
        hear(&said, &page("install.progress", &["one", "two", "three"]), false);
        assert_eq!(kept(), ["one", "two", "three"]);
        // The same again adds nothing.
        hear(&said, &page("install.progress", &["one", "two", "three"]), false);
        assert_eq!(kept().len(), 3);
        // Another job's page starts it afresh, and a page that is no job's
        // leaves nothing.
        hear(&said, &page("repair.progress", &["four"]), true);
        assert_eq!(kept(), ["four"]);
        hear(&said, &page("mode", &[]), true);
        assert!(kept().is_empty());
    }
}
