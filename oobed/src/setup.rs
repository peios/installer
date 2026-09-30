//! What first-boot setup actually does to the machine, behind a trait
//! so the flow can be driven without one.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use msip_serve::Progress;

use crate::network::Manual;

/// The account first-boot setup in a browser runs as.
///
/// GXWI runs an overlay only as an account that needs no credential, and a
/// machine that has not been set up has no account at all, so oobed makes
/// this one for the purpose and removes it once setup is done. It has no
/// groups: all it is for is reaching oobed's socket, which admits it by SID.
/// It is not the machine's `peios`: that is what the person is offered to
/// call their own account, and this must never be the same account as that.
pub const VISITOR: &str = "peios-oobe-setup";

/// Where GXWI reads which overlay to run and as whom. GXWI watches the key,
/// so setting and clearing it apply without restarting anything.
const GXWI_KEY: &str = "Machine\\Software\\GXWI";

/// The group that makes the first account able to administer the
/// machine it was just created on. Named rather than written as
/// S-1-5-32-544 because lpsd resolves group names.
const ADMIN_GROUP: &str = "Administrators";

/// Where netd reads the machine's name. netd watches this subtree, so
/// writing it applies without a reboot, and an explicit name wins over
/// anything DHCP offers.
const NETWORK_KEY: &str = "Machine\\System\\Network";

/// The service definitions that make first-boot setup happen, removed
/// once it has. Both of them: the surface holds the console and the
/// engine answers it, and a machine that has been set up wants neither.
///
/// Retiring by deleting the definitions rather than by a marker file or
/// a `Conditions` check, because the definitions are the only thing that
/// starts these services. A marker would leave two definitions on the
/// system for the rest of its life, each starting a process every boot
/// to discover it has nothing to do.
const RETIRE_KEYS: &[&str] = &[
    "Machine\\System\\Services\\oobe-tui",
    "Machine\\System\\Services\\oobed",
];

pub trait Setup: Send + Sync + 'static {
    /// What `net status` prints, for the network page, or nothing where
    /// netd could not be asked.
    fn net_status(&self) -> Option<String>;
    /// A name to offer, which the person may keep with one keypress.
    fn suggested_hostname(&self) -> String;
    /// Whether a principal of this name already exists.
    fn account_exists(&self, name: &str) -> bool;
    fn create_account(&self, name: &str, password: &str, p: &dyn Progress) -> Result<(), String>;
    fn set_hostname(&self, name: &str, p: &dyn Progress) -> Result<(), String>;
    /// Give an interface the address chosen for it by hand
    /// ([`Manual::registry`]).
    fn set_address(&self, manual: &Manual, p: &dyn Progress) -> Result<(), String>;
    /// Setup is done: make sure it does not happen again.
    ///
    /// Returns nothing and reports nothing, because there is nobody left
    /// to report to — the conversation has ended and the console belongs
    /// to whatever comes next. Failures go to stderr, which reaches the
    /// logs.
    ///
    /// The real implementation does not return: retiring means ending
    /// this process, and that is a fact about the machine rather than
    /// about the flow, so it lives behind this trait with everything
    /// else that touches it. A test double simply records the call.
    fn retire(&self);
}

/// The `reg set` data token for a hostname.
///
/// One token, not two: `reg set` takes the value's data as a single
/// argument whose type is a `type:` prefix or, without one, inferred
/// from its shape. Passing the type as a separate argument — which this
/// did, and which cost a first boot — is simply a fourth positional
/// `reg` has no use for.
///
/// The prefix is not decoration either. Inference is deliberately broad:
/// an all-digit token becomes a `REG_DWORD`, so a machine somebody names
/// `12345` would be stored as a number, and netd reads `Hostname` as a
/// string and would silently ignore it.
fn hostname_literal(name: &str) -> String {
    format!("sz:{name}")
}

/// The SID `lps show` gives a principal, from what it printed.
fn sid_from_show(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("sid "))
        .map(str::trim)
        .filter(|sid| sid.starts_with("S-"))
        .map(str::to_string)
}

/// The home directory `lps show` gives a principal, from what it printed.
fn home_from_show(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("home "))
        .map(str::trim)
        .filter(|home| home.starts_with('/'))
        .map(str::to_string)
}

