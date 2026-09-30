//! What is on a disk, and what the machine has that no disk showed up for.
//!
//! The disk page is where a person decides what to erase, or which system
//! to upgrade or repair, and a device name and a size are not enough to
//! decide either on. This module is the part of that which mounts
//! nothing: a disk's partitions as the kernel lists them, what its
//! partition table and `lsblk` say of each one, and the storage
//! controllers no driver has claimed. Looking *inside* a filesystem
//! takes a mount, which is [`crate::inspect`]'s to do.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::executor::Release;

/// What a partition `holds` when it is a Peios system whose package
/// database would not say which release. It is Peios, so the disk is
/// not one with no Peios on it; and it is not a release, so it is not
/// one to offer an upgrade of.
pub const UNVERSIONED_PEIOS: &str = "Peios";

/// One partition of a disk, as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Partition {
    pub number: u32,
    /// `/dev/nvme0n1p3`.
    pub device: String,
    /// Where it starts on the disk, in bytes.
    pub start: u64,
    /// Bytes.
    pub size: u64,
    /// What the partition table says it is for -- `esp`, `msr`,
    /// `msdata`, `winre`, `linux`, `swap` -- or empty when the table
    /// says something this does not know.
    pub kind: String,
    /// The name the partition table gives it.
    pub name: String,
    /// The filesystem, as a person reads it: `FAT32`, `NTFS`, `ext4`.
    /// Empty when there is none, or none `lsblk` recognises.
    pub fs: String,
    /// The filesystem's own label.
    pub label: String,
    /// Bytes in use, when the filesystem was looked into.
    pub used: Option<u64>,
    /// What it holds that a person would know by name: `Windows`, a
    /// Peios release, another system by the name it gives itself.
    pub holds: Option<String>,
    /// The Peios release on it, when that is what it holds.
    pub peios: Option<Release>,
}

impl Partition {
    /// What to call it: the label its filesystem carries, else what the
    /// table says it is for, else the table's name for it.
    pub fn title(&self) -> String {
        if !self.label.is_empty() {
            return self.label.clone();
        }
        if let Some(name) = kind_name(&self.kind) {
            return name.to_string();
        }
        if !self.name.is_empty() {
            return self.name.clone();
        }
        format!("Partition {}", self.number)
    }
}

/// A partition type as the table records it -- a GPT type GUID, or an
/// MBR type byte as `0x83` -- to the short name this uses for it.
pub fn kind_of(parttype: &str) -> &'static str {
    match parttype.to_ascii_lowercase().as_str() {
        "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" | "0xef" => "esp",
        "e3c9e316-0b5c-4db8-817d-f92df00215ae" => "msr",
        "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7" | "0x7" | "0xb" | "0xc" => "msdata",
        "de94bba4-06d1-4d40-a16a-bfd50179d6ac" | "0x27" => "winre",
        "0fc63daf-8483-4772-8e79-3d69d8477de4"
        | "4f68bce3-e8cd-4db1-96e7-fbcaf984b709"
        | "0x83" => "linux",
        "0657fd6d-a4ab-43c4-84e5-0933c84b4f4f" | "0x82" => "swap",
        _ => "",
    }
}

pub fn kind_name(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "esp" => "EFI system partition",
        "msr" => "Microsoft reserved",
        "msdata" => "Basic data",
        "winre" => "Windows recovery",
        "linux" => "Linux filesystem",
        "swap" => "Swap",
        _ => return None,
    })
}

/// `lsblk`'s name for a filesystem, as a person reads it. FAT is named
/// by its width, which `lsblk` reports as the version.
pub fn fs_name(fstype: &str, fsver: &str) -> String {
    match fstype {
        "vfat" if !fsver.is_empty() => fsver.to_string(),
        "vfat" => "FAT".into(),
        "ntfs" => "NTFS".into(),
        "exfat" => "exFAT".into(),
        "btrfs" => "Btrfs".into(),
        "xfs" => "XFS".into(),
        "crypto_LUKS" => "LUKS".into(),
        other => other.to_string(),
    }
}

/// What `lsblk` says of one block device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Probed {
    pub fstype: String,
    pub fsver: String,
    pub label: String,
    pub parttype: String,
    pub partlabel: String,
}

