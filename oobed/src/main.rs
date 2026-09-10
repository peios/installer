use std::os::unix::net::UnixListener;
use std::sync::Arc;

use msip_serve::{ADMIN_SOCKET_SDDL, Server, serve};
use oobed::TAG;
use oobed::flow_impl::Oobe;
use oobed::setup::{DryRun, Real, Setup};

const USAGE: &str = "Usage: oobed [--socket PATH] [--dry-run]\n\
\n\
Peios first-boot setup service.\n\
\n\
Options:\n\
  --socket PATH   MSIP socket (default: /run/oobed.sock)\n\
  --dry-run       exercise setup without changing the machine\n\
  -h, --help      show this help\n\
  -V, --version   show the version";

fn required(args: &mut impl Iterator<Item = String>, option: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("oobed: {option} needs a value\n{USAGE}");
        std::process::exit(2);
    })
}

fn main() {
    let mut socket = "/run/oobed.sock".to_string();
    let mut dry_run = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = required(&mut args, "--socket"),
            "--dry-run" => dry_run = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            "-V" | "--version" => {
                println!("oobed {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("oobed: unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    let setup: Arc<dyn Setup> = if dry_run {
        Arc::new(DryRun)
    } else {
        Arc::new(Real)
    };
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).expect("bind socket");
    msip_serve::secure_socket(&socket, ADMIN_SOCKET_SDDL);
    msip_serve::note(
        TAG,
        &format!(
            "listening on {socket}{}",
            if dry_run { " (dry run)" } else { "" }
        ),
    );
    // Before accepting: a surface is held by `Requires` until this
    // arrives, and telling peinit we are ready before the socket exists
    // would hand it a race it cannot see.
    msip_serve::notify_ready();
    let server = Server::new(
        "oobe",
        concat!("oobed/", env!("CARGO_PKG_VERSION")),
        move || Box::new(Oobe::new(Arc::clone(&setup))),
    );
    serve(listener, server).expect("serve");
}
