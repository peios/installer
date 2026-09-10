//! A terminal surface for any MSIP conversation.
//!
//! It knows element *types* and nothing else: no page, no ref and no
//! conversation kind appears anywhere in here. That is what lets one
//! renderer serve the installer, first-boot setup and whatever comes
//! next -- each of which ships its own thin binary with its own
//! defaults, so that removing one leaves the others alone.
//!
//! What a page *is for* reaches the renderer only as hints the daemon
//! attaches (§3.7 `class`, §a2 `primary`/`destructive`), and every
//! hint is optional: a page carrying none is drawn as a form.

mod app;
mod render;
#[cfg(test)]
mod render_tests;
mod terminal;
mod theme;

use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event as TermEvent, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use msip::element::types;
use msip::frame::{MsgType, read_msg, write_msg};
use msip::msg::{Hello, Start};

use app::{After, App};
use theme::Theme;

/// What a surface binary supplies; everything else is the same.
pub struct Config {
    /// The daemon's socket.
    pub socket: String,
    /// The conversation kind to open.
    pub kind: String,
    /// How this surface names itself in the daemon's logs.
    pub surface: String,
    /// Terminal size, when the person knows better than the terminal.
    pub size: Option<(u16, u16)>,
    /// Use the plain glyph set whatever the console looks like -- for
    /// a real serial terminal, which cannot be told from an emulator.
    pub plain: bool,
    /// What the header calls this program.
    pub title: &'static str,
    /// Name used in error messages, and the service to suggest when
    /// nothing is listening.
    pub daemon_hint: &'static str,
    /// What leaving does, said in the footer where no element offers its
    /// own help, and in the box Esc opens.
    ///
    /// A surface's own words because leaving means different things:
    /// closing an installer's window leaves an installation running in
    /// the daemon, and closing first-boot setup leaves a machine that
    /// has not been set up. Telling a person the wrong one is worse than
    /// telling them nothing.
    pub leave_hint: &'static str,
}

pub fn run(cfg: Config) {
    let Config {
        socket,
        kind,
        surface,
        size,
        plain,
        title,
        daemon_hint,
        leave_hint,
    } = cfg;
    let mut stream = match UnixStream::connect(&socket) {
        Ok(s) => s,
        Err(e) => {
            // A missing socket is the ordinary case -- no installer is
            // running -- and deserves a sentence, not a panic and a
            // suggestion to set RUST_BACKTRACE.
            eprintln!("{surface}: cannot reach {daemon_hint} at {socket}: {e}");
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("  is {daemon_hint} running? (its service has that name)");
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!("  the socket is there but this principal may not open it");
            }
            std::process::exit(1);
        }
    };
    write_msg(
        &mut stream,
        MsgType::Hello,
        &Hello {
            surface: Some(surface.clone()),
            element_types: types::ALL.iter().map(|s| s.to_string()).collect(),
        },
    )
    .unwrap();

    let (tx, rx) = mpsc::channel();
    let mut reader = stream.try_clone().unwrap();
    thread::spawn(move || {
        loop {
            match read_msg(&mut reader) {
                Ok(m) => {
                    if tx.send(m).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    });

    let vt = terminal::is_linux_vt();
    let mut app = App::new(Theme::detect(plain, vt), title, daemon_hint, leave_hint);

    // Greeting, then bind.
    let (t, body) = rx.recv().expect("daemon hung up");
    app.handle(t, body);
    write_msg(&mut stream, MsgType::Start, &Start { kind }).unwrap();

    // Raw mode first, because sizing the terminal may have to ask it
    // how big it is and read the answer off stdin -- which in cooked
    // mode is echoed onto the screen as litter and withheld from us
    // until a newline that never comes.
    enable_raw_mode().unwrap();
    terminal::ensure_terminal_size(size, &surface);
    std::io::stdout().execute(EnterAlternateScreen).unwrap();
    terminal::apply_vt_palette(&theme::VT_PALETTE);
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        let _ = disable_raw_mode();
        let _ = std::io::stdout().execute(LeaveAlternateScreen);
        terminal::reset_vt_palette();
        hook(p);
    }));
    let mut term = ratatui::init();
    // The alternate screen is not a fresh page on every terminal, and
    // ratatui only repaints cells it believes changed -- so without this
    // the first frame is drawn over whatever the boot left behind.
    let _ = term.clear();

    // Set whenever the page itself changes, and consumed by the draw
    // below. The kernel is quiet while a conversation is open, but
    // peinit and the services write to this console too, and a
    // renderer that diffs against its own idea of the screen cannot see
    // what they did. A full repaint at every page boundary bounds how
    // long a scribble can survive to the page it landed on, without
    // pushing a whole screen down a serial line on a timer.
    let mut repaint = false;

    'outer: loop {
        loop {
            match rx.try_recv() {
                Ok((t, body)) => repaint |= app.handle(t, body),
                Err(mpsc::TryRecvError::Empty) => break,
                // The reader thread ended, which means the socket did.
                // Said out loud: without this the loop simply finds no
                // messages, forever, and the surface sits on whatever it
                // last drew -- which for a daemon that died before its
                // first turn is a box saying "connecting", with no way to
                // tell that from a daemon merely being slow.
                Err(mpsc::TryRecvError::Disconnected) => {
                    repaint |= app.disconnected();
                    break;
                }
            }
        }

        if std::mem::take(&mut repaint) {
            let _ = term.clear();
        }
        term.draw(|f| app.render(f)).unwrap();

        if event::poll(Duration::from_millis(80)).unwrap() {
            if let TermEvent::Key(key) = event::read().unwrap() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.key(key, &mut stream) {
                    After::Continue => {}
                    After::Repaint => repaint = true,
                    After::Quit => break 'outer,
                }
            }
        } else {
            app.tick = app.tick.wrapping_add(1);
        }
    }

    ratatui::restore();
    disable_raw_mode().ok();
    std::io::stdout().execute(LeaveAlternateScreen).ok();
    terminal::reset_vt_palette();
    if let Some(fin) = app.finished {
        println!("{}", fin.message);
    }
}
