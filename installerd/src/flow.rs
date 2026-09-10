//! The installer's decision tree: which page follows which answer.
//! Pure — no sockets, no execution — so the whole flow is testable
//! as data in, data out. Only this module knows the tree; that is
//! MSIP's contract.

use msip::daemon::{TurnSpec, ValidAnswer};
use msip::element::{Element, types};
use serde_json::{Value, json};

use crate::executor::{Disk, Executor, JobKind, Release};
use crate::version;

/// What the disk is being chosen for. Decides the disk page's wording
/// and which page follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Install,
    Upgrade,
    Repair,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlowState {
    Mode,
    DiskSelect { mode: Mode },
    Confirm { target: String, label: String },
    UpgradeConfirm { target: String },
    RepairMenu { target: String },
    Running,
}

/// What a valid answer leads to.
pub enum Advance {
    /// Broadcast this page and move to this state.
    Page(FlowState, TurnSpec),
    /// Re-probe disks and refresh the open page's choices in place.
    Rescan,
    /// Start a job; the server issues the progress page and runs it.
    Begin { kind: JobKind, target: String },
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

fn no_validate(mut e: Element) -> Element {
    e.state.insert("validate".into(), json!(false));
    e
}

fn primary(mut e: Element) -> Element {
    e.state.insert("primary".into(), json!(true));
    e
}

fn disabled(mut e: Element, why: &str) -> Element {
    e.enabled = false;
    e.help = Some(why.into());
    e
}

pub fn mode_page() -> TurnSpec {
    TurnSpec {
        id: Some("mode".into()),
        name: Some("Peios Setup".into()),
        elements: vec![
            text(
                "mode.intro",
                "Set up Peios on this machine, or upgrade or repair a system that is already installed.",
            ),
            primary(action("act.install", "Install Peios")),
            action("act.upgrade", "Upgrade an installation"),
            action("act.repair", "Repair an existing system"),
        ],
        class: vec!["menu".into()],
    }
}

pub fn disk_page(disks: &[Disk], mode: Mode) -> TurnSpec {
    let intro = match mode {
        Mode::Install => "Choose the disk to install onto. Everything on it will be erased.",
        Mode::Upgrade => "Choose the disk holding the system to upgrade.",
        Mode::Repair => "Choose the disk holding the system to repair.",
    };
    let name = match mode {
        Mode::Install => "Choose a disk",
        Mode::Upgrade => "Upgrade: choose a disk",
        Mode::Repair => "Repair: choose a disk",
    };
    let mut target = Element::new("disk.target", types::TABLE);
    target.name = Some("Target disk".into());
    target.required = true;
    target.state.insert("columns".into(), disk_columns());
    target.state.insert("rows".into(), disk_rows(disks));
    target.state.insert(
        "empty".into(),
        json!("No disks found. Attach one and rescan."),
    );
    TurnSpec {
        id: Some("disk.choose".into()),
        name: Some(name.into()),
        elements: vec![
            text("disk.intro", intro),
            target,
            no_validate(action("act.rescan", "Rescan disks")),
            disabled(
                action("disk.custom", "Custom partitioning…"),
                "Whole-disk only for now; a partitioning page is future work (PEI-52).",
            ),
            no_validate(action("nav.back", "Back")),
            primary(action("nav.next", "Next")),
        ],
        ..Default::default()
    }
}

/// In priority order (a2): a surface too narrow for all of them drops
/// from the right, and the device is the one thing a person must see.
pub fn disk_columns() -> Value {
    json!([
        {"key": "device", "name": "Device"},
        {"key": "model", "name": "Model"},
        {"key": "size", "name": "Size", "align": "right"},
        {"key": "bus", "name": "Bus"},
    ])
}

pub fn disk_rows(disks: &[Disk]) -> Value {
    Value::Array(
        disks
            .iter()
            .map(|d| {
                let mut row = json!({
                    "value": d.device,
                    "cells": {
                        "device": d.device,
                        "model": d.model,
                        "size": crate::executor::size_text(d.size),
                        "bus": d.bus,
                    },
                });
                if d.medium {
                    row["enabled"] = json!(false);
                    row["note"] = json!("boot medium");
                } else if d.removable {
                    row["note"] = json!("removable");
                }
                row
            })
            .collect(),
    )
}

pub fn confirm_page(target: &str, label: &str) -> TurnSpec {
    TurnSpec {
        id: Some("confirm".into()),
        name: Some("Ready to install".into()),
        elements: vec![
            text(
                "confirm.summary",
                &format!(
                    "Peios will be installed onto {label} ({target}). \
                     The whole disk will be erased: partitioned, formatted, \
                     and overwritten. This cannot be undone."
                ),
            ),
            no_validate(action("nav.back", "Back")),
            {
                let mut go = primary(action("act.begin", "Erase disk and install"));
                go.state.insert("destructive".into(), json!(true));
                go
            },
        ],
        class: vec!["confirm".into()],
    }
}

/// The upgrade's confirmation: what the disk holds, what the medium
/// carries, and a button that is live only when the second is newer.
///
/// Disabled rather than absent when it is not, and with the reason as
/// its help, because "already current" and "that is not a Peios disk"
/// are answers a person came to this page for, and a page with no way
/// forward and no sentence is the thing MSIP's disabled-with-help
/// exists to prevent.
pub fn upgrade_confirm_page(
    target: &str,
    label: &str,
    installed: Result<Release, String>,
    medium: Result<Release, String>,
) -> TurnSpec {
    let (summary, blocked): (String, Option<String>) = match (&installed, &medium) {
        (Ok(have), Ok(carry)) if have.edition != carry.edition => (
            format!(
                "{label} ({target}) holds {}. This medium carries {}.",
                have.text(),
                carry.text()
            ),
            Some(format!(
                "A different edition: this medium cannot move {} to {}.",
                have.edition, carry.edition
            )),
        ),
        (Ok(have), Ok(carry)) => match version::is_newer(&carry.version, &have.version) {
            Ok(true) => (
                format!(
                    "{label} ({target}) holds {}. It will be upgraded to {}: the system's \
                     packages are replaced with this medium's, its boot files are rewritten, \
                     and everything under /lcl is left as it is. The new release's settings \
                     apply on the next boot.",
                    have.text(),
                    carry.text()
                ),
                None,
            ),
            Ok(false) => (
                format!(
                    "{label} ({target}) holds {}. This medium carries {}.",
                    have.text(),
                    carry.text()
                ),
                Some(if have.version == carry.version {
                    "Already current: the disk holds the release this medium carries.".into()
                } else {
                    "The disk holds a newer release than this medium carries.".into()
                }),
            ),
            Err(e) => (
                format!(
                    "{label} ({target}) holds {}. This medium carries {}.",
                    have.text(),
                    carry.text()
                ),
                Some(format!("Cannot order the two versions: {e}")),
            ),
        },
        (Err(why), _) => (
            format!("{label} ({target}) could not be read as a Peios system: {why}"),
            Some("Nothing to upgrade here. Choose another disk, or install instead.".into()),
        ),
        (Ok(have), Err(why)) => (
            format!(
                "{label} ({target}) holds {}. This medium's own release could not be read: {why}",
                have.text()
            ),
            Some("This medium cannot say what it carries, so it will not upgrade anything.".into()),
        ),
    };
    let go = primary(action("act.begin", "Upgrade"));
    TurnSpec {
        id: Some("upgrade.confirm".into()),
        name: Some("Ready to upgrade".into()),
        elements: vec![
            text("confirm.summary", &summary),
            no_validate(action("nav.back", "Back")),
            match blocked {
                Some(why) => disabled(go, &why),
                None => go,
            },
        ],
        class: vec!["confirm".into()],
    }
}

pub fn repair_menu(target: &str) -> TurnSpec {
    TurnSpec {
        id: Some("repair.menu".into()),
        name: Some("Repair".into()),
        elements: vec![
            text(
                "repair.intro",
                &format!("Repairing the system on {target}. Choose an operation."),
            ),
            primary(action("repair.boot", "Repair boot files")),
            action("repair.fsck", "Check the filesystem"),
            action("repair.sd", "Reseed security descriptors"),
            disabled(
                action("repair.verify", "Verify installed packages"),
                "Blocked on installed-file verification (PEI-86).",
            ),
            disabled(
                action("repair.reinstall", "Reinstall, keeping local data"),
                "Replace the system tree while preserving /lcl — planned (PEI-52).",
            ),
            no_validate(action("nav.back", "Back")),
        ],
        class: vec!["menu".into()],
    }
}

pub fn progress_page(kind: JobKind, executor: &dyn Executor) -> TurnSpec {
    let mut elements: Vec<Element> = executor
        .phases(kind)
        .iter()
        .map(|p| {
            let mut e = Element::new(p.r#ref, types::PROGRESS);
            e.name = Some(p.name.into());
            e.state.insert("value".into(), json!(0));
            e.state.insert("max".into(), json!(100));
            e
        })
        .collect();
    let mut log = Element::new("out", types::LOG);
    log.name = Some("Details".into());
    log.state.insert("lines".into(), json!([]));
    elements.push(log);
    TurnSpec {
        id: Some(
            match kind {
                JobKind::Install => "install.progress",
                JobKind::Upgrade => "upgrade.progress",
                _ => "repair.progress",
            }
            .to_string(),
        ),
        name: Some(
            match kind {
                JobKind::Install => "Installing",
                JobKind::Upgrade => "Upgrading",
                _ => "Repairing",
            }
            .into(),
        ),
        elements,
        class: vec!["progress".into()],
    }
}

/// The tree itself.
pub fn advance(
    state: &FlowState,
    answer: &ValidAnswer,
    executor: &dyn Executor,
) -> Option<Advance> {
    let act = answer.action.as_deref().unwrap_or("");
    let choose = |mode: Mode| {
        Some(Advance::Page(
            FlowState::DiskSelect { mode },
            disk_page(&executor.probe_disks(), mode),
        ))
    };
    match (state, act) {
        (FlowState::Mode, "act.install") => choose(Mode::Install),
        (FlowState::Mode, "act.upgrade") => choose(Mode::Upgrade),
        (FlowState::Mode, "act.repair") => choose(Mode::Repair),
        (FlowState::DiskSelect { .. }, "act.rescan") => Some(Advance::Rescan),
        (FlowState::DiskSelect { .. }, "nav.back") => {
            Some(Advance::Page(FlowState::Mode, mode_page()))
        }
        (FlowState::DiskSelect { mode }, "nav.next") => {
            let target = answer.values.get("disk.target")?.as_str()?.to_string();
            let label = executor
                .probe_disks()
                .iter()
                .find(|d| d.device == target)
                .map(|d| d.label())
                .unwrap_or_else(|| target.clone());
            match mode {
                Mode::Install => Some(Advance::Page(
                    FlowState::Confirm {
                        target: target.clone(),
                        label: label.clone(),
                    },
                    confirm_page(&target, &label),
                )),
                Mode::Upgrade => Some(Advance::Page(
                    FlowState::UpgradeConfirm {
                        target: target.clone(),
                    },
                    upgrade_confirm_page(
                        &target,
                        &label,
                        executor.installed_release(&target),
                        executor.medium_release(),
                    ),
                )),
                Mode::Repair => Some(Advance::Page(
                    FlowState::RepairMenu {
                        target: target.clone(),
                    },
                    repair_menu(&target),
                )),
            }
        }
        (FlowState::Confirm { .. }, "nav.back") => choose(Mode::Install),
        (FlowState::Confirm { target, .. }, "act.begin") => Some(Advance::Begin {
            kind: JobKind::Install,
            target: target.clone(),
        }),
        (FlowState::UpgradeConfirm { .. }, "nav.back") => choose(Mode::Upgrade),
        (FlowState::UpgradeConfirm { target }, "act.begin") => Some(Advance::Begin {
            kind: JobKind::Upgrade,
            target: target.clone(),
        }),
        (FlowState::RepairMenu { .. }, "nav.back") => choose(Mode::Repair),
        (FlowState::RepairMenu { target }, kindref) => {
            let kind = match kindref {
                "repair.boot" => JobKind::RepairBoot,
                "repair.fsck" => JobKind::RepairFsck,
                "repair.sd" => JobKind::RepairSd,
                _ => return None,
            };
            Some(Advance::Begin {
                kind,
                target: target.clone(),
            })
        }
        _ => None,
    }
}
