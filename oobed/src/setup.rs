//! What first-boot setup actually does to the machine, behind a trait
//! so the flow can be driven without one.

use std::io::Write;
use std::process::{Command, Stdio};

use msip_serve::Progress;

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
    /// One line for the network page: what the machine can currently do.
    fn network_status(&self) -> String;
    /// A name to offer, which the person may keep with one keypress.
    fn suggested_hostname(&self) -> String;
    /// Whether a principal of this name already exists.
    fn account_exists(&self, name: &str) -> bool;
    fn create_account(&self, name: &str, password: &str, p: &dyn Progress) -> Result<(), String>;
    fn set_hostname(&self, name: &str, p: &dyn Progress) -> Result<(), String>;
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

pub struct Real;

impl Real {
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
    fn network_status(&self) -> String {
        match Command::new("net").arg("status").output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if text.is_empty() {
                    "No network information available.".into()
                } else {
                    text
                }
            }
            _ => "Could not ask netd about the network.".into(),
        }
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

    fn retire(&self) {
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
pub struct DryRun;

impl Setup for DryRun {
    fn network_status(&self) -> String {
        "Dry run: no network was consulted.".into()
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
    fn retire(&self) {
        msip_serve::note(
            crate::TAG,
            &format!("dry run: would remove {}", RETIRE_KEYS.join(" and ")),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::hostname_literal;

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
