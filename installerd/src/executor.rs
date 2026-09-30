//! What the flow runs when the person says go. The flow and the
//! server know only this trait; M2 ships the dry-run implementation,
//! M3 the real one.

pub use msip_serve::Progress;

use std::thread::sleep;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::contents::{self, Controller, Partition};

/// How big an install makes the EFI system partition, in MiB. One
/// number for the partitioning and for the page that says beforehand
/// what the disk will become, so the two cannot disagree.
pub const ESP_MIB: u64 = 512;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Disk {
    /// Stable device path -- the answer value.
    pub device: String,
    pub model: String,
    /// Bytes.
    pub size: u64,
    /// How it is attached -- virtio, SATA, NVMe, USB, SD -- or empty
    /// when sysfs does not say.
    pub bus: String,
    pub removable: bool,
    /// The disk this live system booted from. Listed, so nobody wonders
    /// where it went; not offered, because installing onto the medium
    /// you are running from is a mistake rather than a choice.
    pub medium: bool,
    /// Its partitions as they are, in disk order. Empty for the medium,
    /// which is not a disk anybody is choosing.
    pub partitions: Vec<Partition>,
    /// Whether the filesystems on it were looked into. Until they were,
    /// a partition with nothing said of what it holds is one nobody
    /// looked at, not one known to be empty.
    pub read: bool,
}

impl Disk {
    /// For prose: "Virtio disk, 8.0 GiB".
    pub fn label(&self) -> String {
        format!("{}, {}", self.model, size_text(self.size))
    }

    /// The Peios system on it, and the partition that holds it.
    pub fn system(&self) -> Option<(&Partition, &Release)> {
        self.partitions
            .iter()
            .find_map(|p| p.peios.as_ref().map(|release| (p, release)))
    }

    /// Why there is no Peios system here to upgrade or repair, once the
    /// disk has been looked into and none was found. A Peios system
    /// that will not say which release it is is not none: nothing is
    /// claimed of that disk either way.
    pub fn no_system(&self) -> Option<&'static str> {
        let unversioned = |p: &Partition| p.holds.as_deref() == Some(contents::UNVERSIONED_PEIOS);
        if !self.read || self.system().is_some() || self.partitions.iter().any(unversioned) {
            return None;
        }
        Some(if self.partitions.is_empty() {
            "it has no partitions"
        } else {
            "no partition on it holds a Peios system"
        })
    }
}

/// Bytes as a person reads them, to one decimal above a gibibyte. The
/// old integer-GiB label showed a 966 MiB medium as "0 GiB", which is
/// how a boot medium came to look like an empty disk.
pub fn size_text(bytes: u64) -> String {
    const KIB: u64 = 1 << 10;
    const MIB: u64 = 1 << 20;
    const GIB: u64 = 1 << 30;
    const TIB: u64 = 1 << 40;
    if bytes >= TIB {
        format!("{:.1} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{} MiB", bytes / MIB)
    } else {
        format!("{} KiB", bytes / KIB)
    }
}

/// Every whole disk `/sys/block` lists, read-only: no udev exists to
/// ask, and lsblk would be a shell round trip per device. Shared by the
/// real executor and the dry run, which differ in what they *do* to a
/// disk and not in how they find one.
pub fn probe_sys_block() -> Vec<Disk> {
    let medium = medium_disk();
    let mut disks = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return disks;
    };
    let probed = contents::lsblk();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Not disks anybody installs onto: loop and RAM devices, optical
        // drives, and a floppy drive, which a machine old enough (or a
        // virtual one) still reports, with or without a floppy in it.
        if ["loop", "ram", "sr", "fd"]
            .iter()
            .any(|kind| name.starts_with(kind))
        {
            continue;
        }
        let read = |f: &str| {
            std::fs::read_to_string(entry.path().join(f))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let sectors: u64 = read("size").parse().unwrap_or(0);
        if sectors == 0 {
            continue;
        }
        let bus = bus_of(
            &std::fs::read_link(entry.path())
                .unwrap_or_default()
                .to_string_lossy(),
        );
        let model = match read("device/model").as_str() {
            "" if bus == "virtio" => "Virtio disk".to_string(),
            "" => "Disk".to_string(),
            m => m.to_string(),
        };
        let is_medium = medium.as_deref() == Some(name.as_str());
        disks.push(Disk {
            device: format!("/dev/{name}"),
            model,
            size: sectors * 512,
            bus,
            removable: read("removable") == "1",
            medium: is_medium,
            partitions: if is_medium {
                Vec::new()
            } else {
                contents::partitions_under(&entry.path(), &probed, &contents::gpt_of(&entry.path()))
            },
            read: false,
        });
    }
    disks.sort_by(|a, b| a.device.cmp(&b.device));
    disks
}