/// What oobed's socket admits: SYSTEM and Administrators, as for every
/// setup socket, and the account first-boot setup in a browser runs as,
/// where there is one.
pub fn socket_sddl(visitor: Option<&str>) -> String {
    match visitor {
        Some(sid) => format!("{}(A;;GA;;;{sid})", msip_serve::ADMIN_SOCKET_SDDL),
        None => msip_serve::ADMIN_SOCKET_SDDL.to_string(),
    }
}

pub struct Real {
    /// The program that draws setup in a browser, if one is to.
    browser: Option<PathBuf>,
    /// The SID of [`VISITOR`], once browsers have been let in.
    visitor: OnceLock<String>,
}

impl Real {
    /// `browser` is the program GXWI is to run as its overlay for setup,
    /// or nothing for setup on the console alone.
    pub fn new(browser: Option<PathBuf>) -> Real {
        Real {
            browser,
            visitor: OnceLock::new(),
        }
    }

    /// Lets setup be done in a browser: makes [`VISITOR`] if it is not
    /// there already, and returns its SID for the socket to admit. Nothing
    /// where no browser is to draw setup, or the program is not installed,
    /// or the account cannot be had; setup is then on the console alone,
    /// as it always was.
    ///
    /// Done before the socket is secured, and [`Real::show_to_browsers`]
    /// after: the overlay must not start before it can connect.
    pub fn welcome_browsers(&self) -> Option<String> {
        let program = self.browser.as_deref()?;
        if !program.is_file() {
            msip_serve::note(
                crate::TAG,
                &format!(
                    "{} is not installed; setup is on the console only",
                    program.display()
                ),
            );
            return None;
        }
        // Kept from an earlier boot whose setup did not finish.
        let sid = match Self::show(VISITOR) {
            Some(shown) => sid_from_show(&shown),
            None => Self::make_visitor()
                .and_then(|()| Self::show(VISITOR))
                .as_deref()
                .and_then(sid_from_show),
        };
        let Some(sid) = sid else {
            msip_serve::note(
                crate::TAG,
                &format!("could not make {VISITOR}; setup is on the console only"),
            );
            return None;
        };
        Some(self.visitor.get_or_init(|| sid).clone())
    }

    /// Makes GXWI send everyone who opens the machine's address to setup,
    /// once [`Real::welcome_browsers`] has let them in.
    pub fn show_to_browsers(&self) {
        let (Some(program), Some(_)) = (self.browser.as_deref(), self.visitor.get()) else {
            return;
        };
        let session = format!("sz:{}", program.display());
        let username = format!("sz:{VISITOR}");
        for (name, data) in [
            ("OverlayUsername", username.as_str()),
            ("OverlaySession", session.as_str()),
        ] {
            // -p: an installed machine that has never had a GXWI setting
            // has no key to set it in.
            if let Err(why) = Self::quietly("reg", &["set", "-p", GXWI_KEY, name, data]) {
                msip_serve::note(
                    crate::TAG,
                    &format!("could not set {GXWI_KEY}\\{name}: {why}"),
                );
            }
        }
    }

    /// What `lps show` prints of `name`, or nothing where there is no such
    /// principal or lpsd cannot say.
    fn show(name: &str) -> Option<String> {
        let out = Command::new("lps").args(["show", name]).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn make_visitor() -> Option<()> {
        let made = Self::quietly(
            "lps",
            &[
                "add",
                VISITOR,
                "--no-password",
                "--no-prompt",
                "--display-name",
                "First-boot setup",
            ],
        );
        made.map_err(|why| msip_serve::note(crate::TAG, &format!("creating {VISITOR}: {why}")))
            .ok()
    }

    /// Runs `program` with nothing to read and nobody to report to but
    /// the log: the first line it complained with, if it failed.
    fn quietly(program: &str, args: &[&str]) -> Result<(), String> {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("could not run {program}: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let said = String::from_utf8_lossy(&out.stderr);
        Err(said
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("failed")
            .to_string())
    }

    /// Undoes [`Real::welcome_browsers`] and [`Real::show_to_browsers`],
    /// in the order that leaves nothing broken if it stops part way: the
    /// overlay first, so that GXWI goes back to its sign-in page rather
    /// than trying to run setup as an account that is going; then the
    /// account, and the home its logon was given.
    fn send_browsers_away(&self) {
        if self.visitor.get().is_none() {
            return;
        }
        for name in ["OverlaySession", "OverlayUsername"] {
            if let Err(why) = Self::quietly("reg", &["del", "--yes", GXWI_KEY, name]) {
                msip_serve::note(crate::TAG, &format!("removing {GXWI_KEY}\\{name}: {why}"));
            }
        }
        let home = Self::show(VISITOR).as_deref().and_then(home_from_show);
        if let Err(why) = Self::quietly("lps", &["remove", VISITOR]) {
            msip_serve::note(crate::TAG, &format!("removing {VISITOR}: {why}"));
            return;
        }
        // Only the account's own, and only once the account is gone: a
        // home that is somewhere else is somebody's.
        if let Some(home) = home.filter(|home| {
            Path::new(home)
                .file_name()
                .is_some_and(|name| name == VISITOR)
        }) && let Err(e) = std::fs::remove_dir_all(&home)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            msip_serve::note(crate::TAG, &format!("removing {home}: {e}"));
        }
    }

    fn run(p: &dyn Progress, program: &str, args: &[&str]) -> Result<(), String> {
        p.log(format!("$ {program} {}", args.join(" ")));
        let out = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("could not run {program}: {e}"))?;
        for stream in [&out.stdout, &out.stderr] {
            for line in String::from_utf8_lossy(stream).lines() {
                if !line.trim().is_empty() {
                    p.log(format!("  {line}"));
                }
            }
        }
        if out.status.success() {
            return Ok(());
        }
        // The reason, not just the program. This is the text a person
        // reads at the end of a setup that went wrong, and "reg failed"
        // sends them looking through a log for the sentence that was
        // right here.
        let why = String::from_utf8_lossy(&out.stderr)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(ToOwned::to_owned);
        Err(match why {
            Some(why) => format!("{program}: {why}"),
            None => format!("{program} failed ({})", out.status),
        })
    }
}

