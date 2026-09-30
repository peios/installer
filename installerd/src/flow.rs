//! The installer's decision tree: which page follows which answer.
//! Pure — no sockets, no execution — so the whole flow is testable
//! as data in, data out. Only this module knows the tree; that is
//! MSIP's contract.

use msip::daemon::{TurnSpec, ValidAnswer};
use msip::element::{Element, types};
use serde_json::{Map, Value, json};

use crate::contents::{self, Controller};
use crate::executor::{Disk, ESP_MIB, Executor, JobKind, Release};
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
    /// Look at the machine again and refresh the open disk page in
    /// place, for the purpose it was opened for.
    Rescan(Mode),
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

/// What the disk page is made from: the machine's disks with what is
/// on them, the controllers no disk showed up for, and -- when the
/// page is choosing a system to upgrade -- what the medium carries.
#[derive(Debug, Clone, Default)]
pub struct Survey {
    pub disks: Vec<Disk>,
    pub controllers: Vec<Controller>,
    pub medium: Option<Release>,
}

/// Look at the machine for the disk page. The one place the flow reads
/// disks' contents, because it is the one page that shows them.
pub fn survey(executor: &dyn Executor, mode: Mode) -> Survey {
    let mut disks = executor.probe_disks();
    executor.read_contents(&mut disks);
    Survey {
        disks,
        controllers: executor.unclaimed_controllers(),
        medium: match mode {
            Mode::Upgrade => executor.medium_release().ok(),
            _ => None,
        },
    }
}

pub fn disk_page(survey: &Survey, mode: Mode) -> TurnSpec {
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
    // What the disk is being chosen for, as a hint: the same page serves
    // all three, and a surface may want to dress it differently for each.
    let purpose = match mode {
        Mode::Install => "install",
        Mode::Upgrade => "upgrade",
        Mode::Repair => "repair",
    };
    let mut target = Element::new("disk.target", types::TABLE);
    target.name = Some("Target disk".into());
    target.required = true;
    target.help = contents::unclaimed_sentence(&survey.controllers);
    target.state.insert("columns".into(), disk_columns());
    target.state.insert("rows".into(), disk_rows(survey, mode));
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
        class: vec![purpose.into()],
    }
}

/// What a rescan changes on the open disk page: the rows, and what it
/// says about controllers nothing drives, which a disk turning up (or
/// a driver loading) can make untrue.
pub fn disk_patch(survey: &Survey, mode: Mode) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert("ref".into(), json!("disk.target"));
    patch.insert("rows".into(), disk_rows(survey, mode));
    patch.insert(
        "help".into(),
        match contents::unclaimed_sentence(&survey.controllers) {
            Some(sentence) => json!(sentence),
            // Null takes the field away (§3.10).
            None => Value::Null,
        },
    );
    patch
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

