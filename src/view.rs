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

use crate::Conversation;
use crate::release::Release;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    /// Whose conversation this is: installerd's (`install`) or oobed's
    /// (`oobe`). The page is the same page either way, and says what
    /// is being done in the words of the one it is.
    pub conversation: Conversation,
    /// Which Peios the medium carries, or the machine runs.
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
    /// installerd has taken the request to restart the machine, and this
    /// process is about to go down with it. A page is a browser elsewhere,
    /// and outlives it: this is its cue to wait for what comes back.
    pub restarting: bool,
    /// Which boot of the machine this is (`/proc/sys/kernel/random/boot_id`),
    /// so that a page waiting on a restart can tell this installer from the
    /// same one on the next boot.
    pub boot: String,
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
        /// How it ended, once it has, and the way on from there. The page
        /// stays to say so, whether the job's end was the conversation's
        /// or installerd went on to offer the restart.
        ended: Option<Ended>,
    },
    /// oobed's `oobe.locale`: the first page of first-boot setup, which
    /// welcomes and would ask for a language and a keyboard.
    Welcome {
        title: String,
        intro: String,
        /// The language, which oobed shows and cannot yet let be chosen.
        /// The keyboard is not sent: in a browser it is the browser's own,
        /// and what oobed says of the console's is nothing to do with it.
        language: Option<Choice>,
        next: Option<Action>,
    },
    /// oobed's `oobe.network`: what the machine's network is. Nothing on it
    /// has to be answered.
    Network {
        title: String,
        /// What oobed says of the network in words: the machine, then a line
        /// for each interface.
        status: String,
        /// Each interface, as oobed read it for this page (`detail`), passed
        /// on as it came for the page to draw. Nothing from an oobed that
        /// says it only in words, or that could not ask netd.
        interfaces: Option<Vec<Value>>,
        note: String,
        /// The address given to an interface by hand, which oobed keeps
        /// until the end of setup, if one is: its words, and as oobed read
        /// it (`detail`: interface, address, gateway, name servers).
        planned: Option<Planned>,
        refresh: Option<Action>,
        /// Giving an interface an address by hand, or changing the one
        /// given.
        manual: Option<Action>,
        /// Going back to the network's address, where one was given.
        unplan: Option<Action>,
        /// What else oobed offers here, in its order: today, joining a
        /// wireless network, which it cannot do yet.
        others: Vec<Action>,
        back: Option<Action>,
        next: Option<Action>,
    },
    /// oobed's `oobe.network.manual`: an address for one interface, given by
    /// hand, which oobed applies at the end of setup.
    Manual {
        title: String,
        intro: String,
        /// The interfaces, each as a row of what it is now.
        interfaces: Vec<Row>,
        /// What oobed says in place of the interfaces when there are none.
        empty: String,
        /// The interface oobed starts with chosen: the one given an address
        /// before, or the only one there is to choose.
        assumed: Option<String>,
        /// Why oobed turned down the interface answered with.
        error: Option<String>,
        /// The address, the gateway and the name servers.
        fields: Vec<Field>,
        back: Option<Action>,
        save: Option<Action>,
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

/// How a job ended: `complete`, `cancelled` or `failed`, and what installerd
/// had to say of it. A job that failed ends the conversation with it; one
/// that finished goes on to a page of installerd's that offers the restart.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ended {
    pub outcome: &'static str,
    pub message: String,
    /// Restarting the machine, where installerd offers it.
    pub reboot: Option<Action>,
    /// Back to the first page, in the same conversation, where installerd
    /// offers it. Without it the way back is another conversation.
    pub start: Option<Action>,
    /// Why installerd could not restart the machine, when it could not.
    pub error: Option<String>,
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

/// A choice among several, as oobed asks for one (MSIP `select`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choice {
    pub r#ref: String,
    pub name: String,
    /// What there is to choose from, each as its value and its name.
    pub choices: Vec<(String, String)>,
    pub enabled: bool,
    /// Why it cannot be chosen, where it cannot.
    pub help: Option<String>,
}

/// What oobed keeps to apply at the end of setup, in words and as it read it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Planned {
    pub words: String,
    pub detail: Value,
}

/// One row of a table that is not disks: what choosing it answers with,
/// its cells by column, and whether it can be chosen, and if not, why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub value: String,
    pub cells: Value,
    pub enabled: bool,
    pub note: String,
}