impl Setup for Real {
    fn net_status(&self) -> Option<String> {
        let out = Command::new("net").arg("status").output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn suggested_hostname(&self) -> String {
        // Four hex characters, as DESKTOP-XXXXXX does: enough that two
        // machines on a desk do not collide, short enough to retype.
        let mut raw = [0u8; 2];
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            let _ = f.read_exact(&mut raw);
        }
        format!("peios-{:02x}{:02x}", raw[0], raw[1])
    }

    fn account_exists(&self, name: &str) -> bool {
        // `lps show` answers for one principal; a failure is "no such
        // principal" or a store that cannot be reached, and both mean
        // "do not skip creation" -- creation will report the real error.
        Command::new("lps")
            .args(["show", name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn create_account(&self, name: &str, password: &str, p: &dyn Progress) -> Result<(), String> {
        // lps takes the password on stdin when stdin is not a terminal;
        // there is deliberately no --password flag, so a credential
        // never reaches a process listing or a shell history.
        p.log(format!("creating {name} as an administrator"));
        let mut child = Command::new("lps")
            .args(["add", name, "--group", ADMIN_GROUP])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not run lps: {e}"))?;
        child
            .stdin
            .as_mut()
            .ok_or("lps took no stdin")?
            .write_all(format!("{password}\n").as_bytes())
            .map_err(|e| format!("could not hand lps the password: {e}"))?;
        let out = child
            .wait_with_output()
            .map_err(|e| format!("lps did not finish: {e}"))?;
        // Only stderr is echoed, and the password was never an argument,
        // so nothing here can carry it.
        for line in String::from_utf8_lossy(&out.stderr).lines() {
            if !line.trim().is_empty() {
                p.log(format!("  {line}"));
            }
        }
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("could not create {name}"))
        }
    }

    fn set_hostname(&self, name: &str, p: &dyn Progress) -> Result<(), String> {
        let data = hostname_literal(name);
        Self::run(p, "reg", &["set", NETWORK_KEY, "Hostname", &data])
    }

    fn set_address(&self, manual: &Manual, p: &dyn Progress) -> Result<(), String> {
        p.log(format!("giving {} {}", manual.interface, manual.address));
        for (key, name, data) in manual.registry() {
            // -p: the profile and the rule are new keys.
            Self::run(p, "reg", &["set", "-p", &key, name, &data])?;
        }
        Ok(())
    }

    fn retire(&self) {
        self.send_browsers_away();
        for key in RETIRE_KEYS {
            // -r because a service key may carry subkeys, --yes because
            // there is no terminal here to answer the prompt on — and
            // the one there might be is the console this flow was just
            // drawn on, where a stray [y/N] would be read out of
            // whatever the login prompt is about to be given.
            match Command::new("reg")
                .args(["del", "-r", key, "--yes"])
                .output()
            {
                Ok(out) if out.status.success() => {}
                // Not fatal, and deliberately so: a machine that was set
                // up correctly and will ask again next boot is far better
                // than one abandoned mid-setup.
                Ok(out) => msip_serve::note(
                    crate::TAG,
                    &format!(
                        "removing {key}: {}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    ),
                ),
                Err(e) => msip_serve::note(
                    crate::TAG,
                    &format!("removing {key}: could not run reg: {e}"),
                ),
            }
        }
        // Leaving rather than idling on, for two reasons. The socket
        // stays bound otherwise, and a second surface attaching to it
        // would run the account flow again on a machine that is now
        // somebody's; and a running service whose definition has just
        // been deleted is a state `svctl list` has no good way to
        // explain.
        //
        // Safe here: the END was broadcast before this was called, and a
        // stream socket delivers what is in its buffer to a reader after
        // the writer closes — so leaving cannot cost the surface its
        // completion message.
        std::process::exit(0);
    }
}

/// Touches nothing; reports what it would have done.
#[derive(Default)]
pub struct DryRun {
    /// A file holding what `net status` is to be taken to print, read
    /// afresh each time it is asked, so that changing it and checking again
    /// is a cable plugged in. Without one, netd cannot be asked.
    pub net_status: Option<PathBuf>,
}

impl Setup for DryRun {
    fn net_status(&self) -> Option<String> {
        std::fs::read_to_string(self.net_status.as_deref()?).ok()
    }
    fn suggested_hostname(&self) -> String {
        "peios-0000".into()
    }
    fn account_exists(&self, _name: &str) -> bool {
        false
    }
    fn create_account(&self, name: &str, _password: &str, p: &dyn Progress) -> Result<(), String> {
        p.log(format!("dry run: would create {name}"));
        Ok(())
    }
    fn set_hostname(&self, name: &str, p: &dyn Progress) -> Result<(), String> {
        p.log(format!("dry run: would set the hostname to {name}"));
        Ok(())
    }
    fn set_address(&self, manual: &Manual, p: &dyn Progress) -> Result<(), String> {
        for (key, name, data) in manual.registry() {
            p.log(format!("dry run: would set {key} {name} {data}"));
        }
        Ok(())
    }
    fn retire(&self) {
        msip_serve::note(
            crate::TAG,
            &format!("dry run: would remove {}", RETIRE_KEYS.join(" and ")),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{home_from_show, hostname_literal, sid_from_show, socket_sddl};

    /// What `lps show` prints (`authd/lps/src/format.rs`), labels padded.
    const SHOWN: &str = "name           peios-oobe-setup\n\
                         display name   First-boot setup\n\
                         rid            1004\n\
                         sid            S-1-5-21-1-2-3-1004\n\
                         uid            1004\n\
                         state          enabled\n\
                         primary group  Users [S-1-5-32-545]\n\
                         home           /home/peios-oobe-setup\n\
                         shell          /bin/sh\n\
                         groups         none\n\
                         claims         none\n";

    #[test]
    fn the_visitor_is_known_by_what_lps_shows_of_it() {
        assert_eq!(sid_from_show(SHOWN).as_deref(), Some("S-1-5-21-1-2-3-1004"));
        assert_eq!(
            home_from_show(SHOWN).as_deref(),
            Some("/home/peios-oobe-setup")
        );
        // Nothing that is not what was asked for.
        assert_eq!(
            sid_from_show("sid            <unreadable SID, 3 bytes>\n"),
            None
        );
        assert_eq!(sid_from_show("primary group  Users [S-1-5-32-545]\n"), None);
        assert_eq!(home_from_show("home           \n"), None);
    }

    #[test]
    fn the_socket_admits_the_visitor_beside_the_administrators() {
        assert_eq!(socket_sddl(None), msip_serve::ADMIN_SOCKET_SDDL);
        assert_eq!(
            socket_sddl(Some("S-1-5-21-1-2-3-1004")),
            "O:SYG:SYD:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;S-1-5-21-1-2-3-1004)"
        );
    }

    /// `reg set` infers a type when the token carries no prefix, and an
    /// all-digit token infers as a number. netd reads `Hostname` as a
    /// string, so a machine named `12345` would have its name silently
    /// ignored. The prefix is what stops that.
    #[test]
    fn a_hostname_is_always_written_as_a_string() {
        assert_eq!(hostname_literal("workshop"), "sz:workshop");
        assert_eq!(hostname_literal("12345"), "sz:12345");
        assert_eq!(hostname_literal("0x40"), "sz:0x40");
    }
}