pub fn disk_rows(survey: &Survey, mode: Mode) -> Value {
    Value::Array(
        survey
            .disks
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
                    "detail": disk_detail(d, mode, survey.medium.as_ref()),
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

/// A row's `detail` (§3.B): what is known of a disk beyond its cells,
/// for a surface that knows this page and can draw more than a table.
///
/// Its keys, all but `bytes` and `partitions` present only when there
/// is something to say:
///
/// - `bytes` -- the disk's size.
/// - `partitions` -- what is on it now, in disk order: each `number`,
///   `device`, `start` and `bytes`, `type` (`esp`, `msr`, `msdata`,
///   `winre`, `linux`, `swap` or empty), `title`, `fs` and `label`,
///   and, where the filesystem was looked into, `used` bytes and what
///   it `holds` by name.
/// - `becomes` -- the partitions an install makes of it, each `role`
///   (`esp`, `root`), `title`, `start`, `bytes` and `fs`. Only when
///   the page is choosing a disk to install onto, and only on a disk
///   that can be chosen.
/// - `system` -- the Peios system on it: the partition it is `on`, its
///   `edition` and `version`, and both as `text`.
/// - `no_system` -- why there is none, when the disk was looked into
///   and none was found.
/// - `upgrade` -- when the page is choosing a system to upgrade and
///   the disk holds one: the release the medium carries as `version`
///   and `text`, and, when it cannot upgrade this disk, why as
///   `blocked`.
pub fn disk_detail(d: &Disk, mode: Mode, medium: Option<&Release>) -> Value {
    let mut detail = Map::new();
    detail.insert("bytes".into(), json!(d.size));
    detail.insert(
        "partitions".into(),
        Value::Array(
            d.partitions
                .iter()
                .map(|p| {
                    let mut part = json!({
                        "number": p.number,
                        "device": p.device,
                        "start": p.start,
                        "bytes": p.size,
                        "type": p.kind,
                        "title": p.title(),
                        "fs": p.fs,
                        "label": p.label,
                    });
                    if let Some(used) = p.used {
                        part["used"] = json!(used);
                    }
                    if let Some(holds) = &p.holds {
                        part["holds"] = json!(holds);
                    }
                    part
                })
                .collect(),
        ),
    );
    if mode == Mode::Install
        && !d.medium
        && let Some(becomes) = becomes(d.size)
    {
        detail.insert("becomes".into(), becomes);
    }
    if let Some((on, release)) = d.system() {
        detail.insert(
            "system".into(),
            json!({
                "on": on.device,
                "edition": release.edition,
                "version": release.version,
                "text": release.text(),
            }),
        );
        if let (Mode::Upgrade, Some(carry)) = (mode, medium) {
            let mut upgrade = json!({ "version": carry.version, "text": carry.text() });
            if let Some(why) = upgrade_blocked(release, carry) {
                upgrade["blocked"] = json!(why);
            }
            detail.insert("upgrade".into(), upgrade);
        }
    } else if let Some(why) = d.no_system() {
        detail.insert("no_system".into(), json!(why));
    }
    Value::Object(detail)
}

/// The layout an install gives a disk of `size` bytes: what
/// `Real::partition` asks `part` for, said beforehand. `part` keeps the
/// first MiB and aligns to one, so the ESP starts there and the root
/// follows it; the root runs to the last sector the backup table leaves
/// (33 of them, at 512 bytes). `None` for a disk too small to hold it.
pub fn becomes(size: u64) -> Option<Value> {
    const MIB: u64 = 1 << 20;
    let esp = ESP_MIB * MIB;
    let root_start = MIB + esp;
    let root = size.checked_sub(root_start + 33 * 512).filter(|r| *r > 0)?;
    Some(json!([
        { "role": "esp", "title": "EFI system partition", "start": MIB, "bytes": esp, "fs": "FAT32" },
        { "role": "root", "title": "Peios", "start": root_start, "bytes": root, "fs": "ext4" },
    ]))
}

/// Why this medium, carrying `carry`, cannot upgrade a disk holding
/// `have`; `None` when it can. The one ordering the disk page and the
/// confirmation both go by.
pub fn upgrade_blocked(have: &Release, carry: &Release) -> Option<String> {
    if have.edition != carry.edition {
        return Some(format!(
            "A different edition: this medium cannot move {} to {}.",
            have.edition, carry.edition
        ));
    }
    match version::is_newer(&carry.version, &have.version) {
        Ok(true) => None,
        Ok(false) if have.version == carry.version => {
            Some("Already current: the disk holds the release this medium carries.".into())
        }
        Ok(false) => Some("The disk holds a newer release than this medium carries.".into()),
        Err(e) => Some(format!("Cannot order the two versions: {e}")),
    }
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
        (Ok(have), Ok(carry)) => match upgrade_blocked(have, carry) {
            None => (
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
            blocked => (
                format!(
                    "{label} ({target}) holds {}. This medium carries {}.",
                    have.text(),
                    carry.text()
                ),
                blocked,
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
            disk_page(&survey(executor, mode), mode),
        ))
    };
    match (state, act) {
        (FlowState::Mode, "act.install") => choose(Mode::Install),
        (FlowState::Mode, "act.upgrade") => choose(Mode::Upgrade),
        (FlowState::Mode, "act.repair") => choose(Mode::Repair),
        (FlowState::DiskSelect { mode }, "act.rescan") => Some(Advance::Rescan(*mode)),
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
