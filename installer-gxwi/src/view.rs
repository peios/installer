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

mod pages;

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
    Lost {
        why: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Page {
    /// installerd's `mode`: what to do with this machine.
    Mode {
        title: String,
        intro: String,
        actions: Vec<Action>,
    },
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
    /// a job as it runs, and how it ended once it has. oobed's
    /// `oobe.applying` is one too, the job being `setup`.
    Progress {
        title: String,
        /// Which job it is: `install`, `upgrade`, `repair` or `setup`.
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
    /// oobed's `oobe.account`: the machine's first account, which
    /// administers it.
    Account {
        title: String,
        intro: String,
        /// The name, the password, and the password again.
        fields: Vec<Field>,
        back: Option<Action>,
        next: Option<Action>,
    },
    /// oobed's `oobe.naming`: the machine's name, the last thing asked.
    Naming {
        title: String,
        /// The name, which oobed offers one for.
        name: Option<Field>,
        /// What else oobed offers here: today, joining a domain, which it
        /// cannot do yet.
        others: Vec<Action>,
        back: Option<Action>,
        finish: Option<Action>,
        /// The account's name, as the account page was answered with it
        /// (trimmed, as oobed takes it), for the page to show the machine as
        /// its prompt will read. oobed does not send it here: this process
        /// heard it answered. Nothing where it did not, as when it came to a
        /// conversation already past the account.
        account: Option<String>,
    },
    /// A page of installerd's that is not drawn here yet.
    Unbuilt { id: String, title: String },
    /// The conversation is over, and not on a page that stays to say so.
    Ended {
        outcome: &'static str,
        message: String,
    },
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
    /// What the job says of the phase beyond its name, passed on as it came,
    /// or nothing. Setup's addressing an interface by hand says the address
    /// it is given, which a page reached through that interface follows the
    /// machine to.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub detail: Value,
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
    /// What it starts with filled in. Never anything for a secret.
    pub default: Option<String>,
    pub required: bool,
    /// What is typed is not to be shown: a password.
    pub secret: bool,
    /// The most characters it takes, where oobed says.
    pub max: Option<u64>,
    /// What it must look like, where oobed says: an anchored regular
    /// expression, for a page to test what is typed against as it is typed.
    /// oobed checks it again, and has the last word.
    pub pattern: Option<String>,
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
const LOG_TAIL_LINES: usize = 200;
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
        // What joining a wireless network or a domain would lead to once
        // oobed can do them, and first-boot setup applying what it has
        // been told.
        (Some("oobe.network"), "network.wifi") => true,
        (Some("oobe.naming"), "domain.join") => true,
        _ => false,
    }
}

/// Whether `turn` is the page a finished job goes on to: `install.done`,
/// and the upgrade's and the repair's.
pub fn is_done(turn: &Turn) -> bool {
    matches!(
        turn.id.as_deref(),
        Some("install.done" | "upgrade.done" | "repair.done")
    )
}

/// Everything the job on `turn` has said of itself so far, a line each, or
/// nothing where `turn` is not a job's page.
pub fn said(turn: &Turn) -> &[Value] {
    element(turn, LOG)
        .and_then(|e| e.state.get("lines"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// Whether `value` is a disk `turn` offers and lets be chosen.
pub fn can_choose(turn: &Turn, value: &str) -> bool {
    rows(turn)
        .iter()
        .any(|row| row.get("value").and_then(Value::as_str) == Some(value) && enabled(row))
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
    ["install", "upgrade", "repair"]
        .into_iter()
        .find(|purpose| turn.class.iter().any(|class| class == purpose))
        .unwrap_or_default()
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
            let Some(e) = element(turn, r#ref).filter(|e| e.enabled && r#ref != DISKS) else {
                return false;
            };
            let Some(text) = value.as_str() else {
                return false;
            };
            match e.r#type.as_str() {
                types::STRING => true,
                types::TABLE => table_rows(e).iter().any(|row| {
                    row.get("value").and_then(Value::as_str) == Some(text) && enabled(row)
                }),
                _ => false,
            }
        })
        .collect()
}

impl Page {
    /// The page, with `named` as the account the naming page says the
    /// machine's prompt is for. Any other page is as it was.
    pub fn for_account(mut self, named: Option<&str>) -> Page {
        if let Page::Naming { account, .. } = &mut self {
            *account = named.map(str::to_string);
        }
        self
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
            Some(Page::Progress {
                title,
                job,
                summary,
                disk,
                phases,
                lines,
                said,
                ..
            }) => Page::Progress {
                title,
                job,
                summary,
                disk,
                phases,
                lines,
                said,
                ended: Some(ended),
            },
            _ => Page::Ended {
                outcome: ended.outcome,
                message: ended.message,
            },
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
        let action = |r#ref: &str| {
            element(turn, r#ref)
                .filter(|e| e.is_action())
                .map(|e| Action::of(turn, e))
        };
        let ended = Ended {
            outcome: "complete",
            message: text(turn, "done.summary"),
            reboot: action("act.reboot"),
            start: action("nav.start"),
            error: element(turn, "act.reboot").and_then(|e| e.error.clone()),
        };
        match shown {
            Some(Page::Progress {
                title,
                job,
                summary,
                disk,
                phases,
                lines,
                said,
                ..
            }) => Page::Progress {
                title,
                job,
                summary,
                disk,
                phases,
                lines,
                said,
                ended: Some(ended),
            },
            _ => Page::Progress {
                title: turn.name.clone().unwrap_or_default(),
                job: turn
                    .id
                    .as_deref()
                    .unwrap_or_default()
                    .trim_end_matches(".done")
                    .to_string(),
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
            detail: element.state.get("detail").cloned().unwrap_or(Value::Null),
        }
    }
}

impl Action {
    fn of(turn: &Turn, element: &Element) -> Action {
        Action {
            r#ref: element.r#ref.clone(),
            name: element.name.clone().unwrap_or_default(),
            primary: element
                .state
                .get("primary")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            enabled: element.enabled,
            help: element.help.clone(),
            destructive: element
                .state
                .get("destructive")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            unbuilt: unbuilt(turn, &element.r#ref),
        }
    }
}

impl Choice {
    /// A `select`'s choices are each a value and a name (PGSS §3.B); one
    /// without both is not a choice.
    fn of(element: &Element) -> Choice {
        let choices = element
            .state
            .get("choices")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let said =
            |choice: &Value, key: &str| choice.get(key).and_then(Value::as_str).map(str::to_string);
        Choice {
            r#ref: element.r#ref.clone(),
            name: element.name.clone().unwrap_or_default(),
            choices: choices
                .iter()
                .filter_map(|choice| Some((said(choice, "value")?, said(choice, "name")?)))
                .collect(),
            enabled: element.enabled,
            help: element.help.clone(),
        }
    }
}

impl Field {
    fn of(element: &Element) -> Field {
        let said = |key: &str| {
            element
                .state
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        Field {
            r#ref: element.r#ref.clone(),
            name: element.name.clone().unwrap_or_default(),
            help: element.help.clone(),
            placeholder: said("placeholder"),
            // Everyone looking is sent the page, and a password is not
            // everyone's, whatever oobed starts it with.
            default: element
                .default
                .as_ref()
                .and_then(Value::as_str)
                .filter(|_| !element.secret)
                .map(str::to_string),
            required: element.required,
            secret: element.secret,
            max: element.state.get("max").and_then(Value::as_u64),
            pattern: said("pattern"),
            error: element.error.clone(),
        }
    }
}

impl Disk {
    fn of(row: &Value) -> Disk {
        let said = |value: Option<&Value>| {
            value
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
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
        let said = |key: &str| {
            detail
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
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
    element(turn, r#ref)
        .and_then(|e| e.state.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The rows of the table of disks, or none where `turn` has no such table.
fn rows(turn: &Turn) -> &[Value] {
    element(turn, DISKS).map(table_rows).unwrap_or_default()
}

/// The rows of a table.
fn table_rows(e: &Element) -> &[Value] {
    e.state
        .get("rows")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// A row can be chosen unless it says otherwise (PGSS §3.B).
fn enabled(row: &Value) -> bool {
    row.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

#[cfg(test)]
mod tests;
