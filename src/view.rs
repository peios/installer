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

use msip::element::{Element, types};
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
    /// yet, for a page to show as under way. After an ending it is
    /// `again`, while the conversation asked for in its place is opened.
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
    /// installerd's `confirm`: whether the disk chosen is really to be
    /// erased and installed onto.
    Confirm {
        title: String,
        /// The whole of it in words: which disk, that all of it goes, and
        /// what a person would miss of what is on it.
        summary: String,
        /// The disk, as installerd read it for this page, for the page to
        /// draw. Nothing from an installerd that says it only in words.
        disk: Option<Disk>,
        back: Option<Action>,
        begin: Option<Action>,
    },
    /// installerd's `install.progress`, and the upgrade's and the repair's:
    /// a job as it runs, and how it ended once it has.
    Progress {
        title: String,
        /// Which job it is: `install`, `upgrade` or `repair`.
        job: String,
        /// What it is being done to, in installerd's words.
        summary: String,
        /// The disk it is being done to, for the page to draw. Nothing from
        /// an installerd that says it only in words.
        disk: Option<Disk>,
        phases: Vec<Phase>,
        /// The last of what the job has said of itself, a line each. The
        /// whole of it is served as a file, since a job that goes wrong can
        /// say a great deal and every line here is sent again with each
        /// change.
        lines: Vec<String>,
        /// How many lines it has said in all.
        said: usize,
        /// How it ended, once it has: the job's end is the conversation's,
        /// and the page stays to say so.
        ended: Option<Ended>,
    },
    /// A page of installerd's that is not drawn here yet.
    Unbuilt { id: String, title: String },
    /// The conversation is over, and not on a page that stays to say so.
    Ended { outcome: &'static str, message: String },
}

/// One phase of a job, and how far along it is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Phase {
    pub r#ref: String,
    pub name: String,
    pub value: f64,
    /// The whole of it, or nothing where installerd cannot say how far
    /// there is to go.
    pub max: Option<f64>,
}

/// How a conversation ended: `complete`, `cancelled` or `failed`, and what
/// installerd had to say of it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ended {
    pub outcome: &'static str,
    pub message: String,
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
    /// Pressing it destroys something (PGSS §3.B), so a page makes it hard
    /// to press by accident.
    pub destructive: bool,
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
/// The sentence on installerd's confirmation, which carries the disk.
const SUMMARY: &str = "confirm.summary";
/// The sentence on the page a job runs on, which carries it too.
const DOING: &str = "progress.summary";
/// What a job says of itself as it goes.
const LOG: &str = "out";
/// How many lines of that a page is sent.
const TAIL: usize = 200;

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
///
/// Which pages those are is known here by what the action is on. For the
/// disk page that takes what the disk is being chosen for, since the one
/// action leads to three pages; an installerd that does not say is taken to
/// lead somewhere not drawn. All of this goes as the pages are drawn.
pub fn unbuilt(turn: &Turn, action: &str) -> bool {
    match (turn.id.as_deref(), action) {
        // An install goes on to the confirmation, which is drawn; an
        // upgrade and a repair to pages of their own, which are not.
        (Some("disk.choose"), "nav.next") => purpose(turn) != "install",
        _ => false,
    }
}

