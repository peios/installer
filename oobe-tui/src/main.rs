//! The first-boot surface: `msip-tui` pointed at oobed.
//!
//! A binary of its own rather than a mode of a shared one: first-boot
//! setup outlives the installer on any machine that had one, and runs
//! on machines that never did — a shipped image, a cloned VM, a
//! factory preinstall — so the two must be separately installable.

const USAGE: &str = "Usage: oobe-tui [--socket PATH] [--kind NAME] [--plain] [--size COLSxROWS]\n\
\n\
Terminal surface for Peios first-boot setup.\n\
\n\
Options:\n\
  --socket PATH     MSIP socket (default: /run/oobed.sock)\n\
  --kind NAME       MSIP conversation kind (default: oobe)\n\
  --plain           use the conservative serial-console presentation\n\
  --size COLSxROWS  override terminal geometry\n\
  -h, --help        show this help\n\
  -V, --version     show the version";

fn required(args: &mut impl Iterator<Item = String>, option: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("oobe-tui: {option} needs a value\n{USAGE}");
        std::process::exit(2);
    })
}

fn size(value: &str) -> Option<(u16, u16)> {
    let (columns, rows) = value.split_once(['x', 'X'])?;
    Some((
        columns.parse().ok().filter(|value| *value > 0)?,
        rows.parse().ok().filter(|value| *value > 0)?,
    ))
}

fn main() {
    let mut cfg = msip_tui::Config {
        socket: "/run/oobed.sock".into(),
        kind: "oobe".into(),
        surface: concat!("oobe-tui/", env!("CARGO_PKG_VERSION")).into(),
        size: None,
        plain: false,
        title: "Peios Setup",
        daemon_hint: "oobed",
        // The opposite of the installer's: nothing carries on without
        // this, and a machine left here has no account to log in with.
        // Setup is not retired until it finishes, so the next boot asks
        // again -- which is what makes leaving survivable rather than
        // safe.
        leave_hint: "leave setup unfinished (it runs again next boot)",
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => cfg.socket = required(&mut args, "--socket"),
            "--kind" => cfg.kind = required(&mut args, "--kind"),
            "--plain" => cfg.plain = true,
            "--size" => {
                let value = required(&mut args, "--size");
                cfg.size = size(&value).or_else(|| {
                    eprintln!("oobe-tui: --size wants non-zero COLSxROWS\n{USAGE}");
                    std::process::exit(2);
                });
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            "-V" | "--version" => {
                println!("oobe-tui {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("oobe-tui: unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    msip_tui::run(cfg);
}
