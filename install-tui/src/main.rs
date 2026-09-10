//! The installer's terminal surface: `msip-tui` pointed at installerd.
//!
//! A binary of its own rather than a mode of a shared one, because an
//! installed system may reasonably have the installer removed from it
//! while first-boot setup stays — and one binary serving both would
//! make that impossible to express in packaging.

const USAGE: &str = "Usage: install-tui [--socket PATH] [--kind NAME] [--plain] [--size COLSxROWS]\n\
\n\
Terminal surface for the Peios installer.\n\
\n\
Options:\n\
  --socket PATH     MSIP socket (default: /run/installerd.sock)\n\
  --kind NAME       MSIP conversation kind (default: install)\n\
  --plain           use the conservative serial-console presentation\n\
  --size COLSxROWS  override terminal geometry\n\
  -h, --help        show this help\n\
  -V, --version     show the version";

fn required(args: &mut impl Iterator<Item = String>, option: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("install-tui: {option} needs a value\n{USAGE}");
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
        socket: "/run/installerd.sock".into(),
        kind: "install".into(),
        surface: concat!("install-tui/", env!("CARGO_PKG_VERSION")).into(),
        size: None,
        plain: false,
        title: "Peios Installer",
        daemon_hint: "installerd",
        // Closing this window does not stop an installation: installerd
        // owns it, and another surface can attach and watch it finish.
        leave_hint: "leave (the installation continues)",
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
                    eprintln!("install-tui: --size wants non-zero COLSxROWS\n{USAGE}");
                    std::process::exit(2);
                });
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            "-V" | "--version" => {
                println!("install-tui {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("install-tui: unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    msip_tui::run(cfg);
}
