//! The disk page as it is sent: a row per disk for any surface to list,
//! and under each row's `detail` what is known of the disk beyond that,
//! for a surface that draws more than a table. And the page an install
//! goes on to from it, which asks whether the disk chosen is to be erased.

use msip::daemon::{TurnSpec, ValidAnswer};
use serde_json::{Value, json};

use installerd::contents::{Controller, Partition};
use installerd::executor::{Disk, DryRun, Executor, Inventory, Release};
use installerd::flow::{Advance, FlowState, Mode, advance, disk_page, disk_patch, survey};

const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;

fn release(version: &str) -> Release {
    Release {
        edition: "dev.peios.peios-experimental".into(),
        version: version.into(),
    }
}

/// A desktop: Windows on one disk, Peios on another, a blank one, and
/// the stick this booted from.
fn desktop() -> Inventory {
    Inventory {
        disks: vec![
            Disk {
                device: "/dev/nvme0n1".into(),
                model: "Samsung SSD 980 PRO 1TB".into(),
                size: 1000204886016,
                bus: "NVMe".into(),
                read: true,
                partitions: vec![
                    Partition {
                        number: 1,
                        device: "/dev/nvme0n1p1".into(),
                        start: MIB,
                        size: 260 * MIB,
                        kind: "esp".into(),
                        fs: "FAT32".into(),
                        used: Some(94 * MIB),
                        holds: Some("Windows Boot Manager".into()),
                        ..Partition::default()
                    },
                    Partition {
                        number: 3,
                        device: "/dev/nvme0n1p3".into(),
                        start: 277 * MIB,
                        size: 930 * GIB,
                        kind: "msdata".into(),
                        name: "Basic data partition".into(),
                        fs: "NTFS".into(),
                        label: "Windows".into(),
                        used: Some(212 * GIB),
                        holds: Some("Windows".into()),
                        ..Partition::default()
                    },
                ],
                ..Disk::default()
            },
            Disk {
                device: "/dev/sda".into(),
                model: "CT500MX500SSD1".into(),
                size: 500107862016,
                bus: "SATA".into(),
                read: true,
                partitions: vec![
                    Partition {
                        number: 1,
                        device: "/dev/sda1".into(),
                        start: MIB,
                        size: 512 * MIB,
                        kind: "esp".into(),
                        fs: "FAT32".into(),
                        label: "PEIOSESP".into(),
                        used: Some(71 * MIB),
                        ..Partition::default()
                    },
                    Partition {
                        number: 2,
                        device: "/dev/sda2".into(),
                        start: 513 * MIB,
                        size: 465 * GIB,
                        kind: "linux".into(),
                        name: "Peios root".into(),
                        fs: "ext4".into(),
                        label: "peios-root".into(),
                        used: Some(18 * GIB),
                        holds: Some("Peios 2026.8-7 (experimental)".into()),
                        peios: Some(release("2026.8-7")),
                    },
                ],
                ..Disk::default()
            },
            Disk {
                device: "/dev/sdb".into(),
                model: "WDC WD20EZAZ".into(),
                size: 2000398934016,
                bus: "SATA".into(),
                read: true,
                ..Disk::default()
            },
            Disk {
                device: "/dev/sdc".into(),
                model: "SanDisk Ultra Fit".into(),
                size: 30752636928,
                bus: "USB".into(),
                removable: true,
                medium: true,
                ..Disk::default()
            },
        ],
        controllers: vec![Controller {
            address: "0000:00:0e.0".into(),
            kind: "RAID".into(),
            id: "8086:9a0b".into(),
        }],
        medium: Some(release("2026.9-1")),
    }
}

fn dry_run(inventory: Inventory) -> DryRun {
    DryRun {
        step_ms: 1,
        inventory: Some(inventory),
        fail_at: None,
    }
}

/// The `disk.target` table of the disk page for `mode`.
fn target(executor: &dyn Executor, mode: Mode) -> Value {
    let page = disk_page(&survey(executor, mode), mode, None);
    assert_eq!(page.id.as_deref(), Some("disk.choose"));
    element(&page, "disk.target")
}

