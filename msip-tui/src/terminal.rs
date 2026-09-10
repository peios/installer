//! The terminal itself: its size, what kind of console it is, and the
//! palette a Linux VT is given for the duration.

use std::time::{Duration, Instant};

/// Give the terminal a size when it does not report one.
///
/// A serial console has no way to tell the kernel its geometry --
/// `stty size` on one answers `0 0` -- and that is the console the
/// installer actually runs on. Drawing into a zero-sized viewport
/// produces a blank screen and no error, which looks like a hung
/// program rather than a missing ioctl.
///
/// So: honour `--size` if given, then ask the terminal itself, then
/// `COLUMNS`/`LINES`, and otherwise assume the VT100 default of 80x24.
/// The size is written back with `TIOCSWINSZ` -- the same thing
/// `stty rows/cols` does -- so that crossterm, ratatui and anything
/// else in the session agree on one answer rather than each guessing
/// separately.
///
/// Asking comes before the environment because the terminal's answer is
/// about the terminal, whereas `COLUMNS` is ambient: as likely to have
/// been inherited from somewhere else as to have been meant. `--size`
/// still beats both, being the one value a person stated on purpose.
///
/// A terminal that does report a size is left alone.
pub(crate) fn ensure_terminal_size(requested: Option<(u16, u16)>, surface: &str) {
    const TIOCSWINSZ: libc::Ioctl = 0x5414;

    let reported = crossterm::terminal::size().unwrap_or((0, 0));
    if requested.is_none() && reported.0 > 0 && reported.1 > 0 {
        return;
    }
    let env = |k: &str| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|v| *v > 0)
    };
    let (cols, rows) = requested
        .or_else(ask_terminal_size)
        .unwrap_or_else(|| (env("COLUMNS").unwrap_or(80), env("LINES").unwrap_or(24)));

    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, TIOCSWINSZ, &ws) };
    if rc < 0 {
        eprintln!(
            "{surface}: this terminal reports no size and would not accept {cols}x{rows}; \
             the display may be blank"
        );
    }
}

/// How long to wait for a terminal to answer.
///
/// Generous on purpose: the reply is ten bytes, under a hundredth of a
/// second even on a 9600 baud line, and this is paid once and only when
/// the terminal had no size to give in the first place.
const PROBE_WAIT: Duration = Duration::from_millis(250);

/// Ask the terminal how big it is, when it will not say on its own.
///
/// Park the cursor past any plausible bottom-right corner -- `CUP`
/// clamps to the edge rather than scrolling -- and ask where it landed
/// with `DSR 6`. The answer arrives on stdin as `ESC [ rows ; cols R`,
/// written by the terminal emulator itself, which over a serial line is
/// the only thing in the chain that knows: a raw byte stream carries no
/// geometry, so QEMU cannot hand the host window's size to the guest
/// and the kernel leaves the tty at 0x0 for the life of the boot. This
/// is what `resize(1)` does, for the same reason.
///
/// Raw mode must already be on -- which is why the caller enables it
/// before sizing the terminal. In cooked mode the reply is echoed onto
/// the screen as litter, and line buffering holds it back from us until
/// a newline that never comes.
///
/// The cursor is put back with `DECSC`/`DECRC`, so a probe that finds
/// nothing has cost the display nothing either. The scroll region is
/// left alone: `resize(1)` resets it, in case one is set and clamps the
/// cursor short of the real bottom row, but changing a terminal's state
/// in order to measure it is a poor trade on a console peinit has just
/// handed over, where none is set. The failure it guards against would
/// under-report the height, which is visible and diagnosable rather
/// than silent.
///
/// `None` when nothing answers: a real serial port with no terminal on
/// the far end, or output redirected into a file.
fn ask_terminal_size() -> Option<(u16, u16)> {
    if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
        return None;
    }
    probe_size(libc::STDIN_FILENO, libc::STDOUT_FILENO, PROBE_WAIT)
}

/// [`ask_terminal_size`] with its descriptors named, so that a test can
/// be the terminal.
fn probe_size(from: libc::c_int, to: libc::c_int, within: Duration) -> Option<(u16, u16)> {
    write_fd(to, b"\x1b7\x1b[999;999H\x1b[6n")?;
    let reply = read_until(from, Instant::now() + within);
    let _ = write_fd(to, b"\x1b8");
    parse_cursor_report(&reply)
}

fn interrupted() -> bool {
    std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
}

fn write_fd(fd: libc::c_int, mut bytes: &[u8]) -> Option<()> {
    while !bytes.is_empty() {
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if n > 0 {
            bytes = &bytes[n as usize..];
        } else if !interrupted() {
            return None;
        }
    }
    Some(())
}

