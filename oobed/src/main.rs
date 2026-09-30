use std::os::unix::net::UnixListener;
use std::sync::Arc;

use std::path::PathBuf;

use msip_serve::{Server, serve};
use oobed::TAG;
use oobed::flow_impl::Oobe;
use oobed::setup::{DryRun, Real, Setup, socket_sddl};

/// The program that draws setup in a browser, as GXWI's overlay.
const BROWSER: &str = "/bin/oobe-gxwi";

const USAGE: &str = "Usage: oobed [--socket PATH] [--browser PATH | --no-browser] [--dry-run]\n\
\n\
Peios first-boot setup service.\n\
\n\
Options:\n\
  --socket PATH    MSIP socket (default: /run/oobed.sock)\n\
  --browser PATH   the program GXWI runs to draw setup in a browser\n\
                   (default: /bin/oobe-gxwi, where it is installed)\n\
  --no-browser     setup on the console only\n\
  --dry-run        exercise setup without changing the machine\n\
  -h, --help       show this help\n\
  -V, --version    show the version";

fn required(args: &mut impl Iterator<Item = String>, option: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("oobed: {option} needs a value\n{USAGE}");
        std::process::exit(2);
    })
}

fn main() {
    let mut socket = "/run/oobed.sock".to_string();
    let mut dry_run = false;
    let mut browser = Some(PathBuf::from(BROWSER));
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = required(&mut args, "--socket"),
            "--browser" => browser = Some(required(&mut args, "--browser").into()),
            "--no-browser" => browser = None,
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

    // A dry run touches nothing, and letting browsers in means making an
    // account and setting GXWI's overlay: it does neither, and a browser is
    // pointed at it by hand.
    let real = (!dry_run).then(|| Arc::new(Real::new(browser)));
    let setup: Arc<dyn Setup> = match &real {
        Some(real) => Arc::clone(real) as Arc<dyn Setup>,
        None => Arc::new(DryRun),
    };
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).expect("bind socket");
    let visitor = real.as_ref().and_then(|real| real.welcome_browsers());
    msip_serve::secure_socket(&socket, &socket_sddl(visitor.as_deref()));
    // Only now that the socket admits it: GXWI starts the overlay as soon
    // as this is set, and it connects at once.
    if let Some(real) = &real {
        real.show_to_browsers();
    }
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