/// The transport, read off the sysfs path a block device symlink
/// resolves to -- the only place a virtio disk says what it is.
fn bus_of(path: &str) -> String {
    for (needle, bus) in [
        ("/virtio", "virtio"),
        ("/nvme", "NVMe"),
        ("/usb", "USB"),
        ("/ata", "SATA"),
        ("/mmc", "SD/MMC"),
        ("/scsi", "SCSI"),
    ] {
        if path.contains(needle) {
            return bus.to_string();
        }
    }
    String::new()
}

/// The whole disk the boot medium is mounted from, by name ("vda"), or
/// `None` when nothing is mounted at the place live-boot puts it --
/// which is every machine that is not a live one.
fn medium_disk() -> Option<String> {
    const MOUNTPOINT: &str = "/media/peios";
    let info = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    let source = info.lines().find_map(|line| {
        let (head, tail) = line.split_once(" - ")?;
        let fields: Vec<&str> = head.split_whitespace().collect();
        if fields.get(4) != Some(&MOUNTPOINT) {
            return None;
        }
        tail.split_whitespace().nth(1).map(str::to_string)
    })?;
    let leaf = source.strip_prefix("/dev/")?.to_string();
    // A partition's sysfs link ends .../block/<disk>/<partition>; a
    // whole disk's ends .../block/<disk>.
    let link = std::fs::read_link(format!("/sys/class/block/{leaf}")).ok()?;
    let mut parts = link.components().rev();
    let last = parts.next()?.as_os_str().to_string_lossy().into_owned();
    let parent = parts.next()?.as_os_str().to_string_lossy().into_owned();
    Some(if parent == "block" { last } else { parent })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Install,
    Upgrade,
    RepairBoot,
    RepairFsck,
    RepairSd,
}

/// One phase of a running job, for the progress page.
#[derive(Debug, Clone, Copy)]
pub struct Phase {
    /// Element ref on the progress page.
    pub r#ref: &'static str,
    pub name: &'static str,
}

/// One Peios release as a package: the edition package's name and its
/// full version, revision included, because a rebuilt edition with the
/// same upstream version is a different release to peipkg and must be
/// to the upgrade page too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    /// `dev.peios.peios-experimental`: the qualified edition package.
    pub edition: String,
    /// `2026.8-7`.
    pub version: String,
}

impl Release {
    /// "Peios 2026.8-7 (experimental)".
    pub fn text(&self) -> String {
        let variant = self
            .edition
            .strip_prefix("dev.peios.peios-")
            .unwrap_or(&self.edition);
        format!("Peios {} ({variant})", self.version)
    }
}

pub trait Executor: Send + Sync + 'static {
    /// The machine's disks and their partition tables. Quick, and
    /// touches nothing.
    fn probe_disks(&self) -> Vec<Disk>;
    /// Look into the filesystems on `disks` and say what each holds,
    /// changing nothing on any of them. Slower than the probe, since it
    /// is a mount for every filesystem, so it is asked for only by the
    /// page that shows what it finds.
    fn read_contents(&self, disks: &mut [Disk]) {
        let _ = disks;
    }
    /// Storage controllers no driver has claimed: where the disks that
    /// are missing from the probe may be.
    fn unclaimed_controllers(&self) -> Vec<Controller> {
        contents::unclaimed_controllers()
    }
    fn phases(&self, kind: JobKind) -> &'static [Phase];
    /// The release this medium carries -- what an upgrade would move a
    /// disk to.
    fn medium_release(&self) -> Result<Release, String>;
    /// The release installed on `target`, read without changing it. An
    /// `Err` is a sentence for the page: no Peios there, or a disk this
    /// could not read.
    fn installed_release(&self, target: &str) -> Result<Release, String>;
    /// Run the job, reporting through `progress`. Blocking; the server
    /// calls this off the connection threads.
    ///
    /// `Ok(Some(text))` is a sentence about what the job found, for the
    /// finish message: a filesystem check that fixed something says
    /// so, and one that wants a reboot before the disk is used says
    /// that. `Ok(None)` is a job with nothing to add beyond having
    /// finished.
    fn run(
        &self,
        kind: JobKind,
        target: &str,
        progress: &dyn Progress,
    ) -> Result<Option<String>, String>;
    /// Restart the machine, once a job has finished and someone has asked
    /// for it. `Ok` is the service manager having taken the request: the
    /// machine is on its way down, and whoever asked is told so before it
    /// gets there. An `Err` is a sentence for the page.
    fn restart(&self) -> Result<(), String>;
}

pub const INSTALL_PHASES: &[Phase] = &[
    Phase {
        r#ref: "phase.partition",
        name: "Partitioning",
    },
    Phase {
        r#ref: "phase.format",
        name: "Formatting",
    },
    Phase {
        r#ref: "phase.copy",
        name: "Copying the system",
    },
    Phase {
        r#ref: "phase.boot",
        name: "Setting up boot",
    },
];

pub const UPGRADE_PHASES: &[Phase] = &[
    Phase {
        r#ref: "phase.upgrade",
        name: "Upgrading the system",
    },
    Phase {
        r#ref: "phase.boot",
        name: "Rewriting boot files",
    },
];

