//! Quieting the kernel console for as long as a conversation owns it.
//!
//! A surface on `/dev/console` shares the terminal with the kernel, and
//! the kernel does not take turns: printk writes wherever the cursor is,
//! and a line landing on the bottom row scrolls the screen. A renderer
//! that repaints only the cells it believes changed -- which is every
//! sane one -- then has a model of the screen that no longer matches it,
//! so the damage is not repaired by the next frame. It stays until
//! something forces a full repaint.
//!
//! The remedy is to stop the writing rather than to keep repairing after
//! it, and the daemon is where that can happen: quieting the console is
//! privileged, and on these images the surface deliberately is not --
//! `install-tui` runs as the operator. So the side with the privilege
//! does it on behalf of the side with the need, for exactly as long as a
//! conversation is open.
//!
//! **For every conversation, not only the ones drawn on a console.** A
//! daemon cannot tell: MSIP's `Hello` carries a surface name, and §3.2
//! says a daemon MUST NOT vary behaviour on it, which is the protocol
//! deliberately refusing to answer this question. Being wrong costs a
//! quiet console for the length of a scripted run; not doing it costs an
//! unreadable form.

use std::path::Path;

use crate::note;

/// The knob `ignore_loglevel` on the kernel command line sets. It is
/// writable at runtime, which is the only reason any of this works:
/// these images boot with `ignore_loglevel`, and while it is set the
/// kernel prints every message to the console whatever the loglevel
/// says, so lowering the loglevel alone changes nothing.
const IGNORE: &str = "/sys/module/printk/parameters/ignore_loglevel";

/// Four numbers; the first is `console_loglevel`. A message prints when
/// its level is *below* this, so 1 leaves only `KERN_EMERG` -- a panic
/// still reaches the screen, and nothing else does.
const PRINTK: &str = "/proc/sys/kernel/printk";

/// What we lower `console_loglevel` to.
const QUIET: u8 = 1;

/// A quieted console, restored when dropped.
///
/// Held by the live conversation, so every way a conversation can end --
/// finishing, failing, a flow that would not open its first page, the
/// daemon exiting -- puts the console back without any of them having to
/// remember to. That matters more than it looks: a daemon that died
/// holding this would leave a machine that has stopped reporting its own
/// faults, which is a worse bug than the one being fixed.
pub(crate) struct Quiet {
    /// Previous contents, for each file this actually changed. `None`
    /// means it was already as wanted, or could not be written, and must
    /// be left alone on the way out.
    ignore: Option<String>,
    printk: Option<String>,
}

impl Quiet {
    pub(crate) fn hold() -> Quiet {
        Quiet {
            ignore: stop_ignoring(Path::new(IGNORE)),
            printk: lower_console_loglevel(Path::new(PRINTK)),
        }
    }
}

impl Drop for Quiet {
    fn drop(&mut self) {
        for (path, previous) in [(IGNORE, &self.ignore), (PRINTK, &self.printk)] {
            let Some(previous) = previous else { continue };
            if let Err(e) = std::fs::write(path, previous) {
                // Said out loud rather than swallowed: the machine is
                // now quieter than whoever booted it asked for, and the
                // next person to wonder why deserves the sentence.
                note(crate::TAG, &format!("restoring {path}: {e}"));
            }
        }
    }
}

/// Turn `ignore_loglevel` off, reporting what it was if we changed it.
///
/// `None` when there was nothing to change and when the change was
/// refused alike: either way there is nothing to put back. A daemon
/// that is refused simply gets the noisy console it already had.
fn stop_ignoring(path: &Path) -> Option<String> {
    let previous = std::fs::read_to_string(path).ok()?;
    if !previous.trim().eq_ignore_ascii_case("Y") {
        return None;
    }
    write(path, "N\n").then_some(previous)
}

/// Lower `console_loglevel` to 1, reporting the whole previous line.
///
/// Writing takes one number and sets `console_loglevel` alone; reading
/// gives four. The whole line is kept because that is what restores
/// cleanly, and because the other three are not ours to guess at.
fn lower_console_loglevel(path: &Path) -> Option<String> {
    let previous = std::fs::read_to_string(path).ok()?;
    let current: u8 = previous.split_whitespace().next()?.parse().ok()?;
    if current <= QUIET {
        return None;
    }
    write(path, &format!("{QUIET}\n")).then_some(previous)
}

/// Write, saying so if refused.
///
/// The refusal is worth a line because it is the difference between
/// "this did nothing because there was nothing to do" and "this did
/// nothing because the daemon is not allowed to" -- invisible from the
/// screen, where both look like a console being drawn over, and the
/// first question anyone will ask.
fn write(path: &Path, contents: &str) -> bool {
    match std::fs::write(path, contents) {
        Ok(()) => true,
        Err(e) => {
            note(
                crate::TAG,
                &format!("quieting the console: {}: {e}", path.display()),
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{lower_console_loglevel, stop_ignoring};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch(contents: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "msip-console-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, contents).expect("scratch file");
        path
    }

    #[test]
    fn a_console_that_ignores_the_loglevel_is_told_not_to() {
        let p = scratch("Y\n");
        assert_eq!(stop_ignoring(&p).as_deref(), Some("Y\n"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "N\n");
        std::fs::remove_file(p).ok();
    }

    /// Nothing to change is nothing to put back. Reporting a previous
    /// value here would have the guard write `N` on the way out to a
    /// machine that was never ignoring the loglevel in the first place.
    #[test]
    fn a_console_that_already_obeys_the_loglevel_is_left_alone() {
        let p = scratch("N\n");
        assert_eq!(stop_ignoring(&p), None);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "N\n");
        std::fs::remove_file(p).ok();
    }

    /// Four numbers in, one number out: writing sets `console_loglevel`
    /// alone, and the whole line is what goes back on restore.
    #[test]
    fn the_console_loglevel_is_lowered_and_the_whole_line_kept() {
        let p = scratch("7\t4\t1\t7\n");
        assert_eq!(lower_console_loglevel(&p).as_deref(), Some("7\t4\t1\t7\n"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "1\n");
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn a_console_already_this_quiet_is_left_alone() {
        let p = scratch("1\t4\t1\t7\n");
        assert_eq!(lower_console_loglevel(&p), None);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "1\t4\t1\t7\n");
        std::fs::remove_file(p).ok();
    }

    /// A kernel without these knobs, or a daemon refused them. Both are
    /// survivable: the form is drawn over, which is the bug this reduces
    /// rather than a reason to fail starting.
    #[test]
    fn knobs_that_cannot_be_read_are_not_an_error() {
        let missing = Path::new("/nonexistent/printk");
        assert_eq!(stop_ignoring(missing), None);
        assert_eq!(lower_console_loglevel(missing), None);
    }
}
