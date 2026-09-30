use std::os::unix::net::UnixListener;
use std::sync::Arc;

use installerd::executor::{DryRun, Executor, Inventory};
use installerd::flow_impl::Install;
use installerd::real::Real;
use msip_serve::{Server, serve};

const USAGE: &str = "Usage: installerd [--socket PATH] [--medium PATH] [--force]\n\
       installerd --dry-run [--socket PATH] [--inventory FILE]\n\
\n\
Peios installation and repair service.\n\
\n\
Options:\n\
  --socket PATH     MSIP socket (default: /run/installerd.sock)\n\
  --medium PATH     mounted installation medium (default: /media/peios)\n\
  --force           permit replacement of an unrecognised partition table\n\
  --dry-run         exercise the flow without changing disks\n\
  --inventory FILE  with --dry-run, pretend to be the machine FILE describes\n\
                    rather than probing this one\n\
  -h, --help        show this help\n\
  -V, --version     show the version";

/// The machine a dry run is to pretend to be, from the JSON at `path`.
fn inventory(path: &str) -> Inventory {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("installerd: cannot read {path}: {e}");
        std::process::exit(2);
    });
    serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("installerd: {path} does not describe a machine: {e}");
        std::process::exit(2);
    })
}

fn required(args: &mut impl Iterator<Item = String>, option: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("installerd: {option} needs a value\n{USAGE}");
        std::process::exit(2);
    })
}

fn main() {
    let mut socket = "/run/installerd.sock".to_string();
    let mut dry_run = false;
    let mut pretend = None;
    let mut real = Real::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = required(&mut args, "--socket"),
            "--medium" => real.medium = required(&mut args, "--medium").into(),
            "--force" => real.force = true,
            "--dry-run" => dry_run = true,
            "--inventory" => pretend = Some(inventory(&required(&mut args, "--inventory"))),
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            "-V" | "--version" => {
                println!("installerd {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("installerd: unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if pretend.is_some() && !dry_run {
        // A described machine has no disks to install onto.
        eprintln!("installerd: --inventory is for --dry-run\n{USAGE}");
        std::process::exit(2);
    }
    let executor: Arc<dyn Executor> = if dry_run {
        Arc::new(DryRun {
            step_ms: 400,
            inventory: pretend,
        })
    } else {
        Arc::new(real)
    };
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).expect("bind socket");
    msip_serve::secure_socket(&socket, msip_serve::ADMIN_SOCKET_SDDL);
    eprintln!(
        "installerd{} listening on {socket}",
        if dry_run { " (dry run)" } else { "" }
    );
    let server = Server::new(
        "install",
        concat!("installerd/", env!("CARGO_PKG_VERSION")),
        move || Box::new(Install::new(Arc::clone(&executor))),
    );
    serve(listener, server).expect("serve");
}