/// A line of text to fill in (MSIP `string`), as oobed asks for it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    pub r#ref: String,
    pub name: String,
    pub help: Option<String>,
    /// Ghost text: an example, not a value.
    pub placeholder: Option<String>,
    /// What it starts with filled in.
    pub default: Option<String>,
    pub required: bool,
    /// Why oobed turned down what it was answered with.
    pub error: Option<String>,
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
/// What oobed says of the network, which carries the interfaces.
const NETWORK: &str = "network.status";
/// What oobed says of the address kept for the end of setup.
const PLANNED: &str = "network.planned";
/// The table of interfaces on oobed's manual page.
const INTERFACES: &str = "manual.interface";

impl View {
    pub fn starting(conversation: Conversation, release: Release, boot: String) -> View {
        View {
            conversation,
            release,
            link: Link::Connecting,
            trail: Vec::new(),
            seq: 0,
            waiting: None,
            page: None,
            restarting: false,
            boot,
        }
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
        // First-boot setup's account page, and what joining a wireless
        // network would lead to once oobed can do it.
        (Some("oobe.network"), "nav.next" | "network.wifi") => true,
        _ => false,
    }
}

/// Whether `turn` is the page a finished job goes on to: `install.done`,
/// and the upgrade's and the repair's.
pub fn is_done(turn: &Turn) -> bool {
    matches!(turn.id.as_deref(), Some("install.done" | "upgrade.done" | "repair.done"))
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

/// Of what a browser says it filled in on `turn`, what is answered: text
/// for a line of `turn`'s that is to be filled in, and a row `turn`'s table
/// offers and lets be chosen. The disks are not among them, being chosen
/// for everyone (`values`); nor is anything else, whatever it is called.
pub fn filled(turn: &Turn, said: serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    said.into_iter()
        .filter(|(r#ref, value)| {
            let Some(e) = element(turn, r#ref).filter(|e| e.enabled && r#ref != DISKS) else { return false };
            let Some(text) = value.as_str() else { return false };
            match e.r#type.as_str() {
                types::STRING => true,
                types::TABLE => table_rows(e).iter().any(|row| row.get("value").and_then(Value::as_str) == Some(text) && enabled(row)),
                _ => false,
            }
        })
        .collect()
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
            Some("oobe.locale") => Page::Welcome {
                title,
                intro: text(turn, "locale.intro"),
                language: element(turn, "locale.language").filter(|e| e.r#type == types::SELECT).map(Choice::of),
                next: action("nav.next"),
            },
            Some("oobe.network") => {
                const OWN: &[&str] = &["network.refresh", "network.static", "network.unplan", "nav.back", "nav.next"];
                Page::Network {
                    title,
                    status: text(turn, NETWORK),
                    interfaces: element(turn, NETWORK)
                        .and_then(|e| e.state.get("detail"))
                        .and_then(|detail| detail.get("interfaces"))
                        .and_then(Value::as_array)
                        .cloned(),
                    note: text(turn, "network.note"),
                    planned: element(turn, PLANNED).map(|e| Planned {
                        words: text(turn, PLANNED),
                        detail: e.state.get("detail").cloned().unwrap_or(Value::Null),
                    }),
                    refresh: action("network.refresh"),
                    manual: action("network.static"),
                    unplan: action("network.unplan"),
                    others: turn.elements.iter().filter(|e| e.is_action() && !OWN.contains(&e.r#ref.as_str())).map(|e| Action::of(turn, e)).collect(),
                    back: action("nav.back"),
                    next: action("nav.next"),
                }
            }
            Some("oobe.network.manual") => {
                let table = element(turn, INTERFACES);
                let said = |value: Option<&Value>| value.and_then(Value::as_str).unwrap_or_default().to_string();
                Page::Manual {
                    title,
                    intro: text(turn, "manual.intro"),
                    interfaces: table
                        .map(table_rows)
                        .unwrap_or_default()
                        .iter()
                        .map(|row| Row {
                            value: said(row.get("value")),
                            cells: row.get("cells").cloned().unwrap_or(Value::Null),
                            enabled: enabled(row),
                            note: said(row.get("note")),
                        })
                        .collect(),
                    empty: said(table.and_then(|t| t.state.get("empty"))),
                    assumed: table.and_then(|t| t.default.as_ref()).and_then(Value::as_str).map(str::to_string),
                    error: table.and_then(|t| t.error.clone()),
                    fields: turn.elements.iter().filter(|e| e.r#type == types::STRING).map(Field::of).collect(),
                    back: action("nav.back"),
                    save: action("manual.save"),
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
            reboot: None,
            start: None,
            error: None,
        };
        match shown {
            Some(Page::Progress { title, job, summary, disk, phases, lines, said, .. }) => {
                Page::Progress { title, job, summary, disk, phases, lines, said, ended: Some(ended) }
            }
            _ => Page::Ended { outcome: ended.outcome, message: ended.message },
        }
    }

    /// What is shown for installerd's page after a job that finished
    /// (`is_done`), `shown` being what was shown before it. It is drawn as
    /// the job's page, finished, so that the phases and what the job said
    /// stay: what the page adds is what the job came to and the way on.
    ///
    /// A process that joins on this page never saw the job's, and draws it
    /// with no phases and nothing said.
    pub fn done(shown: Option<Page>, turn: &Turn) -> Page {
        let action = |r#ref: &str| element(turn, r#ref).filter(|e| e.is_action()).map(|e| Action::of(turn, e));
        let ended = Ended {
            outcome: "complete",
            message: text(turn, "done.summary"),
            reboot: action("act.reboot"),
            start: action("nav.start"),
            error: element(turn, "act.reboot").and_then(|e| e.error.clone()),
        };
        match shown {
            Some(Page::Progress { title, job, summary, disk, phases, lines, said, .. }) => {
                Page::Progress { title, job, summary, disk, phases, lines, said, ended: Some(ended) }
            }
            _ => Page::Progress {
                title: turn.name.clone().unwrap_or_default(),
                job: turn.id.as_deref().unwrap_or_default().trim_end_matches(".done").to_string(),
                summary: String::new(),
                disk: None,
                phases: Vec::new(),
                lines: Vec::new(),
                said: 0,
                ended: Some(ended),
            },
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

impl Choice {
    /// A `select`'s choices are each a value and a name (PGSS §3.B); one
    /// without both is not a choice.
    fn of(element: &Element) -> Choice {
        let choices = element.state.get("choices").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        let said = |choice: &Value, key: &str| choice.get(key).and_then(Value::as_str).map(str::to_string);
        Choice {
            r#ref: element.r#ref.clone(),
            name: element.name.clone().unwrap_or_default(),
            choices: choices.iter().filter_map(|choice| Some((said(choice, "value")?, said(choice, "name")?))).collect(),
            enabled: element.enabled,
            help: element.help.clone(),
        }
    }
}

impl Field {
    fn of(element: &Element) -> Field {
        let said = |key: &str| element.state.get(key).and_then(Value::as_str).map(str::to_string);
        Field {
            r#ref: element.r#ref.clone(),
            name: element.name.clone().unwrap_or_default(),
            help: element.help.clone(),
            placeholder: said("placeholder"),
            default: element.default.as_ref().and_then(Value::as_str).map(str::to_string),
            required: element.required,
            error: element.error.clone(),
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
    element(turn, DISKS).map(table_rows).unwrap_or_default()
}

/// The rows of a table.
fn table_rows(e: &Element) -> &[Value] {
    e.state.get("rows").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
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
            conversation: Conversation::Install,
            release: Release { version: "2026.8".into(), variant: "Experimental".into() },
            link: Link::Connected { daemon: "installerd/0.1.10".into() },
            trail: vec![("connect".into(), "/run/installerd.sock".into())],
            seq: 1,
            waiting: Some("act.install".into()),
            page: Some(Page::of(&mode(), None)),
            restarting: false,
            boot: "8c7c".into(),
        };
        let sent = serde_json::to_value(&view).unwrap();
        assert_eq!(sent["conversation"], "install");
        assert_eq!(serde_json::to_value(Conversation::Oobe).unwrap(), "oobe");
        assert_eq!(sent["link"], json!({ "state": "connected", "daemon": "installerd/0.1.10" }));
        assert_eq!(sent["seq"], 1);
        assert_eq!(sent["waiting"], "act.install");
        assert_eq!(sent["page"]["kind"], "mode");
        assert_eq!(sent["page"]["actions"][0], json!({
            "ref": "act.install", "name": "Install Peios", "primary": true, "enabled": true, "help": null,
            "destructive": false, "unbuilt": false,
        }));
        assert_eq!(sent["trail"], json!([["connect", "/run/installerd.sock"]]));
        assert_eq!((sent["restarting"].clone(), sent["boot"].clone()), (json!(false), json!("8c7c")));
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
        assert_eq!(
            ended,
            Some(Ended {
                outcome: "failed",
                message: "copying /boot: cp failed (exit 1)".into(),
                reboot: None,
                start: None,
                error: None,
            })
        );
        let done = End { seq: 9, outcome: Outcome::Complete, message: None };
        let sent = serde_json::to_value(Page::ended(Some(Page::of(&installing(), None)), &done)).unwrap();
        assert_eq!(sent["kind"], "progress");
        assert_eq!(sent["ended"], json!({ "outcome": "complete", "message": "", "reboot": null, "start": null, "error": null }));
    }

    /// oobed's first page, as it sends it (`oobed/src/flow.rs`).
    fn welcome() -> Turn {
        serde_json::from_value(json!({
            "seq": 1, "id": "oobe.locale", "name": "Welcome to Peios",
            "elements": [
                { "ref": "locale.intro", "type": "text", "text": "A few questions and this machine is ready to use." },
                { "ref": "locale.language", "type": "select", "name": "Language", "choices": [], "enabled": false,
                  "help": "Peios ships in English only for now; there is no locale data to choose from yet." },
                { "ref": "locale.keyboard", "type": "select", "name": "Keyboard layout", "choices": [], "enabled": false,
                  "help": "No keymaps are packaged yet." },
                { "ref": "nav.next", "type": "action", "name": "Next", "primary": true },
            ],
        }))
        .unwrap()
    }

    #[test]
    fn first_boot_setup_is_welcomed_from_what_oobed_sends() {
        let Page::Welcome { title, intro, language, next } = Page::of(&welcome(), None) else { panic!("not the welcome") };
        assert_eq!((title.as_str(), intro.as_str()), ("Welcome to Peios", "A few questions and this machine is ready to use."));
        let language = language.unwrap();
        assert_eq!((language.r#ref.as_str(), language.name.as_str(), language.enabled), ("locale.language", "Language", false));
        assert!(language.choices.is_empty());
        assert!(language.help.unwrap().starts_with("Peios ships in English only"));
        let next = next.unwrap();
        assert!(next.primary && next.enabled);
        // The network page it leads to is drawn.
        assert!(!next.unbuilt);
        // The keyboard is the browser's, and is not sent.
        let sent = serde_json::to_value(Page::of(&welcome(), None)).unwrap();
        assert_eq!(sent["kind"], "welcome");
        assert!(!sent.to_string().contains("keyboard"));
    }

    #[test]
    fn a_choice_is_a_value_and_a_name() {
        let mut turn = welcome();
        turn.elements[1].state.insert(
            "choices".into(),
            json!([{ "value": "en-GB", "name": "English (United Kingdom)" }, { "value": "de" }, "fr", { "value": "nl", "name": "Nederlands" }]),
        );
        let Page::Welcome { language: Some(language), .. } = Page::of(&turn, None) else { panic!("no language") };
        assert_eq!(language.choices, [("en-GB".into(), "English (United Kingdom)".into()), ("nl".into(), "Nederlands".into())]);
    }

    /// oobed's network page, as it sends it (`oobed/src/flow.rs`), with one
    /// interface connected.
    fn network() -> Turn {
        serde_json::from_value(json!({
            "seq": 2, "id": "oobe.network", "name": "Network",
            "elements": [
                { "ref": "network.status", "type": "text",
                  "text": "This machine is connected to a network.\neth0: connected, as 10.0.2.15/24, through 10.0.2.2.",
                  "detail": { "readiness": "routed", "interfaces": [
                      { "name": "eth0", "state": "connected", "addresses": ["10.0.2.15/24"], "dns": ["10.0.2.3"], "gateway": "10.0.2.2" },
                  ] } },
                { "ref": "network.note", "type": "text", "text": "Setup does not need a network, and nothing here has to be answered." },
                { "ref": "network.refresh", "type": "action", "name": "Check again", "validate": false },
                { "ref": "network.wifi", "type": "action", "name": "Connect to Wi-Fi…", "enabled": false,
                  "help": "No wireless stack is packaged yet." },
                { "ref": "network.static", "type": "action", "name": "Configure manually…", "validate": false },
                { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
                { "ref": "nav.next", "type": "action", "name": "Next", "primary": true },
            ],
        }))
        .unwrap()
    }

    #[test]
    fn the_network_is_drawn_from_what_oobed_sends() {
        let Page::Network { title, status, interfaces, note, planned, refresh, manual, unplan, others, back, next } = Page::of(&network(), None) else {
            panic!("not the network page")
        };
        assert_eq!(title, "Network");
        assert!(status.starts_with("This machine is connected to a network.\n"));
        assert!(note.starts_with("Setup does not need a network"));
        let interfaces = interfaces.unwrap();
        assert_eq!((interfaces.len(), &interfaces[0]["state"]), (1, &json!("connected")));
        // Checking again is answered: it changes the page, and leads nowhere.
        assert!(!refresh.unwrap().unbuilt);
        assert!(!back.unwrap().unbuilt);
        // Addressing by hand leads to a page that is drawn.
        let manual = manual.unwrap();
        assert!(manual.enabled && !manual.unbuilt);
        assert_eq!((planned, unplan), (None, None));
        let others: Vec<_> = others.iter().map(|a| (a.r#ref.as_str(), a.enabled, a.unbuilt)).collect();
        assert_eq!(others, [("network.wifi", false, true)]);
        // The account page it leads to is not drawn yet.
        assert!(next.unwrap().unbuilt);

        // With an address kept for the end, it says so, and it can be given up.
        let mut kept = network();
        kept.elements.insert(1, serde_json::from_value(json!({
            "ref": "network.planned", "type": "text", "text": "At the end of setup, eth0 is given 10.0.0.5/24.",
            "detail": { "interface": "eth0", "address": "10.0.0.5/24", "dns": [] },
        })).unwrap());
        kept.elements.insert(6, serde_json::from_value(json!({
            "ref": "network.unplan", "type": "action", "name": "Use the network's address instead", "validate": false,
        })).unwrap());
        let Page::Network { planned, unplan, others, .. } = Page::of(&kept, None) else { panic!("not the network page") };
        let planned = planned.unwrap();
        assert_eq!((planned.words.as_str(), &planned.detail["address"]), ("At the end of setup, eth0 is given 10.0.0.5/24.", &json!("10.0.0.5/24")));
        assert!(unplan.is_some_and(|a| a.enabled && !a.unbuilt));
        assert_eq!(others.len(), 1);

        // An oobed that says it only in words, or could not ask netd.
        let mut words = network();
        words.elements[0].state.remove("detail");
        let Page::Network { interfaces, .. } = Page::of(&words, None) else { panic!("not the network page") };
        assert_eq!(interfaces, None);
    }

    /// oobed's manual page, as it sends it, with the address turned down.
    fn manual() -> Turn {
        serde_json::from_value(json!({
            "seq": 3, "id": "oobe.network.manual", "name": "Configure manually",
            "elements": [
                { "ref": "manual.intro", "type": "text", "text": "It is applied at the end of setup." },
                { "ref": "manual.interface", "type": "table", "name": "Interface", "required": true, "default": "eth0",
                  "columns": [{ "key": "name", "name": "Interface" }, { "key": "state", "name": "Status" }, { "key": "address", "name": "Address now" }],
                  "rows": [
                      { "value": "eth0", "cells": { "name": "eth0", "state": "connected", "address": "10.0.2.15/24" } },
                      { "value": "wlan0", "cells": { "name": "wlan0", "state": "not used", "address": "" }, "enabled": false, "note": "not wired" },
                  ],
                  "empty": "This machine has no network hardware that Peios can use." },
                { "ref": "manual.address", "type": "string", "name": "Address", "required": true, "placeholder": "192.168.1.20/24",
                  "help": "With the length of its network after a slash.", "error": "An address and the length of its network, as 192.168.1.20/24." },
                { "ref": "manual.gateway", "type": "string", "name": "Gateway", "placeholder": "192.168.1.1" },
                { "ref": "manual.dns", "type": "string", "name": "Name servers", "default": "1.1.1.1" },
                { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
                { "ref": "manual.save", "type": "action", "name": "Save", "primary": true },
            ],
        }))
        .unwrap()
    }

    #[test]
    fn an_address_by_hand_is_asked_for_as_oobed_asks() {
        let Page::Manual { title, intro, interfaces, empty, assumed, error, fields, back, save } = Page::of(&manual(), None) else {
            panic!("not the manual page")
        };
        assert_eq!((title.as_str(), intro.as_str()), ("Configure manually", "It is applied at the end of setup."));
        let rows: Vec<_> = interfaces.iter().map(|r| (r.value.as_str(), r.enabled, r.note.as_str())).collect();
        assert_eq!(rows, [("eth0", true, ""), ("wlan0", false, "not wired")]);
        assert_eq!(interfaces[0].cells["address"], "10.0.2.15/24");
        assert!(empty.starts_with("This machine has no network hardware"));
        assert_eq!((assumed.as_deref(), error), (Some("eth0"), None));
        let fields: Vec<_> = fields.iter().map(|f| (f.r#ref.as_str(), f.required, f.default.as_deref(), f.error.is_some())).collect();
        assert_eq!(fields, [("manual.address", true, None, true), ("manual.gateway", false, None, false), ("manual.dns", false, Some("1.1.1.1"), false)]);
        assert!(back.is_some() && save.is_some_and(|s| s.primary && !s.unbuilt));
    }

    #[test]
    fn only_what_the_page_asks_for_is_answered() {
        let said = json!({
            "manual.interface": "eth0", "manual.address": "10.0.0.5/24",
            // Not a row that can be chosen, not text, not the page's, not asked.
            "manual.gateway": 7, "manual.intro": "x", "act.begin": "x", "disk.target": "/dev/sda",
        });
        let filled = filled(&manual(), said.as_object().unwrap().clone());
        assert_eq!(Value::Object(filled), json!({ "manual.interface": "eth0", "manual.address": "10.0.0.5/24" }));
        let wlan = json!({ "manual.interface": "wlan0" });
        assert!(super::filled(&manual(), wlan.as_object().unwrap().clone()).is_empty(), "a row that cannot be chosen");
    }

    /// installerd's page after an install that finished
    /// (`installerd/src/flow.rs`, `done_page`).
    fn finished() -> Turn {
        serde_json::from_value(json!({
            "seq": 5, "id": "install.done", "name": "Installation complete", "class": ["done"],
            "elements": [
                { "ref": "done.summary", "type": "text", "text": "Installation complete. Reboot to start Peios." },
                { "ref": "act.reboot", "type": "action", "name": "Reboot now", "primary": true },
                { "ref": "nav.start", "type": "action", "name": "Back to the start", "validate": false },
            ],
        }))
        .unwrap()
    }

    #[test]
    fn a_job_that_finished_stays_on_its_page_with_the_way_on() {
        assert!(is_done(&finished()));
        assert!(!is_done(&installing()));
        let Page::Progress { title, phases, said, ended, .. } = Page::done(Some(Page::of(&installing(), None)), &finished()) else {
            panic!("the job's page gave way")
        };
        // Everything on it is as the job left it.
        assert_eq!((title.as_str(), phases.len(), said), ("Installing", 4, 300));
        let ended = ended.unwrap();
        assert_eq!((ended.outcome, ended.message.as_str()), ("complete", "Installation complete. Reboot to start Peios."));
        let reboot = ended.reboot.unwrap();
        assert_eq!((reboot.r#ref.as_str(), reboot.name.as_str(), reboot.primary), ("act.reboot", "Reboot now", true));
        assert_eq!(ended.start.unwrap().r#ref, "nav.start");
        assert_eq!(ended.error, None);

        // A restart installerd could not make is said with it.
        let mut refused = finished();
        refused.elements[1].error = Some("peinit did not take the request".into());
        let again = Page::done(Some(Page::done(Some(Page::of(&installing(), None)), &finished())), &refused);
        let Page::Progress { said, ended, .. } = again else { panic!("not the job's page") };
        assert_eq!(said, 300);
        assert_eq!(ended.unwrap().error.as_deref(), Some("peinit did not take the request"));

        // Joined on this page, with nothing seen before it.
        let Page::Progress { title, job, phases, ended, .. } = Page::done(None, &finished()) else { panic!("not the job's page") };
        assert_eq!((title.as_str(), job.as_str(), phases.len()), ("Installation complete", "install", 0));
        assert!(ended.unwrap().reboot.is_some());
    }
}