/// Read until the report is terminated, the deadline passes, or the far
/// end goes away.
///
/// Everything read is kept rather than only the escape sequence: a
/// keystroke made while we were asking arrives in the same buffer, and
/// [`parse_cursor_report`] scans past it. The cap stops a far end that
/// chatters something else entirely from holding the whole wait.
fn read_until(fd: libc::c_int, deadline: Instant) -> Vec<u8> {
    const CAP: usize = 256;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 64];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || buf.len() >= CAP {
            return buf;
        }
        let mut fds = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = left.as_millis().min(i32::MAX as u128) as libc::c_int;
        match unsafe { libc::poll(&mut fds, 1, ms) } {
            n if n > 0 => {}
            0 => return buf,
            _ if interrupted() => continue,
            _ => return buf,
        }
        let n = unsafe { libc::read(fd, chunk.as_mut_ptr().cast(), chunk.len()) };
        if n > 0 {
            buf.extend_from_slice(&chunk[..n as usize]);
            if buf.contains(&b'R') {
                return buf;
            }
        } else if !interrupted() {
            return buf;
        }
    }
}

/// `ESC [ rows ; cols R` -> `(cols, rows)`.
///
/// Columns first on the way out, because that is the order everything
/// else here speaks in -- `--size` is `COLSxROWS`, and
/// `crossterm::terminal::size` answers the same way -- while the report
/// itself is rows first. That swap is the whole reason this is a named
/// function with tests rather than four lines inside the probe.
///
/// Scanned for anywhere in the buffer rather than matched from the
/// start, because a keystroke made during the probe lands in front of
/// the reply; and every candidate is tried, because that keystroke may
/// have been an arrow key and so begun with an escape of its own.
fn parse_cursor_report(buf: &[u8]) -> Option<(u16, u16)> {
    for (at, pair) in buf.windows(2).enumerate() {
        if pair != b"\x1b[" {
            continue;
        }
        let body = &buf[at + 2..];
        let Some(end) = body.iter().position(|b| *b == b'R') else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&body[..end]) else {
            continue;
        };
        let Some((rows, cols)) = text.split_once(';') else {
            continue;
        };
        if let (Ok(rows), Ok(cols)) = (rows.parse::<u16>(), cols.parse::<u16>())
            && rows > 0
            && cols > 0
        {
            return Some((cols, rows));
        }
    }
    None
}

/// Whether stdin is a Linux virtual console -- the VGA or framebuffer
/// console a real machine shows -- as opposed to a serial line or a
/// pty.
///
/// Asked with `KDGKBTYPE`, which only a VT answers; everything else
/// fails it with `ENOTTY`. `TERM` cannot be used for this: a service
/// definition sets it to `vt220` whatever the console turns out to be,
/// and that is the right thing for it to do, since it cannot know
/// either.
///
/// The distinction matters twice. The VT's font is the kernel's, a
/// CP437-shaped set that has box drawing and blocks but not rounded
/// corners, ticks or braille, so it gets the plain glyph set. And its
/// sixteen colours are the VGA defaults -- the blue that is unreadable
/// on black -- unless something programs them, which is what
/// [`apply_vt_palette`] does.
pub(crate) fn is_linux_vt() -> bool {
    const KDGKBTYPE: libc::Ioctl = 0x4B33;
    let mut kind: libc::c_char = 0;
    unsafe { libc::ioctl(libc::STDIN_FILENO, KDGKBTYPE, &mut kind) == 0 }
}

/// Program the VT's sixteen colours, for a console that is one.
///
/// `ESC ] P nrrggbb` is the Linux console's own sequence, and is sent
/// only when [`is_linux_vt`] says so: any other terminal would print
/// it. The point is that a form drawn on a bare machine looks like the
/// same form drawn in a terminal emulator, rather than like 1987.
///
/// Undone by [`reset_vt_palette`], which the exit path and the panic
/// hook both call. A palette left behind is cosmetic -- the login
/// prompt inherits it -- but it is somebody else's console after this.
pub(crate) fn apply_vt_palette(palette: &[&str; 16]) {
    if !is_linux_vt() {
        return;
    }
    use std::io::Write;
    let mut out = std::io::stdout();
    for (i, rgb) in palette.iter().enumerate() {
        let _ = write!(out, "\x1b]P{i:x}{rgb}");
    }
    let _ = out.flush();
}