/// Everything the job on `turn` has said of itself so far, a line each, or
/// nothing where `turn` is not a job's page.
pub fn said(turn: &Turn) -> &[Value] {
    element(turn, LOG).and_then(|e| e.state.get("lines")).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

/// Whether `value` is a disk `turn` offers and lets be chosen.
pub fn can_choose(turn: &Turn, value: &str) -> bool {
    rows(turn).iter().any(|row| row.get("value").and_then(Value::as_str) == Some(value) && enabled(row))
}

/// The disk `turn` arrives with already chosen: the table's `default`,
/// which is what a field starts from, and how a page come back to says
/// what was chosen on it.
pub fn assumed(turn: &Turn) -> Option<String> {
    let disk = element(turn, DISKS)?.default.as_ref()?.as_str()?;
    can_choose(turn, disk).then(|| disk.to_string())
}

/// What the disk on `turn` is being chosen for -- `install`, `upgrade` or
/// `repair` -- or nothing where installerd does not say.
fn purpose(turn: &Turn) -> &'static str {
    ["install", "upgrade", "repair"].into_iter().find(|purpose| turn.class.iter().any(|class| class == purpose)).unwrap_or_default()
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
                    purpose: purpose(turn).to_string(),
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
            Some("confirm") => Page::Confirm {
                title,
                summary: text(turn, SUMMARY),
                disk: element(turn, SUMMARY).and_then(|e| e.state.get("detail")).and_then(Disk::said),
                back: action("nav.back"),
                begin: action("act.begin"),
            },
            // The three jobs' pages are one page: what differs between them
            // is what installerd puts on it.
            Some(id @ ("install.progress" | "upgrade.progress" | "repair.progress")) => {
                let said = said(turn);
                Page::Progress {
                    title,
                    job: id.trim_end_matches(".progress").to_string(),
                    summary: text(turn, DOING),
                    disk: element(turn, DOING).and_then(|e| e.state.get("detail")).and_then(Disk::said),
                    phases: turn.elements.iter().filter(|e| e.r#type == types::PROGRESS).map(Phase::of).collect(),
                    lines: said[said.len().saturating_sub(TAIL)..].iter().map(|line| line.as_str().unwrap_or_default().to_string()).collect(),
                    said: said.len(),
                    ended: None,
                }
            }
            id => Page::Unbuilt { id: id.unwrap_or_default().to_string(), title },
        }
    }

    /// What is shown once the conversation has ended with `end`, `shown`
    /// being the page it ended on. A job's page stays, and says how the job
    /// ended; any other gives way to the ending.
    pub fn ended(shown: Option<Page>, end: &End) -> Page {
        let ended = Ended {
            outcome: match end.outcome {
                Outcome::Complete => "complete",
                Outcome::Cancelled => "cancelled",
                Outcome::Failed => "failed",
            },
            message: end.message.clone().unwrap_or_default(),
        };
        match shown {
            Some(Page::Progress { title, job, summary, disk, phases, lines, said, .. }) => {
                Page::Progress { title, job, summary, disk, phases, lines, said, ended: Some(ended) }
            }
            _ => Page::Ended { outcome: ended.outcome, message: ended.message },
        }
    }
}