/// `lsblk --json` to what it said of each device, by kernel name. A
/// device it nests under another is as much a device as one it does
/// not, so the tree is flattened.
pub fn parse_lsblk(json: &str) -> HashMap<String, Probed> {
    fn walk(devices: &Value, into: &mut HashMap<String, Probed>) {
        for device in devices.as_array().into_iter().flatten() {
            let field = |key: &str| {
                device
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let name = field("kname");
            if !name.is_empty() {
                into.insert(
                    name,
                    Probed {
                        fstype: field("fstype"),
                        fsver: field("fsver"),
                        label: field("label"),
                        parttype: field("parttype"),
                        partlabel: field("partlabel"),
                    },
                );
            }
            if let Some(children) = device.get("children") {
                walk(children, into);
            }
        }
    }
    let mut into = HashMap::new();
    if let Ok(parsed) = serde_json::from_str::<Value>(json)
        && let Some(devices) = parsed.get("blockdevices")
    {
        walk(devices, &mut into);
    }
    into
}

/// What `lsblk` makes of every block device here. Empty when it cannot
/// be run: a disk is then listed with its partitions and nothing said
/// of what is on them, which is the truth.
pub fn lsblk() -> HashMap<String, Probed> {
    Command::new("lsblk")
        .args([
            "--json",
            "--output",
            "KNAME,FSTYPE,FSVER,LABEL,PARTTYPE,PARTLABEL",
        ])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| parse_lsblk(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

/// What a GPT says of each partition: its type, as the GUID is written
/// out, and its name.
pub type Table = HashMap<u32, (String, String)>;

/// Read the GPT of a disk whose sectors are `sector` bytes. Empty for a
/// disk with no GPT, or one that cannot be read.
///
/// `lsblk` is asked for a partition's type too, and util-linux's
/// answers. Peios' does not yet: it reports the filesystem on a
/// partition and leaves the table's columns empty. The table is twenty
/// lines to read, so it is read here rather than a second tool's
/// listing parsed for it.
///
/// Nothing is verified -- no checksum, no backup header. This is said
/// on a page, beside a size and a filesystem read some other way, and
/// a table too damaged to trust is one the partitioning step refuses
/// for its own reasons.
pub fn read_gpt(disk: &mut (impl Read + Seek), sector: u64) -> Table {
    let mut table = Table::new();
    let le = |b: &[u8]| {
        b.iter()
            .rev()
            .fold(0u64, |n, byte| n << 8 | u64::from(*byte))
    };
    let mut header = [0u8; 92];
    if disk.seek(SeekFrom::Start(sector)).is_err()
        || disk.read_exact(&mut header).is_err()
        || &header[..8] != b"EFI PART"
    {
        return table;
    }
    let (entries, count, size) = (
        le(&header[72..80]),
        le(&header[80..84]),
        le(&header[84..88]),
    );
    // A table says how many entries it has and how big each is; one that
    // says something absurd is not read, rather than believed.
    if !(128..=4096).contains(&size)
        || count > 1024
        || disk.seek(SeekFrom::Start(entries * sector)).is_err()
    {
        return table;
    }
    let mut entry = vec![0u8; size as usize];
    for number in 1..=count as u32 {
        if disk.read_exact(&mut entry).is_err() {
            break;
        }
        if entry[..16].iter().all(|b| *b == 0) {
            continue;
        }
        // A GUID is written with its first three fields byte-swapped.
        let kind = format!(
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{}",
            le(&entry[0..4]),
            le(&entry[4..6]),
            le(&entry[6..8]),
            entry[8],
            entry[9],
            entry[10..16]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let name: Vec<u16> = entry[56..128]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|unit| *unit != 0)
            .collect();
        table.insert(number, (kind, String::from_utf16_lossy(&name)));
    }
    table
}

/// The GPT of the disk at `sys` (`/sys/block/sda`), read from its
/// device. Empty where that cannot be opened, which for anyone but
/// SYSTEM it cannot.
pub fn gpt_of(sys: &Path) -> Table {
    let Some(name) = sys.file_name() else {
        return Table::new();
    };
    let sector = std::fs::read_to_string(sys.join("queue/logical_block_size"))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(512);
    match std::fs::File::open(Path::new("/dev").join(name)) {
        Ok(mut disk) => read_gpt(&mut disk, sector),
        Err(_) => Table::new(),
    }
}

/// The partitions of the disk at `sys` (`/sys/block/sda`), in disk
/// order. The kernel lists each as a directory under its disk carrying
/// a `partition` file, with its start and size in 512-byte sectors
/// whatever the disk's own sector size. What each is for comes from
/// `lsblk` where it says, and from the disk's own `table` where it
/// does not.
pub fn partitions_under(
    sys: &Path,
    probed: &HashMap<String, Probed>,
    table: &Table,
) -> Vec<Partition> {
    let mut partitions = Vec::new();
    let Ok(entries) = std::fs::read_dir(sys) else {
        return partitions;
    };
    for entry in entries.flatten() {
        let read = |f: &str| {
            std::fs::read_to_string(entry.path().join(f))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
        };
        let Some(number) = read("partition") else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut said = probed.get(&name).cloned().unwrap_or_default();
        if let Some((kind, called)) = table.get(&(number as u32)) {
            if said.parttype.is_empty() {
                said.parttype = kind.clone();
            }
            if said.partlabel.is_empty() {
                said.partlabel = called.clone();
            }
        }
        partitions.push(Partition {
            number: number as u32,
            device: format!("/dev/{name}"),
            start: read("start").unwrap_or(0) * 512,
            size: read("size").unwrap_or(0) * 512,
            kind: kind_of(&said.parttype).to_string(),
            name: said.partlabel,
            fs: fs_name(&said.fstype, &said.fsver),
            label: said.label,
            ..Partition::default()
        });
    }
    partitions.sort_by_key(|p| p.start);
    partitions
}

/// A storage controller the machine has that no driver has claimed.
///
/// The usual reason a machine's disks are missing from the disk page:
/// they are behind a controller in a mode nothing here drives, and the
/// way out is in the firmware's settings. Naming it is what turns "no
/// disks found" from a dead end into something a person can act on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controller {
    /// Where it is on the bus: `0000:00:0e.0`.
    pub address: String,
    /// What kind of controller it says it is: `RAID`, `NVMe`.
    pub kind: String,
    /// Its vendor and device, as `8086:9a0b`.
    pub id: String,
}

/// A PCI class code (`0x010400`) to the kind of storage controller it
/// names, or `None` when it is not one. A floppy controller is not one
/// anybody is missing disks behind.
pub fn controller_kind(class: u32) -> Option<&'static str> {
    if class >> 16 != 0x01 {
        return None;
    }
    Some(match (class >> 8) & 0xff {
        0x00 => "SCSI",
        0x01 => "IDE",
        0x02 => return None,
        0x04 => "RAID",
        0x05 => "ATA",
        0x06 => "SATA",
        0x07 => "SAS",
        0x08 => "NVMe",
        _ => "storage",
    })
}

/// Every storage controller under `bus` (`/sys/bus/pci/devices`) with
/// no driver bound, by address.
pub fn unclaimed_under(bus: &Path) -> Vec<Controller> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(bus) else {
        return found;
    };
    for entry in entries.flatten() {
        let hex = |f: &str| {
            std::fs::read_to_string(entry.path().join(f))
                .ok()
                .and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
        };
        let Some(kind) = hex("class").and_then(controller_kind) else {
            continue;
        };
        if entry.path().join("driver").exists() {
            continue;
        }
        found.push(Controller {
            address: entry.file_name().to_string_lossy().into_owned(),
            kind: kind.to_string(),
            id: format!(
                "{:04x}:{:04x}",
                hex("vendor").unwrap_or(0),
                hex("device").unwrap_or(0)
            ),
        });
    }
    found.sort_by(|a, b| a.address.cmp(&b.address));
    found
}

