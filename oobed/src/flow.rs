//! The first-boot flow: which page follows which answer.
//!
//! There is deliberately no way out. Until this finishes the machine
//! has no account, so an escape hatch produces a system nobody can log
//! into — Back moves within the flow, and nothing abandons it.

use msip::daemon::{TurnSpec, ValidAnswer};
use msip::element::{Element, types};
use serde_json::{Map, Value, json};

use crate::network::{Answered, Manual, Network};
use crate::setup::Setup;

/// Where the conversation is, and what it has gathered getting there.
///
/// A turn's answer carries only that turn's values, so anything asked
/// on one page and used on another is held here. That includes the
/// password, briefly, in the privileged daemon rather than in the
/// surface that collected it — and it goes out of scope when the job
/// that consumes it finishes.
///
/// A manual address is held beside the page rather than in it
/// ([`Advance::Plan`]), being kept whichever page setup is on from the
/// network page to the end.
#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Locale,
    Network,
    /// An address given to an interface by hand.
    Manual,
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
    /// Change the open page in place.
    Patch(Map<String, Value>),
    /// Keep this manual address to apply at the end, or none, and go back
    /// to the network page, which says so.
    Plan(Option<Manual>, TurnSpec),
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

/// An action that leaves the page or refreshes it, and carries nothing of
/// what was filled in.
fn no_validate(mut e: Element) -> Element {
    e.state.insert("validate".into(), json!(false));
    e
}

