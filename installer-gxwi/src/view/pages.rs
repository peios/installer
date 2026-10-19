//! Construct each browser page from the corresponding MSIP turn.

use super::{
    Action, Choice, DISKS, DOING, Disk, Field, INTERFACES, NETWORK, PLANNED, Page, Phase, Planned,
    Row, SUMMARY, LOG_TAIL_LINES, element, enabled, purpose, rows, said, table_rows, text,
};
use msip::element::types;
use msip::msg::Turn;
use serde_json::Value;

fn action(turn: &Turn, reference: &str) -> Option<Action> {
    element(turn, reference)
        .filter(|element| element.is_action())
        .map(|element| Action::of(turn, element))
}

impl Page {
    /// Build the page for a daemon turn, preserving the shared disk selection.
    pub fn of(turn: &Turn, chosen: Option<&str>) -> Page {
        match turn.id.as_deref() {
            Some("mode") => Self::mode(turn),
            Some("disk.choose") => Self::disk(turn, chosen),
            Some("confirm") => Self::confirm(turn),
            Some(id @ ("install.progress" | "upgrade.progress" | "repair.progress")) => {
                Self::job(turn, id)
            }
            Some("oobe.applying") => Self::applying(turn),
            Some("oobe.locale") => Self::welcome(turn),
            Some("oobe.network") => Self::network(turn),
            Some("oobe.network.manual") => Self::manual(turn),
            Some("oobe.account") => Self::account(turn),
            Some("oobe.naming") => Self::naming(turn),
            id => Page::Unbuilt {
                id: id.unwrap_or_default().to_string(),
                title: turn.name.clone().unwrap_or_default(),
            },
        }
    }

    fn mode(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        Page::Mode {
            title,
            intro: text(turn, "mode.intro"),
            actions: turn
                .elements
                .iter()
                .filter(|e| e.is_action())
                .map(|e| Action::of(turn, e))
                .collect(),
        }
    }

    fn disk(turn: &Turn, chosen: Option<&str>) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let action = |reference: &str| action(turn, reference);
        let table = element(turn, DISKS);
        Page::Disk {
            title,
            intro: text(turn, "disk.intro"),
            purpose: purpose(turn).to_string(),
            disks: rows(turn).iter().map(Disk::of).collect(),
            empty: table
                .and_then(|t| t.state.get("empty"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            trouble: table.and_then(|t| t.help.clone()),
            error: table.and_then(|t| t.error.clone()),
            chosen: chosen.map(str::to_string),
            rescan: action("act.rescan"),
            custom: action("disk.custom"),
            back: action("nav.back"),
            next: action("nav.next"),
        }
    }

    fn confirm(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let action = |reference: &str| action(turn, reference);
        Page::Confirm {
            title,
            summary: text(turn, SUMMARY),
            disk: element(turn, SUMMARY)
                .and_then(|e| e.state.get("detail"))
                .and_then(Disk::said),
            back: action("nav.back"),
            begin: action("act.begin"),
        }
    }

    fn job(turn: &Turn, id: &str) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let said = said(turn);
        Page::Progress {
            title,
            job: id.trim_end_matches(".progress").to_string(),
            summary: text(turn, DOING),
            disk: element(turn, DOING)
                .and_then(|e| e.state.get("detail"))
                .and_then(Disk::said),
            phases: turn
                .elements
                .iter()
                .filter(|e| e.r#type == types::PROGRESS)
                .map(Phase::of)
                .collect(),
            lines: said[said.len().saturating_sub(LOG_TAIL_LINES)..]
                .iter()
                .map(|line| line.as_str().unwrap_or_default().to_string())
                .collect(),
            said: said.len(),
            ended: None,
        }
    }

    fn applying(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let said = said(turn);
        Page::Progress {
            title,
            job: "setup".into(),
            summary: text(turn, "applying.summary"),
            disk: None,
            phases: turn
                .elements
                .iter()
                .filter(|e| e.r#type == types::PROGRESS)
                .map(Phase::of)
                .collect(),
            lines: said[said.len().saturating_sub(LOG_TAIL_LINES)..]
                .iter()
                .map(|line| line.as_str().unwrap_or_default().to_string())
                .collect(),
            said: said.len(),
            ended: None,
        }
    }

    fn welcome(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let action = |reference: &str| action(turn, reference);
        Page::Welcome {
            title,
            intro: text(turn, "locale.intro"),
            language: element(turn, "locale.language")
                .filter(|e| e.r#type == types::SELECT)
                .map(Choice::of),
            next: action("nav.next"),
        }
    }

    fn network(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let action = |reference: &str| action(turn, reference);
        const OWN: &[&str] = &[
            "network.refresh",
            "network.static",
            "network.unplan",
            "nav.back",
            "nav.next",
        ];
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
            others: turn
                .elements
                .iter()
                .filter(|e| e.is_action() && !OWN.contains(&e.r#ref.as_str()))
                .map(|e| Action::of(turn, e))
                .collect(),
            back: action("nav.back"),
            next: action("nav.next"),
        }
    }

    fn manual(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let action = |reference: &str| action(turn, reference);
        let table = element(turn, INTERFACES);
        let said = |value: Option<&Value>| {
            value
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
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
            assumed: table
                .and_then(|t| t.default.as_ref())
                .and_then(Value::as_str)
                .map(str::to_string),
            error: table.and_then(|t| t.error.clone()),
            fields: turn
                .elements
                .iter()
                .filter(|e| e.r#type == types::STRING)
                .map(Field::of)
                .collect(),
            back: action("nav.back"),
            save: action("manual.save"),
        }
    }

    fn account(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let action = |reference: &str| action(turn, reference);
        Page::Account {
            title,
            intro: text(turn, "account.intro"),
            fields: turn
                .elements
                .iter()
                .filter(|e| e.r#type == types::STRING)
                .map(Field::of)
                .collect(),
            back: action("nav.back"),
            next: action("nav.next"),
        }
    }

    fn naming(turn: &Turn) -> Page {
        let title = turn.name.clone().unwrap_or_default();
        let action = |reference: &str| action(turn, reference);
        Page::Naming {
            title,
            name: element(turn, "hostname")
                .filter(|e| e.r#type == types::STRING)
                .map(Field::of),
            others: turn
                .elements
                .iter()
                .filter(|e| {
                    e.is_action() && !["nav.back", "nav.finish"].contains(&e.r#ref.as_str())
                })
                .map(|e| Action::of(turn, e))
                .collect(),
            back: action("nav.back"),
            finish: action("nav.finish"),
            account: None,
        }
    }
}
