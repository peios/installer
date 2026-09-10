//! What the flow runs when the person says go. The flow and the
//! server know only this trait; M2 ships the dry-run implementation,
//! M3 the real one.

pub use msip_serve::Progress;

use std::thread::sleep;
use std::time::Duration;

#[derive(Debug, Clone)]
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
}

impl Disk {
    /// For prose: "Virtio disk, 8.0 GiB".
    pub fn label(&self) -> String {
        format!("{}, {}", self.model, size_text(self.size))
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
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("sr") {
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
        disks.push(Disk {
            device: format!("/dev/{name}"),
            model,
            size: sectors * 512,
            bus,
            removable: read("removable") == "1",
            medium: medium.as_deref() == Some(name.as_str()),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// `peios-experimental`: `peios-` plus os-release's VARIANT_ID.
    pub edition: String,
    /// `2026.8-7`.
    pub version: String,
}

impl Release {
    /// "Peios 2026.8-7 (experimental)".
    pub fn text(&self) -> String {
        let variant = self.edition.strip_prefix("peios-").unwrap_or(&self.edition);
        format!("Peios {} ({variant})", self.version)
    }
}

pub trait Executor: Send + Sync + 'static {
    fn probe_disks(&self) -> Vec<Disk>;
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

/// Touches nothing. Probes real block devices read-only so the disk
/// page is honest, then pretends to work.
pub struct DryRun {
    /// Milliseconds per simulated step; tests set this low.
    pub step_ms: u64,
}

impl Executor for DryRun {
    fn probe_disks(&self) -> Vec<Disk> {
        probe_sys_block()
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

    /// One revision ahead of what it claims every disk holds, so the
    /// upgrade page can be walked.
    fn medium_release(&self) -> Result<Release, String> {
        Ok(Release {
            edition: "peios-experimental".into(),
            version: "2026.8-2".into(),
        })
    }

    fn installed_release(&self, _target: &str) -> Result<Release, String> {
        Ok(Release {
            edition: "peios-experimental".into(),
            version: "2026.8-1".into(),
        })
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
            for pct in [0u8, 25, 50, 75, 100] {
                progress.phase(phase.r#ref, pct);
                sleep(Duration::from_millis(self.step_ms));
            }
            progress.log(format!("{}: ok", phase.name));
        }
        Ok(None)
    }
}
