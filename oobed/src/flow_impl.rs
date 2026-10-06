//! First-boot setup as an [`msip_serve::Flow`].

use std::sync::Arc;

use msip::daemon::{TurnSpec, ValidAnswer};
use msip::element::types;
use msip::msg::Outcome;
use msip_serve::{Flow, Step};

use crate::flow::{self, Advance, Page};
use crate::network::Manual;
use crate::setup::Setup;

/// Every type this flow can put on a page: the applying page has a log,
/// the locale page has (disabled) selects, and the manual page a table of
/// interfaces. A surface must render every type the flow can send,
/// whether or not the element is enabled — a disabled element still has
/// to be drawn.
const NEEDED: &[&str] = &[
    types::TEXT,
    types::STRING,
    types::SELECT,
    types::TABLE,
    types::PROGRESS,
    types::LOG,
    types::ACTION,
];

pub struct Oobe {
    setup: Arc<dyn Setup>,
    page: Page,
    /// The manual address kept for the end, if one is.
    plan: Option<Manual>,
    /// What the first page last said of where the machine is in a browser,
    /// once a refresh has said it; nothing until then.
    browse: Option<String>,
}

impl Oobe {
    pub fn new(setup: Arc<dyn Setup>) -> Oobe {
        Oobe {
            setup,
            page: Page::Locale,
            plan: None,
            browse: None,
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

    /// While the first page is open, where the machine is in a browser, as
    /// it comes: the page was made as the machine started, often before it
    /// had an address.
    fn refresh(&mut self) -> Vec<serde_json::Map<String, serde_json::Value>> {
        if self.page != Page::Locale {
            return Vec::new();
        }
        let Some(now) = msip_serve::browser::where_to_browse() else { return Vec::new() };
        if self.browse.as_deref() == Some(now.as_str()) {
            return Vec::new();
        }
        let mut patch = serde_json::Map::new();
        patch.insert("ref".into(), serde_json::Value::String("locale.browser".into()));
        patch.insert("text".into(), serde_json::Value::String(now.clone()));
        self.browse = Some(now);
        vec![patch]
    }

    fn advance(&mut self, answer: &ValidAnswer) -> Step {
        match flow::advance(&self.page, answer, self.setup.as_ref(), self.plan.as_ref()) {
            Some(Advance::Page(next, spec)) => {
                self.page = next;
                self.browse = None;
                Step::Page(spec)
            }
            Some(Advance::Patch(patch)) => Step::Patch(vec![patch]),
            Some(Advance::Plan(plan, spec)) => {
                self.plan = plan;
                self.page = Page::Network;
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
                let plan = self.plan.clone();
                Step::Work {
                    page: flow::applying_page(plan.as_ref()),
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
                        // Last: whatever was reaching the machine through
                        // this interface loses it here, and everything
                        // else is done by the time it does.
                        if let Some(plan) = &plan {
                            setup.set_address(plan, p)?;
                            p.phase("phase.network", 100);
                        }
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