pub fn unclaimed_controllers() -> Vec<Controller> {
    unclaimed_under(Path::new("/sys/bus/pci/devices"))
}

/// What the disk page says about controllers nothing is driving, or
/// `None` when there are none to mention.
pub fn unclaimed_sentence(controllers: &[Controller]) -> Option<String> {
    let named: Vec<String> = controllers
        .iter()
        .map(|c| {
            // By how each is said aloud: an "eye-dee-ee", a "scuzzy".
            let article = match c.kind.as_str() {
                "IDE" | "ATA" | "NVMe" => "an",
                _ => "a",
            };
            format!(
                "{article} {} controller ({}) at {}",
                c.kind, c.id, c.address
            )
        })
        .collect();
    let (head, it, its) = match named.len() {
        0 => return None,
        1 => (
            "A storage controller that nothing on this system is driving",
            "it",
            "Its",
        ),
        _ => (
            "Storage controllers that nothing on this system is driving",
            "them",
            "Their",
        ),
    };
    Some(format!(
        "{head}: {}. Disks attached to {it} cannot be seen. {its} mode can usually be changed \
         in the firmware's settings, often listed as VMD, RST or RAID.",
        named.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partition_type_is_named_whichever_table_recorded_it() {
        assert_eq!(kind_of("C12A7328-F81F-11D2-BA4B-00A0C93EC93B"), "esp");
        assert_eq!(kind_of("0xef"), "esp");
        assert_eq!(kind_of("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"), "msdata");
        assert_eq!(kind_of("0x83"), "linux");
        assert_eq!(kind_of("00000000-0000-0000-0000-000000000000"), "");
        assert_eq!(kind_of(""), "");
    }

    #[test]
    fn a_partition_is_called_by_the_most_telling_thing_known_of_it() {
        let mut p = Partition {
            number: 3,
            kind: "msdata".into(),
            name: "Basic data partition".into(),
            label: "Windows".into(),
            ..Partition::default()
        };
        assert_eq!(p.title(), "Windows");
        p.label.clear();
        assert_eq!(p.title(), "Basic data");
        p.kind.clear();
        assert_eq!(p.title(), "Basic data partition");
        p.name.clear();
        assert_eq!(p.title(), "Partition 3");
    }

    #[test]
    fn a_filesystem_is_named_as_a_person_reads_it() {
        assert_eq!(fs_name("vfat", "FAT32"), "FAT32");
        assert_eq!(fs_name("vfat", ""), "FAT");
        assert_eq!(fs_name("ntfs", ""), "NTFS");
        assert_eq!(fs_name("ext4", "1.0"), "ext4");
        assert_eq!(fs_name("", ""), "");
    }

    /// peiosutils' lsblk and util-linux's agree on this shape: strings
    /// or null, partitions nested under their disk.
    #[test]
    fn lsblk_is_read_whatever_it_nests() {
        let said = parse_lsblk(
            r#"{ "blockdevices": [
                { "kname": "sr0", "fstype": null, "fsver": null, "label": null, "parttype": null, "partlabel": null },
                { "kname": "sda", "fstype": null, "fsver": null, "label": null, "parttype": null, "partlabel": null,
                  "children": [
                    { "kname": "sda1", "fstype": "vfat", "fsver": "FAT32", "label": "ESP",
                      "parttype": "c12a7328-f81f-11d2-ba4b-00a0c93ec93b", "partlabel": "EFI system partition" },
                    { "kname": "sda2", "fstype": "ntfs", "fsver": null, "label": "Windows",
                      "parttype": "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7", "partlabel": "Basic data partition" }
                  ] }
            ] }"#,
        );
        assert_eq!(said.len(), 4);
        assert_eq!(said["sda1"].fsver, "FAT32");
        assert_eq!(said["sda2"].label, "Windows");
        assert_eq!(said["sda"], Probed::default());
        assert!(parse_lsblk("not json").is_empty());
    }

    #[test]
    fn partitions_are_read_from_sysfs_in_disk_order() {
        let dir = std::env::temp_dir().join(format!("installerd-sys-{}", std::process::id()));
        let part = |name: &str, number: &str, start: &str, size: &str| {
            let at = dir.join(name);
            std::fs::create_dir_all(&at).unwrap();
            std::fs::write(at.join("partition"), number).unwrap();
            std::fs::write(at.join("start"), start).unwrap();
            std::fs::write(at.join("size"), size).unwrap();
        };
        part("sda2", "2\n", "1050624\n", "2048\n");
        part("sda1", "1\n", "2048\n", "1048576\n");
        // Not a partition: every disk has these beside them.
        std::fs::create_dir_all(dir.join("queue")).unwrap();
        let mut probed = HashMap::new();
        probed.insert(
            "sda1".to_string(),
            Probed {
                fstype: "vfat".into(),
                fsver: "FAT32".into(),
                parttype: "c12a7328-f81f-11d2-ba4b-00a0c93ec93b".into(),
                ..Probed::default()
            },
        );
        // lsblk said what the first is for; the table says it of the second.
        let mut table = Table::new();
        table.insert(
            1,
            (
                "00000000-0000-0000-0000-000000000001".into(),
                "ignored".into(),
            ),
        );
        table.insert(
            2,
            (
                "0fc63daf-8483-4772-8e79-3d69d8477de4".into(),
                "Peios root".into(),
            ),
        );
        let found = partitions_under(&dir, &probed, &table);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].device, "/dev/sda1");
        assert_eq!((found[0].start, found[0].size), (1 << 20, 512 << 20));
        assert_eq!(
            (found[0].kind.as_str(), found[0].fs.as_str()),
            ("esp", "FAT32")
        );
        assert_eq!(found[0].name, "ignored");
        assert_eq!(found[1].number, 2);
        assert_eq!(
            (found[1].kind.as_str(), found[1].name.as_str()),
            ("linux", "Peios root")
        );
        assert_eq!(found[1].fs, "");
        assert_eq!(found[1].used, None);
    }

    /// A GPT as it lies on a disk of 512-byte sectors: the header in the
    /// second sector, saying where the entries are, and an entry whose
    /// type GUID is written with its first three fields byte-swapped.
    #[test]
    fn a_gpt_is_read_for_each_partitions_type_and_name() {
        let mut disk = vec![0u8; 512 * 4];
        disk[512..520].copy_from_slice(b"EFI PART");
        disk[512 + 72..512 + 80].copy_from_slice(&2u64.to_le_bytes());
        disk[512 + 80..512 + 84].copy_from_slice(&4u32.to_le_bytes());
        disk[512 + 84..512 + 88].copy_from_slice(&128u32.to_le_bytes());
        // The second entry; the first is empty, as a deleted partition leaves it.
        let entry = 1024 + 128;
        disk[entry..entry + 16].copy_from_slice(&[
            0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e,
            0xc9, 0x3b,
        ]);
        for (i, unit) in "EFI system partition".encode_utf16().enumerate() {
            disk[entry + 56 + i * 2..entry + 58 + i * 2].copy_from_slice(&unit.to_le_bytes());
        }
        let table = read_gpt(&mut std::io::Cursor::new(&disk), 512);
        assert_eq!(table.len(), 1);
        assert_eq!(
            table[&2],
            (
                "c12a7328-f81f-11d2-ba4b-00a0c93ec93b".to_string(),
                "EFI system partition".to_string()
            )
        );
        assert_eq!(kind_of(&table[&2].0), "esp");

        // No GPT, a disk too short to hold one, and a table that says
        // something absurd of itself are all simply not read.
        assert!(read_gpt(&mut std::io::Cursor::new(vec![0u8; 4096]), 512).is_empty());
        assert!(read_gpt(&mut std::io::Cursor::new(vec![0u8; 100]), 512).is_empty());
        disk[512 + 84..512 + 88].copy_from_slice(&7u32.to_le_bytes());
        assert!(read_gpt(&mut std::io::Cursor::new(&disk), 512).is_empty());
    }

    #[test]
    fn only_a_storage_controller_with_no_driver_is_unclaimed() {
        let dir = std::env::temp_dir().join(format!("installerd-pci-{}", std::process::id()));
        let device = |address: &str, class: &str, driven: bool| {
            let at = dir.join(address);
            std::fs::create_dir_all(&at).unwrap();
            std::fs::write(at.join("class"), class).unwrap();
            std::fs::write(at.join("vendor"), "0x8086\n").unwrap();
            std::fs::write(at.join("device"), "0x9a0b\n").unwrap();
            if driven {
                std::fs::create_dir_all(at.join("driver")).unwrap();
            }
        };
        device("0000:00:0e.0", "0x010400\n", false);
        device("0000:00:17.0", "0x010601\n", true);
        device("0000:00:02.0", "0x030000\n", false);
        device("0000:00:1f.0", "0x010200\n", false);
        let found = unclaimed_under(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            found,
            vec![Controller {
                address: "0000:00:0e.0".into(),
                kind: "RAID".into(),
                id: "8086:9a0b".into()
            }]
        );
    }

    #[test]
    fn unclaimed_controllers_are_said_in_a_sentence_or_not_at_all() {
        assert_eq!(unclaimed_sentence(&[]), None);
        let raid = Controller {
            address: "0000:00:0e.0".into(),
            kind: "RAID".into(),
            id: "8086:9a0b".into(),
        };
        let one = unclaimed_sentence(std::slice::from_ref(&raid)).unwrap();
        assert!(
            one.starts_with(
                "A storage controller that nothing on this system is driving: \
                 a RAID controller (8086:9a0b) at 0000:00:0e.0. Disks attached to it cannot be seen."
            ),
            "{one}"
        );
        let nvme = Controller {
            address: "0000:01:00.0".into(),
            kind: "NVMe".into(),
            id: "144d:a80a".into(),
        };
        let two = unclaimed_sentence(&[raid, nvme]).unwrap();
        assert!(
            two.contains(
                "; an NVMe controller (144d:a80a) at 0000:01:00.0. Disks attached to them"
            ),
            "{two}"
        );
    }
}
