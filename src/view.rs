//! What every page is shown: the state this process holds, as it is sent.
//!
//! The process holds the state and a page draws it. A page decides nothing
//! about where the installation stands: it is told which page this is and
//! what is on it, and how it gets from one page to the next, the animation
//! included, is its own business. Every browser is sent the same thing.
//!
//! This is the installer's front end and no other's. It knows installerd's
//! pages by their ids and their elements by their refs, and draws each as the
//! page it is. It does not try to draw a conversation it has never seen.

use msip::element::Element;
use msip::msg::{End, Outcome, Turn};
use serde::Serialize;
use serde_json::Value;

use crate::release::Release;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    /// Which Peios the medium carries.
    pub release: Release,
    /// How things stand with installerd.
    pub link: Link,
    /// What this process did to get here, in order, for a page to show while
    /// it starts. Each is a word and what it was done to.
    pub trail: Vec<(String, String)>,
    /// Which of installerd's pages `page` is, by installerd's own count of
    /// them. A browser says it back with whatever it asks for, so that what
    /// was pressed on a page that has since gone is not taken for an answer
    /// to the one that replaced it. Nothing when installerd has asked
    /// nothing.
    pub seq: u64,
    /// The action pressed on this page that installerd has not answered
    /// yet, for a page to show as under way.
    pub waiting: Option<String>,
    /// The page installerd is asking for, once it has asked.
    pub page: Option<Page>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Link {
    Connecting,
    Connected {
        /// What installerd calls itself, as `installerd/0.1.10`.
        daemon: String,
    },
    /// It is being tried again; `why` is for the person looking.
    Lost { why: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[expect(clippy::large_enum_variant, reason = "there is one page at a time, and nothing is saved by boxing it")]
pub enum Page {
    /// installerd's `mode`: what to do with this machine.
    Mode { title: String, intro: String, actions: Vec<Action> },
    /// installerd's `disk.choose`: which disk.
    Disk {
        title: String,
        intro: String,
        /// What the disk is being chosen for -- `install`, `upgrade` or
        /// `repair` -- or nothing when installerd does not say.
        purpose: String,
        disks: Vec<Disk>,
        /// What installerd says in place of the disks when there are none.
        empty: String,
        /// What installerd has to say about the disks as a whole: today, a
        /// storage controller nothing is driving.
        trouble: Option<String>,
        /// Why installerd turned the last answer down.
        error: Option<String>,
        /// The disk chosen so far. It is held here, not in a browser, so
        /// everyone looking sees the same one chosen.
        chosen: Option<String>,
        rescan: Option<Action>,
        custom: Option<Action>,
        back: Option<Action>,
        next: Option<Action>,
    },
    /// A page of installerd's that is not drawn here yet.
    Unbuilt { id: String, title: String },
    /// The conversation is over.
    Ended { outcome: &'static str, message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Action {
    pub r#ref: String,
    pub name: String,
    /// The one a person most likely wants.
    pub primary: bool,
    pub enabled: bool,
    /// Why it cannot be chosen, where it cannot.
    pub help: Option<String>,
    /// It leads to a page that is not drawn here yet, so pressing it says
    /// so instead of answering.
    pub unbuilt: bool,
}

/// One row of installerd's table of disks.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Disk {
    /// What choosing it answers with: its device.
    pub value: String,
    pub device: String,
    pub model: String,
    /// Its size as installerd words it, `931.5 GiB`.
    pub size: String,
    pub bus: String,
    /// A remark installerd makes of it: `boot medium`, `removable`.
    pub note: String,
    /// Whether it can be chosen. The boot medium is listed and cannot.
    pub enabled: bool,
    /// What installerd knows of the disk beyond its row: its partitions,
    /// what it would become, the system on it. Passed on as it came for the
    /// page to draw, and nothing at all from an installerd that does not
    /// send it.
    pub detail: Value,
}

/// The table on installerd's disk page.
const DISKS: &str = "disk.target";

impl View {
    pub fn starting(release: Release) -> View {
        View { release, link: Link::Connecting, trail: Vec::new(), seq: 0, waiting: None, page: None }
    }

    pub fn did(&mut self, what: &str, to: impl Into<String>) {
        self.trail.push((what.to_string(), to.into()));
    }

    /// No page is being asked for: installerd is not there, or has finished.
    pub fn without_a_turn(&mut self) {
        self.seq = 0;
        self.waiting = None;
    }
}

/// Whether pressing `action` on `turn` leads to a page that is not drawn
/// here yet. Such an action is not sent on: the conversation is installerd's
/// and shared, and an answer would move everyone to a page this cannot show.
pub fn unbuilt(turn: &Turn, action: &str) -> bool {
    matches!((turn.id.as_deref(), action), (Some("disk.choose"), "nav.next"))
}

/// Whether `value` is a disk `turn` offers and lets be chosen.
pub fn can_choose(turn: &Turn, value: &str) -> bool {
    rows(turn).iter().any(|row| row.get("value").and_then(Value::as_str) == Some(value) && enabled(row))
}

/// What is answered for `turn` beside the action pressed: the disk chosen,
/// on the page that asks for one.
pub fn values(turn: &Turn, chosen: Option<&str>) -> serde_json::Map<String, Value> {
    let mut values = serde_json::Map::new();
    if let (Some("disk.choose"), Some(chosen)) = (turn.id.as_deref(), chosen) {
        values.insert(DISKS.into(), Value::String(chosen.into()));
    }
    values
}

impl Page {
    /// The page for a turn of installerd's, by the turn's id. `chosen` is
    /// the disk chosen on it so far.
    pub fn of(turn: &Turn, chosen: Option<&str>) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let action = |r#ref: &str| element(turn, r#ref).filter(|e| e.is_action()).map(|e| Action::of(turn, e));
        match turn.id.as_deref() {
            Some("mode") => Page::Mode {
                title,
                intro: text(turn, "mode.intro"),
                actions: turn.elements.iter().filter(|e| e.is_action()).map(|e| Action::of(turn, e)).collect(),
            },
            Some("disk.choose") => {
                let table = element(turn, DISKS);
                Page::Disk {
                    title,
                    intro: text(turn, "disk.intro"),
                    purpose: ["install", "upgrade", "repair"]
                        .into_iter()
                        .find(|purpose| turn.class.iter().any(|class| class == purpose))
                        .unwrap_or_default()
                        .to_string(),
                    disks: rows(turn).iter().map(Disk::of).collect(),
                    empty: table.and_then(|t| t.state.get("empty")).and_then(Value::as_str).unwrap_or_default().to_string(),
                    trouble: table.and_then(|t| t.help.clone()),
                    error: table.and_then(|t| t.error.clone()),
                    chosen: chosen.map(str::to_string),
                    rescan: action("act.rescan"),
                    custom: action("disk.custom"),
                    back: action("nav.back"),
                    next: action("nav.next"),
                }
            }
            id => Page::Unbuilt { id: id.unwrap_or_default().to_string(), title },
        }
    }

    pub fn ended(end: &End) -> Page {
        let outcome = match end.outcome {
            Outcome::Complete => "complete",
            Outcome::Cancelled => "cancelled",
            Outcome::Failed => "failed",
        };
        Page::Ended { outcome, message: end.message.clone().unwrap_or_default() }
    }
}

impl Action {
    fn of(turn: &Turn, element: &Element) -> Action {
        Action {
            r#ref: element.r#ref.clone(),
            name: element.name.clone().unwrap_or_default(),
            primary: element.state.get("primary").and_then(Value::as_bool).unwrap_or(false),
            enabled: element.enabled,
            help: element.help.clone(),
            unbuilt: unbuilt(turn, &element.r#ref),
        }
    }
}

impl Disk {
    fn of(row: &Value) -> Disk {
        let said = |value: Option<&Value>| value.and_then(Value::as_str).unwrap_or_default().to_string();
        let cell = |key: &str| said(row.get("cells").and_then(|cells| cells.get(key)));
        Disk {
            value: said(row.get("value")),
            device: cell("device"),
            model: cell("model"),
            size: cell("size"),
            bus: cell("bus"),
            note: said(row.get("note")),
            enabled: enabled(row),
            detail: row.get("detail").cloned().unwrap_or(Value::Null),
        }
    }
}

fn element<'a>(turn: &'a Turn, r#ref: &str) -> Option<&'a Element> {
    turn.elements.iter().find(|e| e.r#ref == r#ref)
}

/// What the text element `r#ref` of `turn` says, or nothing.
fn text(turn: &Turn, r#ref: &str) -> String {
    element(turn, r#ref).and_then(|e| e.state.get("text")).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The rows of the table of disks, or none where `turn` has no such table.
fn rows(turn: &Turn) -> &[Value] {
    element(turn, DISKS).and_then(|e| e.state.get("rows")).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

/// A row can be chosen unless it says otherwise (PGSS §3.B).
fn enabled(row: &Value) -> bool {
    row.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// installerd's first page, as it sends it (`installerd/src/flow.rs`).
    fn mode() -> Turn {
        serde_json::from_value(json!({
            "seq": 1, "id": "mode", "name": "Peios Setup", "class": ["menu"],
            "elements": [
                { "ref": "mode.intro", "type": "text", "text": "Set up Peios on this machine." },
                { "ref": "act.install", "type": "action", "name": "Install Peios", "primary": true },
                { "ref": "act.upgrade", "type": "action", "name": "Upgrade an installation" },
                { "ref": "act.repair", "type": "action", "name": "Repair an existing system",
                  "enabled": false, "help": "Not on this medium." },
            ],
        }))
        .unwrap()
    }

    /// installerd's disk page, for an install, with one disk to choose and
    /// the medium it booted from.
    fn disks() -> Turn {
        serde_json::from_value(json!({
            "seq": 2, "id": "disk.choose", "name": "Choose a disk", "class": ["install"],
            "elements": [
                { "ref": "disk.intro", "type": "text", "text": "Choose the disk to install onto." },
                { "ref": "disk.target", "type": "table", "name": "Target disk", "required": true,
                  "help": "A storage controller that nothing on this system is driving.",
                  "empty": "No disks found. Attach one and rescan.",
                  "columns": [{ "key": "device", "name": "Device" }],
                  "rows": [
                    { "value": "/dev/sda", "note": "removable",
                      "cells": { "device": "/dev/sda", "model": "CT500MX500SSD1", "size": "465.8 GiB", "bus": "SATA" },
                      "detail": { "bytes": 500107862016u64, "partitions": [] } },
                    { "value": "/dev/sdb", "enabled": false, "note": "boot medium",
                      "cells": { "device": "/dev/sdb", "model": "SanDisk Ultra Fit", "size": "28.6 GiB", "bus": "USB" } },
                  ] },
                { "ref": "act.rescan", "type": "action", "name": "Rescan disks", "validate": false },
                { "ref": "disk.custom", "type": "action", "name": "Custom partitioning…",
                  "enabled": false, "help": "Whole-disk only for now." },
                { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
                { "ref": "nav.next", "type": "action", "name": "Next", "primary": true },
            ],
        }))
        .unwrap()
    }

    #[test]
    fn the_first_page_is_drawn_from_what_installerd_sends() {
        let Page::Mode { title, intro, actions } = Page::of(&mode(), None) else { panic!("not the mode page") };
        assert_eq!(title, "Peios Setup");
        assert_eq!(intro, "Set up Peios on this machine.");
        assert_eq!(actions.iter().map(|a| a.r#ref.as_str()).collect::<Vec<_>>(), ["act.install", "act.upgrade", "act.repair"]);
        assert_eq!(actions.iter().map(|a| a.primary).collect::<Vec<_>>(), [true, false, false]);
        assert!(actions[0].enabled && actions[0].help.is_none());
        assert!(!actions[2].enabled);
        assert_eq!(actions[2].help.as_deref(), Some("Not on this medium."));
        // Every one of them leads to the disk page, which is drawn.
        assert!(actions.iter().all(|a| !a.unbuilt));
    }

    #[test]
    fn the_disk_page_is_drawn_from_what_installerd_sends() {
        let Page::Disk { title, intro, purpose, disks, empty, trouble, error, chosen, rescan, custom, back, next } =
            Page::of(&disks(), Some("/dev/sda"))
        else {
            panic!("not the disk page")
        };
        assert_eq!((title.as_str(), intro.as_str(), purpose.as_str()), ("Choose a disk", "Choose the disk to install onto.", "install"));
        assert_eq!(empty, "No disks found. Attach one and rescan.");
        assert_eq!(trouble.as_deref(), Some("A storage controller that nothing on this system is driving."));
        assert_eq!(error, None);
        assert_eq!(chosen.as_deref(), Some("/dev/sda"));
        assert_eq!(
            disks[0],
            Disk {
                value: "/dev/sda".into(),
                device: "/dev/sda".into(),
                model: "CT500MX500SSD1".into(),
                size: "465.8 GiB".into(),
                bus: "SATA".into(),
                note: "removable".into(),
                enabled: true,
                detail: json!({ "bytes": 500107862016u64, "partitions": [] }),
            }
        );
        // The medium is listed and cannot be chosen, and an installerd that
        // sends no detail is drawn without any.
        assert!(!disks[1].enabled);
        assert_eq!(disks[1].note, "boot medium");
        assert_eq!(disks[1].detail, Value::Null);
        assert_eq!(rescan.unwrap().name, "Rescan disks");
        let custom = custom.unwrap();
        assert!(!custom.enabled);
        assert_eq!(custom.help.as_deref(), Some("Whole-disk only for now."));
        assert!(!back.unwrap().unbuilt);
        let next = next.unwrap();
        assert!(next.primary && next.enabled);
        // The page after this one is not drawn yet.
        assert!(next.unbuilt);
    }

    #[test]
    fn what_a_disk_is_chosen_for_is_read_from_the_page_not_guessed() {
        let mut turn = disks();
        turn.class = vec!["something".into(), "repair".into()];
        assert!(matches!(Page::of(&turn, None), Page::Disk { purpose, .. } if purpose == "repair"));
        turn.class.clear();
        assert!(matches!(Page::of(&turn, None), Page::Disk { purpose, .. } if purpose.is_empty()));
    }

    #[test]
    fn only_a_disk_that_is_offered_can_be_chosen() {
        assert!(can_choose(&disks(), "/dev/sda"));
        // The boot medium is listed, and is not a choice.
        assert!(!can_choose(&disks(), "/dev/sdb"));
        assert!(!can_choose(&disks(), "/dev/nvme0n1"));
        assert!(!can_choose(&mode(), "/dev/sda"));
    }

    #[test]
    fn the_disk_chosen_is_answered_with_on_the_page_that_asks_for_one() {
        assert_eq!(Value::Object(values(&disks(), Some("/dev/sda"))), json!({ "disk.target": "/dev/sda" }));
        assert!(values(&disks(), None).is_empty());
        assert!(values(&mode(), Some("/dev/sda")).is_empty());
    }

    #[test]
    fn an_action_that_leads_to_a_page_not_drawn_is_not_sent_on() {
        assert!(unbuilt(&disks(), "nav.next"));
        assert!(!unbuilt(&disks(), "nav.back"));
        assert!(!unbuilt(&disks(), "act.rescan"));
        assert!(!unbuilt(&mode(), "act.install"));
    }

    #[test]
    fn a_page_not_drawn_here_is_said_to_be_one() {
        let mut turn = mode();
        turn.id = Some("confirm".into());
        turn.name = Some("Ready to install".into());
        assert_eq!(Page::of(&turn, None), Page::Unbuilt { id: "confirm".into(), title: "Ready to install".into() });
        turn.id = None;
        assert!(matches!(Page::of(&turn, None), Page::Unbuilt { id, .. } if id.is_empty()));
    }

    #[test]
    fn a_page_is_sent_as_the_kind_it_is() {
        let view = View {
            release: Release { version: "2026.8".into(), variant: "Experimental".into() },
            link: Link::Connected { daemon: "installerd/0.1.10".into() },
            trail: vec![("connect".into(), "/run/installerd.sock".into())],
            seq: 1,
            waiting: Some("act.install".into()),
            page: Some(Page::of(&mode(), None)),
        };
        let sent = serde_json::to_value(&view).unwrap();
        assert_eq!(sent["link"], json!({ "state": "connected", "daemon": "installerd/0.1.10" }));
        assert_eq!(sent["seq"], 1);
        assert_eq!(sent["waiting"], "act.install");
        assert_eq!(sent["page"]["kind"], "mode");
        assert_eq!(sent["page"]["actions"][0], json!({
            "ref": "act.install", "name": "Install Peios", "primary": true, "enabled": true, "help": null, "unbuilt": false,
        }));
        assert_eq!(sent["trail"], json!([["connect", "/run/installerd.sock"]]));
        assert_eq!(sent["release"], json!({ "version": "2026.8", "variant": "Experimental" }));

        let sent = serde_json::to_value(Page::of(&disks(), Some("/dev/sda"))).unwrap();
        assert_eq!(sent["kind"], "disk");
        assert_eq!(sent["chosen"], "/dev/sda");
        assert_eq!(sent["disks"][0]["detail"]["bytes"], 500107862016u64);
        assert_eq!(sent["next"]["unbuilt"], true);
    }

    #[test]
    fn an_ending_says_how_it_ended() {
        let end = End { seq: 9, outcome: Outcome::Failed, message: Some("cp failed (exit 1)".into()) };
        assert_eq!(Page::ended(&end), Page::Ended { outcome: "failed", message: "cp failed (exit 1)".into() });
    }
}