fn back() -> Element {
    no_validate(action("nav.back", "Back"))
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

/// What the network page says of the machine's network: its words, and
/// the same as `detail` ([`crate::network`]) where netd could be asked.
fn network_said(setup: &dyn Setup) -> (String, Option<Value>) {
    match setup.net_status() {
        Some(printed) => {
            let network = Network::parse(&printed);
            (network.words(), Some(network.detail()))
        }
        None => ("Could not ask netd about the network.".into(), None),
    }
}

/// The network page. `plan` is the manual address kept for the end of
/// setup, if one is, which the page says beside what the network is now.
pub fn network_page(setup: &dyn Setup, plan: Option<&Manual>) -> TurnSpec {
    let (words, detail) = network_said(setup);
    let mut status = text("network.status", &words);
    if let Some(detail) = detail {
        status.state.insert("detail".into(), detail);
    }
    let mut elements = vec![status];
    if let Some(plan) = plan {
        let mut planned = text("network.planned", &plan.words());
        planned.state.insert("detail".into(), plan.detail());
        elements.push(planned);
    }
    elements.push(text(
        "network.note",
        "Setup does not need a network, and nothing here has to be answered.",
    ));
    elements.push(no_validate(action("network.refresh", "Check again")));
    elements.push(disabled(
        action("network.wifi", "Connect to Wi-Fi…"),
        "No wireless stack is packaged yet.",
    ));
    match plan {
        None => elements.push(no_validate(action("network.static", "Configure manually…"))),
        Some(_) => {
            elements.push(no_validate(action(
                "network.static",
                "Change the manual address…",
            )));
            elements.push(no_validate(action(
                "network.unplan",
                "Use the network's address instead",
            )));
        }
    }
    elements.push(back());
    elements.push(primary(action("nav.next", "Next")));
    TurnSpec {
        id: Some("oobe.network".into()),
        name: Some("Network".into()),
        elements,
        ..Default::default()
    }
}

/// An interface as the manual page offers it: a row of what it is now,
/// and only a wired one can be chosen (`Interface::addressable`).
fn manual_row(i: &crate::network::Interface) -> Value {
    let address = i
        .addresses
        .iter()
        .find(|a| !a.to_ascii_lowercase().starts_with("fe80:"))
        .cloned()
        .unwrap_or_default();
    let mut row = json!({
        "value": i.name,
        "cells": { "name": i.name, "state": i.how(), "address": address },
    });
    if !i.addressable() {
        row["enabled"] = json!(false);
        row["note"] = json!("not wired");
    }
    row
}

/// The page that gives one interface an address by hand. `plan` is the
/// address already kept, if one is, to start from.
pub fn manual_page(setup: &dyn Setup, plan: Option<&Manual>) -> TurnSpec {
    let network = setup.net_status().map(|printed| Network::parse(&printed));
    let interfaces = network
        .as_ref()
        .map(|n| n.interfaces.as_slice())
        .unwrap_or_default();
    let mut interface = Element::new("manual.interface", types::TABLE);
    interface.name = Some("Interface".into());
    interface.required = true;
    interface.state.insert(
        "columns".into(),
        json!([
            { "key": "name", "name": "Interface" },
            { "key": "state", "name": "Status" },
            { "key": "address", "name": "Address now" },
        ]),
    );
    interface.state.insert(
        "rows".into(),
        Value::Array(interfaces.iter().map(manual_row).collect()),
    );
    interface.state.insert(
        "empty".into(),
        json!(match network {
            Some(_) => "This machine has no network hardware that Peios can use.",
            None => "Could not ask netd about the network, so there is no interface to choose.",
        }),
    );
    // The one kept, or the only one there is to choose.
    let addressable: Vec<_> = interfaces.iter().filter(|i| i.addressable()).collect();
    interface.default = match (plan, addressable.as_slice()) {
        (Some(plan), _) if addressable.iter().any(|i| i.name == plan.interface) => {
            Some(json!(plan.interface))
        }
        (_, [only]) => Some(json!(only.name)),
        _ => None,
    };
    let mut address = field("manual.address", "Address", false);
    address
        .state
        .insert("placeholder".into(), json!("192.168.1.20/24"));
    address.help = Some("With the length of its network after a slash.".into());
    let mut gateway = field("manual.gateway", "Gateway", false);
    gateway.required = false;
    gateway
        .state
        .insert("placeholder".into(), json!("192.168.1.1"));
    gateway.help = Some("Where traffic for other networks goes. Leave it empty for a machine that talks only to its own network.".into());
    let mut dns = field("manual.dns", "Name servers", false);
    dns.required = false;
    dns.state
        .insert("placeholder".into(), json!("1.1.1.1 9.9.9.9"));
    dns.help = Some("Separated by spaces. Leave it empty for none.".into());
    if let Some(plan) = plan {
        address.default = Some(json!(plan.address));
        gateway.default = plan.gateway.as_ref().map(|g| json!(g));
        dns.default = Some(json!(plan.dns.join(" ")));
    }
    TurnSpec {
        id: Some("oobe.network.manual".into()),
        name: Some("Configure manually".into()),
        elements: vec![
            text(
                "manual.intro",
                "Give one interface an address of its own instead of the one the network offers. \
                 It is applied at the end of setup, after the account and the machine's name; \
                 until then the machine keeps the address it has now.",
            ),
            interface,
            address,
            gateway,
            dns,
            back(),
            primary(action("manual.save", "Save")),
        ],
        ..Default::default()
    }
}

/// What checking the network again changes on the open network page.
pub fn network_patch(setup: &dyn Setup) -> Map<String, Value> {
    let (words, detail) = network_said(setup);
    let mut patch = Map::new();
    patch.insert("ref".into(), json!("network.status"));
    patch.insert("text".into(), json!(words));
    // Null takes the field away (§3.10): netd that answered before and
    // does not now has said nothing of the interfaces it listed.
    patch.insert("detail".into(), detail.unwrap_or(Value::Null));
    patch
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

/// What applying setup shows. `plan` is the manual address, applied last,
/// which gets a phase of its own and, as `detail` on it, where the machine
/// will be: a surface reached through that interface loses the machine as
/// it is applied, and can go on to the new address.
pub fn applying_page(plan: Option<&Manual>) -> TurnSpec {
    let phase = |r#ref: &str, name: &str| {
        let mut e = Element::new(r#ref, types::PROGRESS);
        e.name = Some(name.into());
        e.state.insert("value".into(), json!(0));
        e.state.insert("max".into(), json!(100));
        e
    };
    let mut elements = vec![
        phase("phase.account", "Creating your account"),
        phase("phase.hostname", "Naming the machine"),
    ];
    if let Some(plan) = plan {
        let mut network = phase("phase.network", &format!("Addressing {}", plan.interface));
        network.state.insert("detail".into(), plan.detail());
        elements.push(network);
    }
    let mut log = Element::new("out", types::LOG);
    log.name = Some("Details".into());
    log.state.insert("lines".into(), json!([]));
    elements.push(log);
    TurnSpec {
        id: Some("oobe.applying".into()),
        name: Some("Finishing setup".into()),
        elements,
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

/// Which page follows `answer` on `page`. `plan` is the manual address
/// kept so far, if one is.
pub fn advance(
    page: &Page,
    answer: &ValidAnswer,
    setup: &dyn Setup,
    plan: Option<&Manual>,
) -> Option<Advance> {
    let act = answer.action.as_deref().unwrap_or("");
    match (page, act) {
        (Page::Locale, "nav.next") => Some(Advance::Page(Page::Network, network_page(setup, plan))),
        (Page::Network, "network.refresh") => Some(Advance::Patch(network_patch(setup))),
        (Page::Network, "network.static") => {
            Some(Advance::Page(Page::Manual, manual_page(setup, plan)))
        }
        (Page::Network, "network.unplan") => Some(Advance::Plan(None, network_page(setup, None))),
        (Page::Network, "nav.back") => Some(Advance::Page(Page::Locale, locale_page())),
        (Page::Manual, "nav.back") => Some(Advance::Page(Page::Network, network_page(setup, plan))),
        (Page::Manual, "manual.save") => {
            let network = Network::parse(&setup.net_status().unwrap_or_default());
            let answered = Answered {
                interface: &value(answer, "manual.interface"),
                address: &value(answer, "manual.address"),
                gateway: &value(answer, "manual.gateway"),
                dns: &value(answer, "manual.dns"),
            };
            Some(match Manual::check(answered, &network) {
                Ok(manual) => {
                    let spec = network_page(setup, Some(&manual));
                    Advance::Plan(Some(manual), spec)
                }
                Err(wrong) => Advance::Reject(wrong),
            })
        }
        (Page::Network, "nav.next") => Some(Advance::Page(
            Page::Account {
                prefill: "peios".into(),
            },
            account_page("peios"),
        )),
        (Page::Account { .. }, "nav.back") => {
            Some(Advance::Page(Page::Network, network_page(setup, plan)))
        }
        (Page::Account { .. }, "nav.next") => {
            let account = value(answer, "account.name");
            let password = value(answer, "account.password");
            // Setup keeps an account that already exists, and this one does
            // while setup runs: taking its name would finish setup with no
            // account the person can use, and remove it besides.
            if account.trim().eq_ignore_ascii_case(crate::setup::VISITOR) {
                return Some(Advance::Reject(vec![(
                    "account.name".into(),
                    "That name is used by setup itself. Choose another.".into(),
                )]));
            }
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