/// Put the VT's colours back to the kernel's defaults. Harmless and
/// silent anywhere that is not a VT.
pub(crate) fn reset_vt_palette() {
    if !is_linux_vt() {
        return;
    }
    use std::io::Write;
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b]R");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::{parse_cursor_report, probe_size};
    use std::time::Duration;

    /// A pair of connected descriptors to stand in for the line between
    /// this program and a terminal. Not a pty: the probe never asks
    /// what kind of device it is talking to, only whether an answer
    /// comes back, which is exactly what makes it work over a serial
    /// console in the first place.
    fn line() -> (libc::c_int, libc::c_int) {
        let mut fds = [0 as libc::c_int; 2];
        let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
        assert_eq!(rc, 0, "socketpair");
        (fds[0], fds[1])
    }

    fn close(fds: (libc::c_int, libc::c_int)) {
        unsafe {
            libc::close(fds.0);
            libc::close(fds.1);
        }
    }

    fn say(fd: libc::c_int, bytes: &[u8]) {
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        assert_eq!(n, bytes.len() as isize);
    }

    /// The report is rows first and every caller here is columns first.
    /// Getting this backwards produces a plausible-looking display of
    /// the wrong shape, which is the failure this whole function exists
    /// to stop.
    #[test]
    fn a_cursor_report_is_read_rows_first_and_answered_columns_first() {
        assert_eq!(parse_cursor_report(b"\x1b[24;80R"), Some((80, 24)));
        assert_eq!(parse_cursor_report(b"\x1b[43;150R"), Some((150, 43)));
    }

    /// Someone pressing a key while the probe is in flight puts their
    /// bytes in front of the reply -- and an arrow key puts an escape
    /// sequence there, so finding the first escape is not enough.
    #[test]
    fn a_keystroke_during_the_probe_does_not_hide_the_reply() {
        assert_eq!(parse_cursor_report(b"q\x1b[24;80R"), Some((80, 24)));
        assert_eq!(parse_cursor_report(b"\x1b[A\x1b[24;80R"), Some((80, 24)));
    }

    /// Anything short of a whole report is nothing, because the caller's
    /// fallback -- 80x24 -- is better than half a measurement.
    #[test]
    fn nothing_that_is_not_a_report_is_read_as_one() {
        assert_eq!(parse_cursor_report(b""), None);
        assert_eq!(parse_cursor_report(b"\x1b[24;80"), None);
        assert_eq!(parse_cursor_report(b"\x1b[24R"), None);
        assert_eq!(parse_cursor_report(b"\x1b[0;0R"), None);
        assert_eq!(parse_cursor_report(b"\x1b[x;yR"), None);
    }

    #[test]
    fn a_terminal_that_answers_is_asked_and_believed() {
        let (ours, theirs) = line();
        let answering = std::thread::spawn(move || {
            let mut heard = [0u8; 64];
            let n = unsafe { libc::read(theirs, heard.as_mut_ptr().cast(), heard.len()) };
            assert!(n > 0, "the probe asked nothing");
            let asked = &heard[..n as usize];
            assert!(
                asked.windows(4).any(|w| w == b"\x1b[6n"),
                "no cursor position request in {asked:?}"
            );
            say(theirs, b"\x1b[43;150R");
        });
        let size = probe_size(ours, ours, Duration::from_secs(2));
        answering.join().unwrap();
        close((ours, theirs));
        assert_eq!(size, Some((150, 43)));
    }

    /// A serial line delivers when it delivers. The reply may arrive a
    /// byte at a time, and a probe that read once would see `ESC` and
    /// conclude the terminal said nothing.
    #[test]
    fn a_reply_split_across_reads_is_still_read() {
        let (ours, theirs) = line();
        let answering = std::thread::spawn(move || {
            let mut heard = [0u8; 64];
            unsafe { libc::read(theirs, heard.as_mut_ptr().cast(), heard.len()) };
            for byte in b"\x1b[24;80R" {
                say(theirs, &[*byte]);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let size = probe_size(ours, ours, Duration::from_secs(2));
        answering.join().unwrap();
        close((ours, theirs));
        assert_eq!(size, Some((80, 24)));
    }

    /// Nothing on the far end -- a real serial port, or output going to
    /// a file. The probe gives up when the deadline passes rather than
    /// waiting on a terminal that is not there, and the caller falls
    /// through to 80x24 as it did before there was a probe at all.
    #[test]
    fn a_silent_far_end_gives_up() {
        let fds = line();
        assert_eq!(probe_size(fds.0, fds.0, Duration::from_millis(50)), None);
        close(fds);
    }
}
