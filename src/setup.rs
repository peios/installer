//! The conversation with installerd, held here for every page at once.
//!
//! installerd owns the installation and asks its questions a page at a time
//! (MSIP, PGSS §3). This process is the one it is talking to, however many
//! browsers are looking: it takes each page installerd sends and makes it the
//! state every browser is shown.
//!
//! Nothing a browser does goes back to installerd yet, because nothing past
//! the first page is drawn: a page that answered would move the conversation
//! on to a page nobody could see.

use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use msip::element::types;
use msip::frame::{FrameError, MsgType, read_msg, write_msg};
use msip::msg::{Hello, Start};
use msip::surface::{Event, Session};
use tokio::sync::watch;

use crate::view::{Link, Page, View};

/// The kind of conversation this is a front end to.
const KIND: &str = "install";
/// What this program calls itself to installerd, which only records it.
const SURFACE: &str = concat!("installer-gxwi/", env!("CARGO_PKG_VERSION"));
/// How long after installerd was lost it is tried again.
const AGAIN: Duration = Duration::from_secs(2);

/// Holds the conversation for as long as the process runs, and keeps `view`
/// as it stands. installerd not being there is said and tried again: it is
/// started by the system, and may simply not be up yet.
pub fn keep(socket: &Path, view: &watch::Sender<View>) {
    loop {
        view.send_modify(|view| {
            view.link = Link::Connecting;
            view.page = None;
            view.trail.clear();
            view.did("$", "installer-gxwi");
            view.did("connect", socket.display().to_string());
        });
        match converse(socket, view) {
            // The conversation ended as conversations do. What it ended with
            // stays on the page.
            Ok(()) => return,
            Err(why) => {
                eprintln!("installer-gxwi: {why}; trying again");
                view.send_modify(|view| {
                    view.link = Link::Lost { why };
                    view.page = None;
                });
            }
        }
        std::thread::sleep(AGAIN);
    }
}

/// One connection to installerd, from its opening to the end of the
/// conversation. An error is why the connection was given up.
fn converse(socket: &Path, view: &watch::Sender<View>) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket).map_err(|e| format!("connect {}: {e}", socket.display()))?;
    let hello = Hello {
        surface: Some(SURFACE.into()),
        element_types: types::ALL.iter().map(|name| name.to_string()).collect(),
    };
    write_msg(&mut stream, MsgType::Hello, &hello).map_err(|e| format!("hello: {e}"))?;
    view.send_modify(|view| view.did("hello", format!("surface={SURFACE}")));

    let mut session = Session::new();
    loop {
        let (kind, body) = read_msg(&mut stream).map_err(|e| match e {
            // The ordinary way for it to go: stopped, or restarted.
            FrameError::Io(e) if e.kind() == ErrorKind::UnexpectedEof => "installerd closed the connection".to_string(),
            e => format!("installerd went away: {e}"),
        })?;
        match session.handle(kind, body).map_err(|e| format!("installerd said something unreadable: {e}"))? {
            Event::Welcome(welcome) => {
                let daemon = welcome.daemon.unwrap_or_else(|| "installerd".into());
                write_msg(&mut stream, MsgType::Start, &Start { kind: KIND.into() })
                    .map_err(|e| format!("start: {e}"))?;
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
                view.send_modify(|view| {
                    if matches!(event, Event::NewTurn) {
                        let class = if turn.class.is_empty() { String::new() } else { format!("  class={}", turn.class.join(",")) };
                        view.did("turn", format!("{}{class}", turn.id.as_deref().unwrap_or("?")));
                    }
                    view.page = Some(Page::of(turn));
                });
            }
            Event::Ended(end) => {
                view.send_modify(|view| view.page = Some(Page::ended(&end)));
                return Ok(());
            }
            Event::ProtocolError(error) => {
                let message = error.message.unwrap_or_default();
                return Err(format!("installerd says this program broke the protocol ({:?}) {message}", error.code));
            }
            // This program never asks for a listing.
            Event::Listing(_) => {}
        }
    }
}