pub const REPAIR_BOOT_PHASES: &[Phase] = &[Phase {
    r#ref: "phase.boot",
    name: "Rewriting boot files",
}];
pub const REPAIR_FSCK_PHASES: &[Phase] = &[Phase {
    r#ref: "phase.fsck",
    name: "Checking the filesystem",
}];
pub const REPAIR_SD_PHASES: &[Phase] = &[Phase {
    r#ref: "phase.sd",
    name: "Reseeding security descriptors",
}];

/// A machine described rather than probed: the disks it has and what
/// is on them, the controllers nothing drives, and the release the
/// medium carries. What `installerd --dry-run --inventory` reads, so
/// that a surface can be shown a machine other than the one it is
/// being worked on, and what tests supply so their result does not
/// depend on the build host's devices.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Inventory {
    pub disks: Vec<Disk>,
    pub controllers: Vec<Controller>,
    pub medium: Option<Release>,
}

/// Touches nothing. By default it probes real block devices read-only so the
/// interactive dry run is honest, and says nothing of what their filesystems
/// hold, which it has no privilege to look at.
pub struct DryRun {
    /// Milliseconds per simulated step; tests set this low.
    pub step_ms: u64,
    /// A machine to pretend to be, or `None` to probe this one.
    pub inventory: Option<Inventory>,
    /// The phase a job is to fail part way through, by the end of its ref
    /// (`copy` for `phase.copy`), so that a surface can be shown a job
    /// going wrong. `None` for one that finishes.
    pub fail_at: Option<String>,
}

impl DryRun {
    /// What the dry run claims a disk holds when nothing says otherwise:
    /// one revision behind what it claims the medium carries, so the
    /// upgrade page can be walked.
    fn pretend(version: &str) -> Release {
        Release {
            edition: "dev.peios.peios-experimental".into(),
            version: version.into(),
        }
    }
}

impl Executor for DryRun {
    fn probe_disks(&self) -> Vec<Disk> {
        match &self.inventory {
            Some(inventory) => inventory.disks.clone(),
            None => probe_sys_block(),
        }
    }

    fn unclaimed_controllers(&self) -> Vec<Controller> {
        match &self.inventory {
            Some(inventory) => inventory.controllers.clone(),
            None => contents::unclaimed_controllers(),
        }
    }

    fn phases(&self, kind: JobKind) -> &'static [Phase] {
        match kind {
            JobKind::Install => INSTALL_PHASES,
            JobKind::Upgrade => UPGRADE_PHASES,
            JobKind::RepairBoot => REPAIR_BOOT_PHASES,
            JobKind::RepairFsck => REPAIR_FSCK_PHASES,
            JobKind::RepairSd => REPAIR_SD_PHASES,
        }
    }

    fn medium_release(&self) -> Result<Release, String> {
        Ok(self
            .inventory
            .as_ref()
            .and_then(|inventory| inventory.medium.clone())
            .unwrap_or_else(|| Self::pretend("2026.8-2")))
    }

    /// What the inventory says the disk holds, where it says what is on
    /// the disk at all; the pretence otherwise.
    fn installed_release(&self, target: &str) -> Result<Release, String> {
        let described = self
            .inventory
            .iter()
            .flat_map(|inventory| &inventory.disks)
            .find(|d| d.device == target && d.read);
        match described {
            Some(disk) => match disk.system() {
                Some((_, release)) => Ok(release.clone()),
                None => Err(disk.no_system().unwrap_or("no Peios system here").into()),
            },
            None => Ok(Self::pretend("2026.8-1")),
        }
    }

    fn run(
        &self,
        kind: JobKind,
        target: &str,
        progress: &dyn Progress,
    ) -> Result<Option<String>, String> {
        progress.log(format!("dry run: no bytes will be written to {target}"));
        for phase in self.phases(kind) {
            progress.log(format!("{} ({target})…", phase.name));
            let fails = self
                .fail_at
                .as_deref()
                .is_some_and(|at| phase.r#ref.strip_prefix("phase.") == Some(at));
            // The copy is the long one of a real install, and says how far
            // it has got many times over; the rest are soon done.
            let step = if phase.r#ref == "phase.copy" { 4 } else { 25 };
            for pct in (0..=100u8).step_by(step) {
                progress.phase(phase.r#ref, pct);
                sleep(Duration::from_millis(self.step_ms));
                if fails && pct >= 50 {
                    progress.log(format!("  dry run: told to fail at {}", phase.r#ref));
                    return Err(format!(
                        "dry run: stopped part way through {}, as it was told to",
                        phase.name.to_lowercase()
                    ));
                }
            }
            progress.log(format!("{}: ok", phase.name));
        }
        Ok(None)
    }

    /// Restarts nothing: the machine a dry run is on is not one it may
    /// take down. It is taken to have gone, which is as far as a surface
    /// can tell from here.
    fn restart(&self) -> Result<(), String> {
        Ok(())
    }
}
