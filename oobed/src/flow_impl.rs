//! First-boot setup as an [`msip_serve::Flow`].

use std::sync::Arc;

use msip::daemon::{TurnSpec, ValidAnswer};
use msip::element::types;
use msip::msg::Outcome;
use msip_serve::{Flow, Step};

use crate::flow::{self, Advance, Page};
use crate::setup::Setup;

/// No `select` and no `log`... except the applying page has a log, and
/// the locale page has (disabled) selects. A surface must render every
/// type this flow can put on a page, whether or not the element is
/// enabled — a disabled element still has to be drawn.
const NEEDED: &[&str] = &[
    types::TEXT,
    types::STRING,
    types::SELECT,
    types::PROGRESS,
    types::LOG,
    types::ACTION,
];

pub struct Oobe {
    setup: Arc<dyn Setup>,
    page: Page,
}

impl Oobe {
    pub fn new(setup: Arc<dyn Setup>) -> Oobe {
        Oobe {
            setup,
            page: Page::Locale,
        }
    }
}

impl Flow for Oobe {
    fn needs(&self) -> &'static [&'static str] {
        NEEDED
    }

    fn open(&mut self) -> TurnSpec {
        flow::locale_page()
    }

    fn advance(&mut self, answer: &ValidAnswer) -> Step {
        match flow::advance(&self.page, answer, self.setup.as_ref()) {
            Some(Advance::Page(next, spec)) => {
                self.page = next;
                Step::Page(spec)
            }
            Some(Advance::Reject(errors)) => Step::Reject(errors),
            Some(Advance::Apply {
                account,
                password,
                hostname,
            }) => {
                self.page = Page::Applying;
                let setup = Arc::clone(&self.setup);
                Step::Work {
                    page: flow::applying_page(),
                    job: Box::new(move |p| {
                        // Idempotent by construction: a setup that
                        // crashed after creating the account must be
                        // able to run again, and the failure mode we are
                        // avoiding is exactly the one lpsd-first-account
                        // has — a provisioner that treats "already done"
                        // as an error.
                        if setup.account_exists(&account) {
                            p.log(format!("{account} already exists; keeping it"));
                        } else {
                            setup.create_account(&account, &password, p)?;
                        }
                        p.phase("phase.account", 100);
                        setup.set_hostname(&hostname, p)?;
                        p.phase("phase.hostname", 100);
                        Ok(())
                    }),
                }
            }
            None => Step::Refuse("answer does not advance setup"),
        }
    }

    fn completion_message(&self) -> Option<String> {
        Some("Setup is complete. You can log in now.".into())
    }

    /// Setup succeeded: retire it, so it does not run again.
    ///
    /// **Only on success.** A failed run leaves both service definitions
    /// in place, so the recovery for a setup that went wrong is to
    /// reboot and be asked again — which is the only recovery there is,
    /// since a machine with no account yet cannot be logged into to fix
    /// anything by hand.
    fn finished(&mut self, outcome: Outcome) {
        if outcome != Outcome::Complete {
            msip_serve::note(
                crate::TAG,
                "setup did not complete; leaving it to run again on the next boot",
            );
            return;
        }
        self.setup.retire();
    }
}