impl Phase {
    fn of(element: &Element) -> Phase {
        let number = |key: &str| element.state.get(key).and_then(Value::as_f64);
        Phase {
            r#ref: element.r#ref.clone(),
            name: element.name.clone().unwrap_or_default(),
            value: number("value").unwrap_or(0.0),
            max: number("max"),
        }
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
            destructive: element.state.get("destructive").and_then(Value::as_bool).unwrap_or(false),
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

    /// The disk the confirmation is about, from what installerd says of it
    /// with the sentence: what a row of the disk page says, and the same
    /// `detail`. Nothing unless it at least says which disk.
    fn said(detail: &Value) -> Option<Disk> {
        let said = |key: &str| detail.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let device = said("device");
        (!device.is_empty()).then(|| Disk {
            value: device.clone(),
            device,
            model: said("model"),
            size: said("size"),
            bus: said("bus"),
            note: String::new(),
            enabled: true,
            detail: detail.clone(),
        })
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
        assert!(next.primary && next.enabled && !next.destructive);
        // An install goes on to the confirmation, which is drawn.
        assert!(!next.unbuilt);
    }

    /// installerd's confirmation of an install, with the disk it is about.
    fn confirm() -> Turn {
        serde_json::from_value(json!({
            "seq": 3, "id": "confirm", "name": "Ready to install", "class": ["confirm"],
            "elements": [
                { "ref": "confirm.summary", "type": "text",
                  "text": "Peios will be installed onto CT500MX500SSD1, 465.8 GiB (/dev/sda). The whole disk will be erased.",
                  "detail": { "device": "/dev/sda", "model": "CT500MX500SSD1", "size": "465.8 GiB", "bus": "SATA",
                              "bytes": 500107862016u64, "partitions": [], "erases": [] } },
                { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
                { "ref": "act.begin", "type": "action", "name": "Erase disk and install", "primary": true, "destructive": true },
            ],
        }))
        .unwrap()
    }

    #[test]
    fn the_confirmation_is_drawn_from_what_installerd_sends() {
        let Page::Confirm { title, summary, disk, back, begin } = Page::of(&confirm(), None) else { panic!("not the confirmation") };
        assert_eq!(title, "Ready to install");
        assert!(summary.starts_with("Peios will be installed onto CT500MX500SSD1"));
        let disk = disk.unwrap();
        assert_eq!(
            (disk.value.as_str(), disk.device.as_str(), disk.model.as_str(), disk.size.as_str(), disk.bus.as_str()),
            ("/dev/sda", "/dev/sda", "CT500MX500SSD1", "465.8 GiB", "SATA")
        );
        assert_eq!(disk.detail["bytes"], 500107862016u64);
        assert_eq!(back.unwrap().r#ref, "nav.back");
        let begin = begin.unwrap();
        assert_eq!(begin.name, "Erase disk and install");
        assert!(begin.primary && begin.destructive && begin.enabled);
        // It leads to the installation as it runs, which is drawn.
        assert!(!begin.unbuilt);
        // Nothing of it is answered with but the action.
        assert!(values(&confirm(), Some("/dev/sda")).is_empty());
        assert_eq!(serde_json::to_value(Page::of(&confirm(), None)).unwrap()["kind"], "confirm");
    }

    #[test]
    fn a_confirmation_that_says_it_only_in_words_is_drawn_without_the_disk() {
        let mut turn = confirm();
        turn.elements[0].state.remove("detail");
        assert!(matches!(Page::of(&turn, None), Page::Confirm { disk: None, summary, .. } if !summary.is_empty()));
        // Nor is a disk made of facts that do not say which disk.
        turn.elements[0].state.insert("detail".into(), json!({ "bytes": 1 }));
        assert!(matches!(Page::of(&turn, None), Page::Confirm { disk: None, .. }));
    }

    #[test]
    fn a_page_come_back_to_arrives_with_its_disk_chosen() {
        let mut turn = disks();
        assert_eq!(assumed(&turn), None);
        turn.elements[1].default = Some(json!("/dev/sda"));
        assert_eq!(assumed(&turn).as_deref(), Some("/dev/sda"));
        // Only a disk that is there to be chosen.
        turn.elements[1].default = Some(json!("/dev/sdb"));
        assert_eq!(assumed(&turn), None);
        turn.elements[1].default = Some(json!("/dev/nvme0n1"));
        assert_eq!(assumed(&turn), None);
        assert_eq!(assumed(&mode()), None);
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
        // What an upgrade or a repair goes on to from the disks is not drawn
        // yet, nor what a disk page that does not say what it is for goes
        // on to.
        let mut turn = disks();
        assert!(!unbuilt(&turn, "nav.next"));
        for purpose in ["upgrade", "repair"] {
            turn.class = vec![purpose.into()];
            assert!(unbuilt(&turn, "nav.next"));
        }
        turn.class.clear();
        assert!(unbuilt(&turn, "nav.next"));
        assert!(!unbuilt(&turn, "nav.back"));
        assert!(!unbuilt(&turn, "act.rescan"));
        assert!(!unbuilt(&mode(), "act.install"));
        // The installation as it runs is drawn, so its button is sent on.
        assert!(!unbuilt(&confirm(), "act.begin"));
        assert!(!unbuilt(&confirm(), "nav.back"));
    }

    /// installerd's page for an install under way: the copy part done, and
    /// three hundred lines said.
    fn installing() -> Turn {
        serde_json::from_value(json!({
            "seq": 4, "id": "install.progress", "name": "Installing", "class": ["progress"],
            "elements": [
                { "ref": "progress.summary", "type": "text",
                  "text": "Installing Peios onto CT500MX500SSD1, 465.8 GiB (/dev/sda). Leave the machine on.",
                  "detail": { "device": "/dev/sda", "model": "CT500MX500SSD1", "size": "465.8 GiB", "bus": "SATA",
                              "bytes": 500107862016u64, "partitions": [], "becomes": [], "erases": [] } },
                { "ref": "phase.partition", "type": "progress", "name": "Partitioning", "value": 100, "max": 100 },
                { "ref": "phase.format", "type": "progress", "name": "Formatting", "value": 100, "max": 100 },
                { "ref": "phase.copy", "type": "progress", "name": "Copying the system", "value": 36, "max": 100 },
                { "ref": "phase.boot", "type": "progress", "name": "Setting up boot", "value": 0 },
                { "ref": "out", "type": "log", "name": "Details",
                  "lines": (1..=300).map(|n| format!("line {n}")).collect::<Vec<_>>() },
            ],
        }))
        .unwrap()
    }

    #[test]
    fn a_job_under_way_is_drawn_from_what_installerd_sends() {
        let Page::Progress { title, job, summary, disk, phases, lines, said, ended } = Page::of(&installing(), None) else {
            panic!("not a job's page")
        };
        assert_eq!((title.as_str(), job.as_str()), ("Installing", "install"));
        assert!(summary.starts_with("Installing Peios onto CT500MX500SSD1"));
        assert_eq!(disk.unwrap().device, "/dev/sda");
        assert_eq!(phases.iter().map(|p| p.r#ref.as_str()).collect::<Vec<_>>(), ["phase.partition", "phase.format", "phase.copy", "phase.boot"]);
        assert_eq!(phases[2], Phase { r#ref: "phase.copy".into(), name: "Copying the system".into(), value: 36.0, max: Some(100.0) });
        // One that does not say how far there is to go.
        assert_eq!(phases[3].max, None);
        // A page is sent the last of what the job said, and how much there
        // was; the whole of it is kept to be served as a file.
        assert_eq!(said, 300);
        assert_eq!(lines.len(), 200);
        assert_eq!((lines[0].as_str(), lines[199].as_str()), ("line 101", "line 300"));
        assert_eq!(super::said(&installing()).len(), 300);
        assert!(super::said(&mode()).is_empty());
        assert_eq!(ended, None);

        // The upgrade's and the repair's are the same page, of another job,
        // and an installerd that says it only in words is drawn without the
        // disk.
        let mut turn = installing();
        turn.id = Some("upgrade.progress".into());
        turn.elements[0].state.remove("detail");
        assert!(matches!(Page::of(&turn, None), Page::Progress { job, disk: None, .. } if job == "upgrade"));
        turn.id = Some("repair.progress".into());
        assert!(matches!(Page::of(&turn, None), Page::Progress { job, .. } if job == "repair"));

        let sent = serde_json::to_value(Page::of(&installing(), None)).unwrap();
        assert_eq!(sent["kind"], "progress");
        assert_eq!(sent["phases"][2], json!({ "ref": "phase.copy", "name": "Copying the system", "value": 36.0, "max": 100.0 }));
        assert_eq!(sent["ended"], Value::Null);
    }

    #[test]
    fn a_page_not_drawn_here_is_said_to_be_one() {
        let mut turn = mode();
        turn.id = Some("upgrade.confirm".into());
        turn.name = Some("Ready to upgrade".into());
        assert_eq!(Page::of(&turn, None), Page::Unbuilt { id: "upgrade.confirm".into(), title: "Ready to upgrade".into() });
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
            "ref": "act.install", "name": "Install Peios", "primary": true, "enabled": true, "help": null,
            "destructive": false, "unbuilt": false,
        }));
        assert_eq!(sent["trail"], json!([["connect", "/run/installerd.sock"]]));
        assert_eq!(sent["release"], json!({ "version": "2026.8", "variant": "Experimental" }));

        let sent = serde_json::to_value(Page::of(&disks(), Some("/dev/sda"))).unwrap();
        assert_eq!(sent["kind"], "disk");
        assert_eq!(sent["chosen"], "/dev/sda");
        assert_eq!(sent["disks"][0]["detail"]["bytes"], 500107862016u64);
        assert_eq!(sent["next"]["unbuilt"], false);
    }

    #[test]
    fn an_ending_says_how_it_ended() {
        let end = End { seq: 9, outcome: Outcome::Failed, message: Some("cp failed (exit 1)".into()) };
        let ending = Page::Ended { outcome: "failed", message: "cp failed (exit 1)".into() };
        assert_eq!(Page::ended(None, &end), ending);
        assert_eq!(Page::ended(Some(Page::of(&mode(), None)), &end), ending);
    }

    #[test]
    fn a_job_that_has_ended_stays_on_its_page_and_says_how() {
        let end = End { seq: 9, outcome: Outcome::Failed, message: Some("copying /boot: cp failed (exit 1)".into()) };
        let Page::Progress { title, phases, said, ended, .. } = Page::ended(Some(Page::of(&installing(), None)), &end) else {
            panic!("the job's page gave way to the ending")
        };
        // Everything on it is as it was when the job stopped.
        assert_eq!((title.as_str(), phases[2].value, said), ("Installing", 36.0, 300));
        assert_eq!(ended, Some(Ended { outcome: "failed", message: "copying /boot: cp failed (exit 1)".into() }));
        let done = End { seq: 9, outcome: Outcome::Complete, message: None };
        let sent = serde_json::to_value(Page::ended(Some(Page::of(&installing(), None)), &done)).unwrap();
        assert_eq!(sent["kind"], "progress");
        assert_eq!(sent["ended"], json!({ "outcome": "complete", "message": "" }));
    }
}