/// The element `r#ref` of `page`, as it is sent.
fn element(page: &TurnSpec, r#ref: &str) -> Value {
    serde_json::to_value(page.elements.iter().find(|e| e.r#ref == r#ref).unwrap()).unwrap()
}

/// What pressing `action` on the disk page for `mode` leads to, with
/// `disk` chosen.
fn pressed(executor: &dyn Executor, mode: Mode, action: &str, disk: &str) -> Advance {
    let answer = ValidAnswer {
        action: Some(action.into()),
        values: [("disk.target".to_string(), json!(disk))]
            .into_iter()
            .collect(),
    };
    advance(&FlowState::DiskSelect { mode }, &answer, executor).unwrap()
}

/// The page `advance` went on to, and the state it left the flow in.
fn page_of(advance: Advance) -> (FlowState, TurnSpec) {
    match advance {
        Advance::Page(state, page) => (state, page),
        _ => panic!("the answer did not lead to a page"),
    }
}

/// `desktop()`, with files on the blank disk: a partition that is nobody's
/// system.
fn desktop_with_media() -> Inventory {
    let mut inventory = desktop();
    inventory.disks[2].partitions = vec![Partition {
        number: 1,
        device: "/dev/sdb1".into(),
        start: MIB,
        size: 1228 * GIB,
        kind: "msdata".into(),
        name: "Basic data partition".into(),
        fs: "NTFS".into(),
        label: "Media".into(),
        used: Some(640 * GIB),
        ..Partition::default()
    }];
    inventory
}

#[test]
fn a_row_is_still_what_any_surface_lists() {
    let table = target(&dry_run(desktop()), Mode::Install);
    let rows = table["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0]["value"], "/dev/nvme0n1");
    assert_eq!(
        rows[0]["cells"],
        json!({ "device": "/dev/nvme0n1", "model": "Samsung SSD 980 PRO 1TB", "size": "931.5 GiB", "bus": "NVMe" })
    );
    assert_eq!(rows[0].get("enabled"), None);
    assert_eq!(rows[3]["enabled"], false);
    assert_eq!(rows[3]["note"], "boot medium");
}

#[test]
fn a_disk_says_what_is_on_it() {
    let table = target(&dry_run(desktop()), Mode::Install);
    let windows = &table["rows"][0]["detail"];
    assert_eq!(windows["bytes"], 1000204886016u64);
    assert_eq!(
        windows["partitions"][0],
        json!({
            "number": 1, "device": "/dev/nvme0n1p1", "start": MIB, "bytes": 260 * MIB,
            "type": "esp", "title": "EFI system partition", "fs": "FAT32", "label": "",
            "used": 94 * MIB, "holds": "Windows Boot Manager",
        })
    );
    assert_eq!(windows["partitions"][1]["title"], "Windows");
    assert_eq!(windows["partitions"][1]["holds"], "Windows");
    assert_eq!(
        windows["no_system"],
        "no partition on it holds a Peios system"
    );
    assert_eq!(windows.get("system"), None);

    let peios = &table["rows"][1]["detail"];
    assert_eq!(
        peios["system"],
        json!({
            "on": "/dev/sda2", "edition": "dev.peios.peios-experimental",
            "version": "2026.8-7", "text": "Peios 2026.8-7 (experimental)",
        })
    );
    assert_eq!(peios.get("no_system"), None);

    let blank = &table["rows"][2]["detail"];
    assert_eq!(blank["partitions"], json!([]));
    assert_eq!(blank["no_system"], "it has no partitions");
}

/// A disk nobody looked into says nothing either way: a partition with
/// no `used` is one that was not read, not one that is empty.
#[test]
fn a_disk_that_was_not_read_claims_nothing() {
    let mut inventory = desktop();
    inventory.disks[0].read = false;
    for partition in &mut inventory.disks[0].partitions {
        partition.used = None;
        partition.holds = None;
    }
    let table = target(&dry_run(inventory), Mode::Repair);
    let detail = &table["rows"][0]["detail"];
    assert_eq!(detail["partitions"].as_array().unwrap().len(), 2);
    assert_eq!(detail["partitions"][0].get("used"), None);
    assert_eq!(detail["partitions"][0].get("holds"), None);
    assert_eq!(detail.get("no_system"), None);
    assert_eq!(detail.get("system"), None);
}

/// What `Real::partition` asks `part` for, said beforehand: on the 8 GiB
/// disk the VM tests install onto, `part` makes 2048..=1050623 and
/// 1050624..=16777182.
#[test]
fn an_install_says_what_the_disk_becomes() {
    let table = target(&dry_run(desktop()), Mode::Install);
    let becomes = &table["rows"][2]["detail"]["becomes"];
    assert_eq!(
        becomes[0],
        json!({ "role": "esp", "title": "EFI system partition", "start": MIB, "bytes": 512 * MIB, "fs": "FAT32" })
    );
    assert_eq!(becomes[1]["role"], "root");
    assert_eq!(becomes[1]["start"], 513 * MIB);

    let eight = installerd::flow::becomes(8 * GIB).unwrap();
    assert_eq!(eight[0]["start"], 2048 * 512);
    assert_eq!(eight[0]["bytes"], 1048576u64 * 512);
    assert_eq!(eight[1]["start"], 1050624u64 * 512);
    assert_eq!(eight[1]["bytes"], 15726559u64 * 512);
    assert_eq!(installerd::flow::becomes(256 * MIB), None);

    // The medium cannot be chosen, so it becomes nothing; nor does any
    // disk when the page is not choosing one to install onto.
    assert_eq!(table["rows"][3]["detail"].get("becomes"), None);
    let repair = target(&dry_run(desktop()), Mode::Repair);
    assert_eq!(repair["rows"][2]["detail"].get("becomes"), None);
}

/// A system is missed by its name and other files by their label; what a
/// machine starts from is not missed in its own right, and nothing is
/// claimed of a filesystem nobody looked into.
#[test]
fn an_install_says_what_would_be_missed_of_what_it_erases() {
    let table = target(&dry_run(desktop_with_media()), Mode::Install);
    // Windows, and not the boot manager on its EFI system partition.
    assert_eq!(table["rows"][0]["detail"]["erases"], json!(["Windows"]));
    assert_eq!(
        table["rows"][1]["detail"]["erases"],
        json!(["Peios 2026.8-7 (experimental)"])
    );
    assert_eq!(
        table["rows"][2]["detail"]["erases"],
        json!(["“Media” (640.0 GiB in use)"])
    );
    // Only where an install is what is being chosen for, and only on a
    // disk it could be done to.
    assert_eq!(table["rows"][3]["detail"].get("erases"), None);
    let repair = target(&dry_run(desktop_with_media()), Mode::Repair);
    assert_eq!(repair["rows"][0]["detail"].get("erases"), None);

    let blank = target(&dry_run(desktop()), Mode::Install);
    assert_eq!(blank["rows"][2]["detail"]["erases"], json!([]));

    let mut unread = desktop();
    unread.disks[0].read = false;
    for partition in &mut unread.disks[0].partitions {
        partition.used = None;
        partition.holds = None;
    }
    let table = target(&dry_run(unread), Mode::Install);
    assert_eq!(table["rows"][0]["detail"]["erases"], json!([]));
}

#[test]
fn the_confirmation_says_which_disk_and_what_goes_with_it() {
    let executor = dry_run(desktop_with_media());
    let (state, page) = page_of(pressed(
        &executor,
        Mode::Install,
        "nav.next",
        "/dev/nvme0n1",
    ));
    assert_eq!(page.id.as_deref(), Some("confirm"));
    let summary = element(&page, "confirm.summary");
    // What it says of the disk as structure is kept for the page after.
    assert_eq!(
        state,
        FlowState::Confirm {
            target: "/dev/nvme0n1".into(),
            label: "Samsung SSD 980 PRO 1TB, 931.5 GiB".into(),
            detail: Some(summary["detail"].clone()),
        }
    );
    assert_eq!(
        summary["text"],
        "Peios will be installed onto Samsung SSD 980 PRO 1TB, 931.5 GiB (/dev/nvme0n1). \
         The whole disk will be erased: partitioned, formatted, and overwritten, \
         including Windows. This cannot be undone."
    );
    // The same, as structure, for a surface that draws the disk.
    let detail = &summary["detail"];
    assert_eq!(detail["device"], "/dev/nvme0n1");
    assert_eq!(detail["model"], "Samsung SSD 980 PRO 1TB");
    assert_eq!(detail["size"], "931.5 GiB");
    assert_eq!(detail["bus"], "NVMe");
    assert_eq!(detail["bytes"], 1000204886016u64);
    assert_eq!(detail["partitions"].as_array().unwrap().len(), 2);
    assert_eq!(detail["becomes"][1]["role"], "root");
    assert_eq!(detail["erases"], json!(["Windows"]));
    let begin = element(&page, "act.begin");
    assert_eq!(begin["destructive"], true);
    assert_eq!(begin["primary"], true);

    // Several things to miss are listed, and none is not mentioned.
    let (_, page) = page_of(pressed(&executor, Mode::Install, "nav.next", "/dev/sdb"));
    assert!(
        element(&page, "confirm.summary")["text"]
            .as_str()
            .unwrap()
            .contains("overwritten, including “Media” (640.0 GiB in use). This")
    );
    let blank = dry_run(desktop());
    let (_, page) = page_of(pressed(&blank, Mode::Install, "nav.next", "/dev/sdb"));
    assert!(
        element(&page, "confirm.summary")["text"]
            .as_str()
            .unwrap()
            .contains("formatted, and overwritten. This cannot be undone.")
    );
}

/// The page a job runs on says what it is being done to: in words for any
/// surface, and for an install with the disk as the confirmation had it,
/// since a surface may join when this page is the first it sees.
#[test]
fn the_page_a_job_runs_on_says_which_disk() {
    let executor = dry_run(desktop_with_media());
    let begin = ValidAnswer {
        action: Some("act.begin".into()),
        values: Default::default(),
    };
    let (confirming, confirmation) = page_of(pressed(
        &executor,
        Mode::Install,
        "nav.next",
        "/dev/nvme0n1",
    ));
    let Some(Advance::Begin { target, page, .. }) = advance(&confirming, &begin, &executor) else {
        panic!("the confirmation's button did not begin a job");
    };
    assert_eq!(target, "/dev/nvme0n1");
    assert_eq!(page.id.as_deref(), Some("install.progress"));
    let summary = element(&page, "progress.summary");
    assert_eq!(
        summary["text"],
        "Installing Peios onto Samsung SSD 980 PRO 1TB, 931.5 GiB (/dev/nvme0n1). \
         Leave the machine on and the install medium in place until this finishes."
    );
    assert_eq!(
        summary["detail"],
        element(&confirmation, "confirm.summary")["detail"]
    );
    assert_eq!(summary["detail"]["becomes"][1]["role"], "root");
    // The phases follow it, each from nothing, and then what the job says.
    let refs: Vec<&str> = page.elements.iter().map(|e| e.r#ref.as_str()).collect();
    assert_eq!(
        refs,
        [
            "progress.summary",
            "phase.partition",
            "phase.format",
            "phase.copy",
            "phase.boot",
            "out"
        ]
    );
    assert_eq!(element(&page, "phase.copy")["value"], 0);

    // The jobs on a system already there name the disk, and draw nothing.
    for (state, id, says) in [
        (
            FlowState::UpgradeConfirm {
                target: "/dev/sda".into(),
            },
            "upgrade.progress",
            "Upgrading the system on /dev/sda. ",
        ),
        (
            FlowState::RepairMenu {
                target: "/dev/sda".into(),
            },
            "repair.progress",
            "Repairing the system on /dev/sda. ",
        ),
    ] {
        let press = ValidAnswer {
            action: Some(
                if id == "repair.progress" {
                    "repair.boot"
                } else {
                    "act.begin"
                }
                .into(),
            ),
            values: Default::default(),
        };
        let Some(Advance::Begin { page, .. }) = advance(&state, &press, &executor) else {
            panic!("no job was begun");
        };
        assert_eq!(page.id.as_deref(), Some(id));
        let summary = element(&page, "progress.summary");
        assert!(summary["text"].as_str().unwrap().starts_with(says));
        assert_eq!(summary.get("detail"), None);
    }
}

/// The answer names a disk, and any string is a well-formed answer: what
/// may be gone on with is a disk of this machine's that is not the medium.
#[test]
fn only_a_disk_that_may_be_chosen_is_gone_on_with() {
    let executor = dry_run(desktop());
    for mode in [Mode::Install, Mode::Upgrade, Mode::Repair] {
        let Advance::Reject(errors) = pressed(&executor, mode, "nav.next", "/dev/sdc") else {
            panic!("the medium was gone on with");
        };
        assert_eq!(
            errors,
            [(
                "disk.target".to_string(),
                "/dev/sdc is the medium this is running from.".to_string()
            )]
        );
        let Advance::Reject(errors) = pressed(&executor, mode, "nav.next", "/dev/sdz") else {
            panic!("a disk that is not there was gone on with");
        };
        assert_eq!(
            errors[0].1,
            "/dev/sdz is no longer there. Rescan and choose again."
        );
    }
}

/// Back from the page after the disk page returns to it with the disk
/// still chosen, as the table's default.
#[test]
fn going_back_to_the_disks_leaves_the_disk_chosen() {
    let executor = dry_run(desktop());
    let back = ValidAnswer {
        action: Some("nav.back".into()),
        values: Default::default(),
    };
    let states = [
        (
            Mode::Install,
            FlowState::Confirm {
                target: "/dev/sda".into(),
                label: "CT500MX500SSD1, 465.8 GiB".into(),
                detail: None,
            },
        ),
        (
            Mode::Upgrade,
            FlowState::UpgradeConfirm {
                target: "/dev/sda".into(),
            },
        ),
        (
            Mode::Repair,
            FlowState::RepairMenu {
                target: "/dev/sda".into(),
            },
        ),
    ];
    for (mode, state) in states {
        let (state, page) = page_of(advance(&state, &back, &executor).unwrap());
        assert_eq!(state, FlowState::DiskSelect { mode });
        assert_eq!(element(&page, "disk.target")["default"], "/dev/sda");
    }
    // Arriving at the page the first time, nothing is.
    assert_eq!(target(&executor, Mode::Install).get("default"), None);
    // Nor is a disk that has gone, or the medium.
    let survey = survey(&executor, Mode::Install);
    for gone in ["/dev/sdz", "/dev/sdc"] {
        let page = disk_page(&survey, Mode::Install, Some(gone));
        assert_eq!(element(&page, "disk.target").get("default"), None);
    }
}

#[test]
fn an_upgrade_says_what_the_medium_would_move_each_system_to() {
    let table = target(&dry_run(desktop()), Mode::Upgrade);
    assert_eq!(
        table["rows"][1]["detail"]["upgrade"],
        json!({ "version": "2026.9-1", "text": "Peios 2026.9-1 (experimental)" })
    );
    // Only a disk with a system on it has anything to upgrade.
    assert_eq!(table["rows"][0]["detail"].get("upgrade"), None);
    // And only the upgrade's page says it.
    let install = target(&dry_run(desktop()), Mode::Install);
    assert_eq!(install["rows"][1]["detail"].get("upgrade"), None);

    let mut current = desktop();
    current.medium = Some(release("2026.8-7"));
    let table = target(&dry_run(current), Mode::Upgrade);
    assert!(
        table["rows"][1]["detail"]["upgrade"]["blocked"]
            .as_str()
            .unwrap()
            .starts_with("Already current")
    );
}

#[test]
fn the_page_says_what_it_is_choosing_a_disk_for() {
    let executor = dry_run(desktop());
    for (mode, purpose) in [
        (Mode::Install, "install"),
        (Mode::Upgrade, "upgrade"),
        (Mode::Repair, "repair"),
    ] {
        let page = disk_page(&survey(&executor, mode), mode, None);
        assert_eq!(page.class, vec![purpose.to_string()]);
    }
}

#[test]
fn a_controller_nothing_drives_is_said_on_the_page_and_unsaid_by_a_rescan() {
    let table = target(&dry_run(desktop()), Mode::Install);
    assert!(
        table["help"]
            .as_str()
            .unwrap()
            .contains("a RAID controller (8086:9a0b) at 0000:00:0e.0")
    );

    let mut claimed = desktop();
    claimed.controllers.clear();
    let executor = dry_run(claimed);
    assert_eq!(target(&executor, Mode::Install).get("help"), None);
    // A rescan takes the sentence away once it is no longer true.
    let patch = disk_patch(&survey(&executor, Mode::Install), Mode::Install);
    assert_eq!(patch["ref"], "disk.target");
    assert_eq!(patch["help"], Value::Null);
    assert_eq!(patch["rows"].as_array().unwrap().len(), 4);
}

/// Peios, by its os-release, whose package database would not say which
/// release: not a release to upgrade, and not a disk with no Peios on it.
#[test]
fn a_peios_system_that_will_not_say_its_release_is_neither_claimed_nor_denied() {
    let mut inventory = desktop();
    let root = &mut inventory.disks[1].partitions[1];
    root.holds = Some(installerd::contents::UNVERSIONED_PEIOS.into());
    root.peios = None;
    let table = target(&dry_run(inventory), Mode::Upgrade);
    let detail = &table["rows"][1]["detail"];
    assert_eq!(detail["partitions"][1]["holds"], "Peios");
    assert_eq!(detail.get("system"), None);
    assert_eq!(detail.get("no_system"), None);
    assert_eq!(detail.get("upgrade"), None);
}

#[test]
fn a_described_machine_answers_for_what_each_disk_holds() {
    let executor = dry_run(desktop());
    assert_eq!(
        executor.installed_release("/dev/sda").unwrap().version,
        "2026.8-7"
    );
    assert_eq!(
        executor.installed_release("/dev/nvme0n1").unwrap_err(),
        "no partition on it holds a Peios system"
    );
    assert_eq!(
        executor.installed_release("/dev/sdb").unwrap_err(),
        "it has no partitions"
    );
    assert_eq!(executor.medium_release().unwrap().version, "2026.9-1");
}

/// `installerd --dry-run --inventory`: a machine described in JSON, with
/// everything left out taken as nothing, and a misspelt key refused
/// rather than silently describing a different machine.
#[test]
fn a_machine_is_described_in_json() {
    let inventory: Inventory = serde_json::from_value(json!({
        "disks": [{
            "device": "/dev/sda", "model": "CT500MX500SSD1", "size": 500107862016u64, "bus": "SATA", "read": true,
            "partitions": [{
                "number": 2, "device": "/dev/sda2", "start": 537919488, "size": 499569942528u64,
                "kind": "linux", "fs": "ext4", "label": "peios-root", "used": 19756220416u64,
                "holds": "Peios 2026.8-7 (experimental)",
                "peios": { "edition": "dev.peios.peios-experimental", "version": "2026.8-7" },
            }],
        }],
        "medium": { "edition": "dev.peios.peios-experimental", "version": "2026.9-1" },
    }))
    .unwrap();
    assert!(inventory.controllers.is_empty());
    assert!(!inventory.disks[0].medium);
    assert_eq!(inventory.disks[0].system().unwrap().1.version, "2026.8-7");

    let misspelt =
        serde_json::from_value::<Inventory>(json!({ "disks": [{ "devise": "/dev/sda" }] }));
    assert!(misspelt.is_err());
}
