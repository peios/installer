//! The disk page as it is sent: a row per disk for any surface to list,
//! and under each row's `detail` what is known of the disk beyond that,
//! for a surface that draws more than a table.

use serde_json::{Value, json};

use installerd::contents::{Controller, Partition};
use installerd::executor::{Disk, DryRun, Executor, Inventory, Release};
use installerd::flow::{Mode, disk_page, disk_patch, survey};

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
    }
}

/// The `disk.target` table of the disk page for `mode`.
fn target(executor: &dyn Executor, mode: Mode) -> Value {
    let page = disk_page(&survey(executor, mode), mode);
    assert_eq!(page.id.as_deref(), Some("disk.choose"));
    serde_json::to_value(
        page.elements
            .iter()
            .find(|e| e.r#ref == "disk.target")
            .unwrap(),
    )
    .unwrap()
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
        let page = disk_page(&survey(&executor, mode), mode);
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
