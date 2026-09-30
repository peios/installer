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
}

impl View {
    pub fn starting(release: Release) -> View {
        View { release, link: Link::Connecting, trail: Vec::new(), page: None }
    }

    pub fn did(&mut self, what: &str, to: impl Into<String>) {
        self.trail.push((what.to_string(), to.into()));
    }
}

impl Page {
    /// The page for a turn of installerd's, by the turn's id.
    pub fn of(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        match turn.id.as_deref() {
            Some("mode") => Page::Mode {
                title,
                intro: text(turn, "mode.intro"),
                actions: turn.elements.iter().filter(|e| e.is_action()).map(Action::of).collect(),
            },
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
    fn of(element: &Element) -> Action {
        Action {
            r#ref: element.r#ref.clone(),
            name: element.name.clone().unwrap_or_default(),
            primary: element.state.get("primary").and_then(Value::as_bool).unwrap_or(false),
            enabled: element.enabled,
            help: element.help.clone(),
        }
    }
}

/// What the text element `r#ref` of `turn` says, or nothing.
fn text(turn: &Turn, r#ref: &str) -> String {
    turn.elements
        .iter()
        .find(|e| e.r#ref == r#ref)
        .and_then(|e| e.state.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
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

    #[test]
    fn the_first_page_is_drawn_from_what_installerd_sends() {
        let Page::Mode { title, intro, actions } = Page::of(&mode()) else { panic!("not the mode page") };
        assert_eq!(title, "Peios Setup");
        assert_eq!(intro, "Set up Peios on this machine.");
        assert_eq!(actions.iter().map(|a| a.r#ref.as_str()).collect::<Vec<_>>(), ["act.install", "act.upgrade", "act.repair"]);
        assert_eq!(actions.iter().map(|a| a.primary).collect::<Vec<_>>(), [true, false, false]);
        assert!(actions[0].enabled && actions[0].help.is_none());
        assert!(!actions[2].enabled);
        assert_eq!(actions[2].help.as_deref(), Some("Not on this medium."));
    }

    #[test]
    fn a_page_not_drawn_here_is_said_to_be_one() {
        let mut turn = mode();
        turn.id = Some("disk.choose".into());
        turn.name = Some("Choose a disk".into());
        assert_eq!(Page::of(&turn), Page::Unbuilt { id: "disk.choose".into(), title: "Choose a disk".into() });
        turn.id = None;
        assert!(matches!(Page::of(&turn), Page::Unbuilt { id, .. } if id.is_empty()));
    }

    #[test]
    fn a_page_is_sent_as_the_kind_it_is() {
        let view = View {
            release: Release { version: "2026.8".into(), variant: "Experimental".into() },
            link: Link::Connected { daemon: "installerd/0.1.10".into() },
            trail: vec![("connect".into(), "/run/installerd.sock".into())],
            page: Some(Page::of(&mode())),
        };
        let sent = serde_json::to_value(&view).unwrap();
        assert_eq!(sent["link"], json!({ "state": "connected", "daemon": "installerd/0.1.10" }));
        assert_eq!(sent["page"]["kind"], "mode");
        assert_eq!(sent["page"]["actions"][0], json!({
            "ref": "act.install", "name": "Install Peios", "primary": true, "enabled": true, "help": null,
        }));
        assert_eq!(sent["trail"], json!([["connect", "/run/installerd.sock"]]));
        assert_eq!(sent["release"], json!({ "version": "2026.8", "variant": "Experimental" }));
    }

    #[test]
    fn an_ending_says_how_it_ended() {
        let end = End { seq: 9, outcome: Outcome::Failed, message: Some("cp failed (exit 1)".into()) };
        assert_eq!(Page::ended(&end), Page::Ended { outcome: "failed", message: "cp failed (exit 1)".into() });
    }
}
