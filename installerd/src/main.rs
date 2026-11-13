use std::os::unix::net::UnixListener;
use std::sync::Arc;

use installerd::executor::{DryRun, Executor, Inventory, JobKind};
use installerd::flow_impl::Install;
use installerd::real::Real;
use msip_serve::{Server, serve};

const USAGE: &str = "Usage: installerd [--socket PATH] [--medium PATH] [--force]\n\
       installerd --install DISK [--medium PATH] [--force]\n\
       installerd --dry-run [--socket PATH] [--inventory FILE] [--fail-at PHASE]\n\
\n\
Peios installation and repair service.\n\
\n\
Options:\n\
  --socket PATH     MSIP socket (default: /run/installerd.sock)\n\
  --medium PATH     mounted installation medium (default: /media/peios)\n\
  --force           permit replacement of an unrecognised partition table\n\
  --install DISK    install onto DISK now, erasing it, and report progress\n\
                    on standard output, rather than serving surfaces\n\
  --dry-run         exercise the flow without changing disks\n\
  --inventory FILE  with --dry-run, pretend to be the machine FILE describes\n\
                    rather than probing this one\n\
  --fail-at PHASE   with --dry-run, fail every job part way through this\n\
                    phase (partition, format, copy, boot, ...)\n\
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
    let mut fail_at = None;
    let mut install = None;
    let mut real = Real::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = required(&mut args, "--socket"),
            "--medium" => real.medium = required(&mut args, "--medium").into(),
            "--force" => real.force = true,
            "--install" => install = Some(required(&mut args, "--install")),
            "--dry-run" => dry_run = true,
            "--inventory" => pretend = Some(inventory(&required(&mut args, "--inventory"))),
            "--fail-at" => fail_at = Some(required(&mut args, "--fail-at")),
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
    if fail_at.is_some() && !dry_run {
        // A real job fails when something goes wrong, not when told to.
        eprintln!("installerd: --fail-at is for --dry-run\n{USAGE}");
        std::process::exit(2);
    }
    if let Some(disk) = install {
        if dry_run {
            eprintln!("installerd: --install is not for --dry-run\n{USAGE}");
            std::process::exit(2);
        }
        std::process::exit(install_now(&real, &disk));
    }
    let executor: Arc<dyn Executor> = if dry_run {
        Arc::new(DryRun {
            step_ms: 400,
            inventory: pretend,
            fail_at,
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

/// Progress written to standard output, a line for each phase as it moves
/// and each line of the log.
struct Printed(std::sync::Mutex<(String, u8)>);

impl installerd::executor::Progress for Printed {
    fn phase(&self, name: &str, percent: u8) {
        let mut last = self.0.lock().unwrap();
        if last.0 != name || percent / 10 != last.1 / 10 || percent == 100 {
            println!("[{percent:3}%] {name}");
            *last = (name.to_owned(), percent);
        }
    }

    fn log(&self, line: String) {
        println!("       {line}");
    }
}

/// Install onto `disk` with `real`, as the install surface's job does,
/// reporting on standard output, and give the exit status.
fn install_now(real: &Real, disk: &str) -> i32 {
    let progress = Printed(std::sync::Mutex::new((String::new(), 0)));
    match real.run(JobKind::Install, disk, &progress) {
        Ok(_) => {
            println!("installed onto {disk}");
            0
        }
        Err(error) => {
            eprintln!("installerd: {error}");
            1
        }
    }
}
