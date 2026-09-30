//! installer-gxwi and oobe-gxwi — Peios setup, in a browser.
//!
//! Each is what GXWI runs as its overlay: one process for the machine, which
//! everyone who opens the machine's address is sent to, with no logon. GXWI
//! hands it a listening socket and it answers HTTP/2 there, as a compositor
//! does. `installer-gxwi` is the overlay on an install medium, and
//! `oobe-gxwi` on an installed machine that has not been set up yet.
//!
//! Neither does anything to the machine itself. installerd installs, and
//! oobed makes the first account and names the machine, each as SYSTEM, and
//! each asks its questions over a socket; this holds that conversation
//! (`setup`), keeps what it has come to as one state (`view`), and serves a
//! page that draws it (`page`). The two are one program because they are one
//! page: the same mark, the same stars, and a restart from the one lands in
//! the other.

use std::os::fd::{AsFd, FromRawFd};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

use peios::token::ImpersonationLevel;
use serde::Serialize;
use tokio::sync::watch;

mod page;
mod release;
mod setup;
mod view;

/// Where peinit puts the first descriptor a job was submitted with.
const LISTENER_FD: i32 = 3;
/// A fresh one each time the machine starts.
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

/// Which conversation this is a front end to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Conversation {
    /// installerd's: installing Peios, from the medium.
    Install,
    /// oobed's: first-boot setup, on the machine installed.
    Oobe,
}

impl Conversation {
    /// What this program is called.
    pub fn program(self) -> &'static str {
        match self {
            Conversation::Install => "installer-gxwi",
            Conversation::Oobe => "oobe-gxwi",
        }
    }

    /// Who holds the conversation.
    pub fn daemon(self) -> &'static str {
        match self {
            Conversation::Install => "installerd",
            Conversation::Oobe => "oobed",
        }
    }

    /// Where it listens.
    fn socket(self) -> &'static str {
        match self {
            Conversation::Install => "/run/installerd.sock",
            Conversation::Oobe => "/run/oobed.sock",
        }
    }

    /// The kind of conversation asked for (MSIP `start`).
    fn kind(self) -> &'static str {
        match self {
            Conversation::Install => "install",
            Conversation::Oobe => "oobe",
        }
    }
}

/// Runs the program that is a front end to `conversation`, reading its
/// command line.
#[tokio::main]
pub async fn run(conversation: Conversation) {
    let program = conversation.program();
    let daemon = conversation.daemon();
    let usage = format!(
        "usage: {program} [--socket PATH] [--listen ADDRESS:PORT]

Started by GXWI as its overlay, which hands it the socket it serves on.

  --socket PATH          {daemon}'s socket (default {socket})
  --listen ADDRESS:PORT  serve here instead of on a socket handed over, for
                         working on the page where there is no GXWI
  -V, --version          print the version
  -h, --help             print this",
        socket = conversation.socket(),
    );
    let die = |message: &str| -> ! {
        eprintln!("{program}: {message}");
        std::process::exit(1);
    };
    let (mut socket, mut listen) = (PathBuf::from(conversation.socket()), None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--socket" => socket = args.next().unwrap_or_else(|| die("--socket needs a path")).into(),
            "--listen" => listen = Some(args.next().unwrap_or_else(|| die("--listen needs an address and port"))),
            "-V" | "--version" => return println!("{program} {}", env!("CARGO_PKG_VERSION")),
            "-h" | "--help" => return println!("{usage}"),
            other => die(&format!("unknown argument {other}\n{usage}")),
        }
    }

    let boot = std::fs::read_to_string(BOOT_ID).map(|id| id.trim().to_string()).unwrap_or_default();
    let (show, view) = watch::channel(view::View::starting(conversation, release::read(), boot));
    // Everything the conversation is to hear of, from the daemon and from
    // browsers alike, in the order it came.
    let (tell, inbox) = std::sync::mpsc::channel();
    // What the job under way has said of itself, whole.
    let said = setup::Said::default();
    let app = page::routes(conversation, view, said.clone(), tell.clone());
    std::thread::Builder::new()
        .name("setup".into())
        .spawn(move || setup::keep(conversation, &socket, &show, &said, &tell, &inbox))
        .unwrap_or_else(|e| die(&format!("setup thread: {e}")));

    if let Some(address) = listen {
        let listener = tokio::net::TcpListener::bind(&address).await.unwrap_or_else(|e| die(&format!("listen {address}: {e}")));
        eprintln!("{program}: serving on http://{address}, which is for working on the page only");
        axum::serve(listener, app).await.unwrap_or_else(|e| die(&format!("serve: {e}")));
        return;
    }

    if std::env::var("LISTEN_FDS").ok().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0) < 1 {
        die(&format!("no socket was handed over: {program} is started by GXWI as its overlay, not by hand"));
    }
    // SAFETY: peinit placed this descriptor for us and nothing else owns it.
    let listener = unsafe { UnixListener::from_raw_fd(LISTENER_FD) };
    // gxwid bound this socket, so it still answers in gxwid's name. gxwid
    // waits for it to answer in ours before it sends anyone here.
    ImpersonationLevel::restamp_listener(listener.as_fd()).unwrap_or_else(|e| die(&format!("restamp: {e}")));
    listener.set_nonblocking(true).unwrap_or_else(|e| die(&format!("listener: {e}")));
    let listener = tokio::net::UnixListener::from_std(listener).unwrap_or_else(|e| die(&format!("listener: {e}")));
    // Only SYSTEM can reach the socket, by the descriptor gxwid put on it, so
    // there is nobody to check for here.
    axum::serve(listener, app).await.unwrap_or_else(|e| die(&format!("serve: {e}")));
}
