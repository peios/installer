//! installer-gxwi — the graphical Peios installer.
//!
//! It is what GXWI runs as its overlay on an install medium: one process for
//! the machine, which everyone who opens the machine's address is sent to,
//! with no logon. GXWI hands it a listening socket and it answers HTTP/2
//! there, as a compositor does.
//!
//! It installs nothing itself. installerd does that, as SYSTEM, and asks its
//! questions over a socket; this program holds that conversation (`setup`),
//! keeps what it has come to as one state (`view`), and serves a page that
//! draws it (`page`).

use std::os::fd::{AsFd, FromRawFd};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

use peios::token::ImpersonationLevel;
use tokio::sync::watch;

mod page;
mod release;
mod setup;
mod view;

/// Where peinit puts the first descriptor a job was submitted with.
const LISTENER_FD: i32 = 3;
const INSTALLERD_SOCKET: &str = "/run/installerd.sock";
/// A fresh one each time the machine starts.
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

const USAGE: &str = "usage: installer-gxwi [--socket PATH] [--listen ADDRESS:PORT]

Started by GXWI as its overlay, which hands it the socket it serves on.

  --socket PATH          installerd's socket (default /run/installerd.sock)
  --listen ADDRESS:PORT  serve here instead of on a socket handed over, for
                         working on the page where there is no GXWI
  -V, --version          print the version
  -h, --help             print this";

#[tokio::main]
async fn main() {
    let (mut socket, mut listen) = (PathBuf::from(INSTALLERD_SOCKET), None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--socket" => socket = args.next().unwrap_or_else(|| die("--socket needs a path")).into(),
            "--listen" => listen = Some(args.next().unwrap_or_else(|| die("--listen needs an address and port"))),
            "-V" | "--version" => return println!("installer-gxwi {}", env!("CARGO_PKG_VERSION")),
            "-h" | "--help" => return println!("{USAGE}"),
            other => die(&format!("unknown argument {other}\n{USAGE}")),
        }
    }

    let boot = std::fs::read_to_string(BOOT_ID).map(|id| id.trim().to_string()).unwrap_or_default();
    let (show, view) = watch::channel(view::View::starting(release::read(), boot));
    // Everything the conversation is to hear of, from installerd and from
    // browsers alike, in the order it came.
    let (tell, inbox) = std::sync::mpsc::channel();
    // What the job under way has said of itself, whole.
    let said = setup::Said::default();
    let app = page::routes(view, said.clone(), tell.clone());
    std::thread::Builder::new()
        .name("setup".into())
        .spawn(move || setup::keep(&socket, &show, &said, &tell, &inbox))
        .unwrap_or_else(|e| die(&format!("setup thread: {e}")));

    if let Some(address) = listen {
        let listener = tokio::net::TcpListener::bind(&address).await.unwrap_or_else(|e| die(&format!("listen {address}: {e}")));
        eprintln!("installer-gxwi: serving on http://{address}, which is for working on the page only");
        axum::serve(listener, app).await.unwrap_or_else(|e| die(&format!("serve: {e}")));
        return;
    }

    if std::env::var("LISTEN_FDS").ok().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0) < 1 {
        die("no socket was handed over: installer-gxwi is started by GXWI as its overlay, not by hand");
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

fn die(message: &str) -> ! {
    eprintln!("installer-gxwi: {message}");
    std::process::exit(1);
}
