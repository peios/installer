//! The first-boot flow: which page follows which answer.
//!
//! There is deliberately no way out. Until this finishes the machine
//! has no account, so an escape hatch produces a system nobody can log
//! into — Back moves within the flow, and nothing abandons it.

use msip::daemon::{TurnSpec, ValidAnswer};
use msip::element::{Element, types};
use serde_json::{Value, json};

use crate::setup::Setup;

/// Where the conversation is, and what it has gathered getting there.
///
/// A turn's answer carries only that turn's values, so anything asked
/// on one page and used on another is held here. That includes the
/// password, briefly, in the privileged daemon rather than in the
/// surface that collected it — and it goes out of scope when the job
/// that consumes it finishes.
#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Locale,
    Network,
    /// `prefill` is what the person typed last time, if they went back.
    Account {
        prefill: String,
    },
    Naming {
        account: String,
        password: String,
    },
    Applying,
}

pub enum Advance {
    Page(Page, TurnSpec),
    Reject(Vec<(String, String)>),
    Apply {
        account: String,
        password: String,
        hostname: String,
    },
}

fn text(r#ref: &str, body: &str) -> Element {
    let mut e = Element::new(r#ref, types::TEXT);
    e.state.insert("text".into(), json!(body));
    e
}

fn action(r#ref: &str, name: &str) -> Element {
    let mut e = Element::new(r#ref, types::ACTION);
    e.name = Some(name.into());
    e
}

fn primary(mut e: Element) -> Element {
    e.state.insert("primary".into(), json!(true));
    e
}

fn back() -> Element {
    let mut e = action("nav.back", "Back");
    e.state.insert("validate".into(), json!(false));
    e
}

fn disabled(mut e: Element, why: &str) -> Element {
    e.enabled = false;
    e.help = Some(why.into());
    e
}

fn field(r#ref: &str, name: &str, secret: bool) -> Element {
    let mut e = Element::new(r#ref, types::STRING);
    e.name = Some(name.into());
    e.required = true;
    e.secret = secret;
    e
}

pub fn locale_page() -> TurnSpec {
    TurnSpec {
        id: Some("oobe.locale".into()),
        name: Some("Welcome to Peios".into()),
        elements: vec![
            text(
                "locale.intro",
                "A few questions and this machine is ready to use.",
            ),
            disabled(
                {
                    let mut e = Element::new("locale.language", types::SELECT);
                    e.name = Some("Language".into());
                    e.state.insert("choices".into(), json!([]));
                    e
                },
                "Peios ships in English only for now; there is no locale data to choose from yet.",
            ),
            disabled(
                {
                    let mut e = Element::new("locale.keyboard", types::SELECT);
                    e.name = Some("Keyboard layout".into());
                    e.state.insert("choices".into(), json!([]));
                    e
                },
                "No keymaps are packaged yet. Until they are, the console is US layout — \
                 which matters most on the next page, where a password is typed.",
            ),
            primary(action("nav.next", "Next")),
        ],
        ..Default::default()
    }
}

pub fn network_page(status: &str) -> TurnSpec {
    TurnSpec {
        id: Some("oobe.network".into()),
        name: Some("Network".into()),
        elements: vec![
            text("network.status", status),
            text(
                "network.note",
                "Setup does not need a network, and nothing here has to be answered.",
            ),
            disabled(
                action("network.wifi", "Connect to Wi-Fi…"),
                "No wireless stack is packaged yet.",
            ),
            disabled(
                action("network.static", "Configure manually…"),
                "Manual addressing is configured after setup, with `net`.",
            ),
            back(),
            primary(action("nav.next", "Next")),
        ],
        ..Default::default()
    }
}

pub fn account_page(prefill: &str) -> TurnSpec {
    let mut name = field("account.name", "User name", false);
    name.default = Some(json!(prefill));
    TurnSpec {
        id: Some("oobe.account".into()),
        name: Some("Create your account".into()),
        elements: vec![
            text(
                "account.intro",
                "This account administers the machine. It is created here rather than \
                 shipped in the image, so nothing on this system carries a password \
                 anyone else could know.",
            ),
            name,
            field("account.password", "Password", true),
            field("account.confirm", "Confirm password", true),
            back(),
            primary(action("nav.next", "Next")),
        ],
        ..Default::default()
    }
}

pub fn naming_page(suggested: &str) -> TurnSpec {
    let mut host = field("hostname", "Machine name", false);
    host.default = Some(json!(suggested));
    host.help = Some("How this machine identifies itself on a network.".into());
    TurnSpec {
        id: Some("oobe.naming".into()),
        name: Some("Name this machine".into()),
        elements: vec![
            host,
            disabled(
                action("domain.join", "Join a domain instead…"),
                "Domain membership needs a directory source, which is not built yet. \
                 A machine that joins one still keeps this local account.",
            ),
            back(),
            primary(action("nav.finish", "Finish")),
        ],
        ..Default::default()
    }
}

pub fn applying_page() -> TurnSpec {
    let mut account = Element::new("phase.account", types::PROGRESS);
    account.name = Some("Creating your account".into());
    account.state.insert("value".into(), json!(0));
    account.state.insert("max".into(), json!(100));
    let mut naming = Element::new("phase.hostname", types::PROGRESS);
    naming.name = Some("Naming the machine".into());
    naming.state.insert("value".into(), json!(0));
    naming.state.insert("max".into(), json!(100));
    let mut log = Element::new("out", types::LOG);
    log.name = Some("Details".into());
    log.state.insert("lines".into(), json!([]));
    TurnSpec {
        id: Some("oobe.applying".into()),
        name: Some("Finishing setup".into()),
        elements: vec![account, naming, log],
        class: vec!["progress".into()],
    }
}

fn value(answer: &ValidAnswer, r#ref: &str) -> String {
    answer
        .values
        .get(r#ref)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn advance(page: &Page, answer: &ValidAnswer, setup: &dyn Setup) -> Option<Advance> {
    let act = answer.action.as_deref().unwrap_or("");
    match (page, act) {
        (Page::Locale, "nav.next") => Some(Advance::Page(
            Page::Network,
            network_page(&setup.network_status()),
        )),
        (Page::Network, "nav.back") => Some(Advance::Page(Page::Locale, locale_page())),
        (Page::Network, "nav.next") => Some(Advance::Page(
            Page::Account {
                prefill: "peios".into(),
            },
            account_page("peios"),
        )),
        (Page::Account { .. }, "nav.back") => Some(Advance::Page(
            Page::Network,
            network_page(&setup.network_status()),
        )),
        (Page::Account { .. }, "nav.next") => {
            let account = value(answer, "account.name");
            let password = value(answer, "account.password");
            if password != value(answer, "account.confirm") {
                // On confirm, not on password: the person retypes the
                // one they got wrong, and the first field keeps what
                // they meant.
                return Some(Advance::Reject(vec![(
                    "account.confirm".into(),
                    "The passwords do not match.".into(),
                )]));
            }
            Some(Advance::Page(
                Page::Naming { account, password },
                naming_page(&setup.suggested_hostname()),
            ))
        }
        // Back from naming re-asks for the password rather than keeping
        // it: it is the one answer worth making the person confirm
        // again, and the name they chose is offered back to them.
        (Page::Naming { account, .. }, "nav.back") => Some(Advance::Page(
            Page::Account {
                prefill: account.clone(),
            },
            account_page(account),
        )),
        (Page::Naming { account, password }, "nav.finish") => Some(Advance::Apply {
            account: account.clone(),
            password: password.clone(),
            hostname: value(answer, "hostname"),
        }),
        _ => None,
    }
}
