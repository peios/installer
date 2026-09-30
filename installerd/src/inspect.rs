//! Looking into a disk's filesystems without changing them.
//!
//! The disk page says what a disk holds before anyone has chosen it, so
//! this runs against every disk the machine has, including ones the
//! person has no intention of touching. That sets the bar: opening the
//! page must not write a byte to any of them.
//!
//! A read-only mount does not promise that. ext3 and ext4 replay their
//! journal on one, and what another driver does with a volume that was
//! not shut down cleanly is its own business. So nothing here mounts a
//! partition: each is bound to a read-only loop device and *that* is
//! mounted, which leaves a driver nothing it can write to whatever it
//! would have liked to do. ext3 and ext4 are also told `noload`,
//! because on a device they cannot write a journal that wants replaying
//! is otherwise a refusal to mount.
//!
//! What is read is what a person would miss, or came for: how much is
//! in use, and whether a system lives there -- Peios, by its release;
//! another Linux, by the name it gives itself; Windows, by its kernel
//! or its boot manager.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::contents::{Partition, UNVERSIONED_PEIOS};
use crate::executor::{Disk, Release};
use crate::loopdev::LoopDevice;
use crate::real::{edition_of, version_of};

/// How a partition's `release` is read once it is mounted: the real
/// executor's `peipkg`-backed reading ([`release_at`]), or a test's.
pub type ReleaseIn<'a> = &'a dyn Fn(&Path) -> Result<Release, String>;

/// What stopped a filesystem being looked into.
pub enum Failure {
    /// This one could not be read; the next can be tried.
    Skipped(String),
    /// The place filesystems are mounted to be looked at is still
    /// occupied, so nothing more can be mounted there.
    Stuck(String),
}

impl Failure {
    pub fn why(self) -> String {
        match self {
            Failure::Skipped(why) | Failure::Stuck(why) => why,
        }
    }
}

/// Where peipkg keeps its database, under the root it manages.
const PACKAGE_DB: &str = "var/state/peipkg";

/// The Peios release on the filesystem at `root`, which may be one
/// that nothing can write to.
///
/// Which release a root holds is peipkg's to say, and peipkg cannot be
/// asked of a read-only root: its database is SQLite in write-ahead
/// mode, which will not open where it cannot make its companion files,
/// so `peipkg --root` against a read-only mount fails with "unable to
/// open database file" whatever is in it. The database is therefore
/// copied out to `scratch`, which is on a tmpfs, and peipkg is asked
/// there. The disk is read and nothing else.
pub fn release_at(root: &Path, scratch: &Path) -> Result<Release, String> {
    let text = plain(root, "usr/lib/os-release")
        .and_then(|at| std::fs::read_to_string(at).ok())
        .ok_or("no Peios system here (no usr/lib/os-release)")?;
    let edition = edition_of(&text)?;

    let _ = std::fs::remove_dir_all(scratch);
    let copy = scratch.join(PACKAGE_DB);
    std::fs::create_dir_all(&copy).map_err(|e| format!("{}: {e}", copy.display()))?;
    // The database, and its write-ahead log if the system was not shut
    // down cleanly: what was committed to the log is part of the truth.
    let mut found = false;
    for name in ["db.sqlite", "db.sqlite-wal", "db.sqlite-shm"] {
        if let Some(from) = plain(root, &format!("{PACKAGE_DB}/{name}")) {
            std::fs::copy(&from, copy.join(name))
                .map_err(|e| format!("reading {}: {e}", from.display()))?;
            found |= name == "db.sqlite";
        }
    }
    if !found {
        let _ = std::fs::remove_dir_all(scratch);
        return Err(format!(
            "no package database here (no {PACKAGE_DB}/db.sqlite)"
        ));
    }
    let out = Command::new("peipkg")
        .arg("--root")
        .arg(scratch)
        .args(["info", &edition])
        .output()
        .map_err(|e| format!("could not run peipkg: {e}"));
    let _ = std::fs::remove_dir_all(scratch);
    let out = out?;
    if !out.status.success() {
        return Err(format!(
            "{edition} is not recorded as installed here: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let version = version_of(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| format!("peipkg info {edition} did not report a version"))?;
    Ok(Release { edition, version })
}

/// Extra mount options for a filesystem this can look into, or `None`
/// for one it leaves alone. Left alone: anything that is not a
/// filesystem (swap, LUKS), and the ones whose read-only mount was not
/// tried on Peios.
pub fn mount_options(fs: &str) -> Option<&'static str> {
    match fs {
        "ext3" | "ext4" => Some("noload,"),
        "ext2" | "NTFS" | "exFAT" | "FAT" | "FAT12" | "FAT16" | "FAT32" => Some(""),
        _ => None,
    }
}

/// Look into every filesystem on `disk`, mounting at `mnt`. Returns
/// whether another disk can be looked into after this one.
pub fn read_disk(disk: &mut Disk, mnt: &Path, release: ReleaseIn) -> bool {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    // A Peios root is ext4, so the disk is only known to hold no Peios
    // system if every ext4 filesystem on it was read.
    let mut every_root_read = true;
    for partition in &mut disk.partitions {
        let Some(options) = mount_options(&partition.fs) else {
            continue;
        };
        match look(partition, options, mnt, &mounts, release) {
            Ok(()) => {}
            Err(Failure::Skipped(why)) => {
                eprintln!("installerd: {} was not read: {why}", partition.device);
                every_root_read &= partition.fs != "ext4";
            }
            Err(Failure::Stuck(why)) => {
                eprintln!(
                    "installerd: {}: {why}; reading no more disks",
                    partition.device
                );
                return false;
            }
        }
    }
    disk.read = every_root_read;
    true
}

fn look(
    partition: &mut Partition,
    options: &str,
    mnt: &Path,
    mounts: &str,
    release: ReleaseIn,
) -> Result<(), Failure> {
    // Mounted already -- the system this is running from, or something
    // an operator mounted by hand. It is read where it is.
    if let Some(at) = mount_point_of(mounts, &partition.device) {
        see(partition, Path::new(&at), release);
        return Ok(());
    }
    let device = partition.device.clone();
    mounted(Path::new(&device), options, mnt, |root| {
        see(partition, root, release)
    })
}

/// Mount the filesystem on `device` at `mnt` so that nothing can write
/// to it, let `read` look at it there, and unmount it.
///
/// It is not `device` that is mounted but a read-only loop device bound
/// to it, and the policy is the one that synthesises a descriptor in
/// memory for whatever has none and writes nothing: a filesystem from
/// another system carries no descriptors, and one from Peios keeps its
/// own.
pub fn mounted<T>(
    device: &Path,
    options: &str,
    mnt: &Path,
    read: impl FnOnce(&Path) -> T,
) -> Result<T, Failure> {
    std::fs::create_dir_all(mnt).map_err(|e| Failure::Skipped(e.to_string()))?;
    let readonly = LoopDevice::attach_readonly(device)
        .map_err(|e| Failure::Skipped(format!("no read-only loop device: {e}")))?;
    let mount = Command::new("mount")
        .arg("--read-only")
        .args(["-o", &format!("{options}policy=synth-ephemeral")])
        .arg(readonly.path())
        .arg(mnt)
        .output()
        .map_err(|e| Failure::Skipped(format!("could not run mount: {e}")))?;
    if !mount.status.success() {
        return Err(Failure::Skipped(
            String::from_utf8_lossy(&mount.stderr).trim().to_string(),
        ));
    }
    let found = read(mnt);
    match Command::new("umount").arg(mnt).output() {
        Ok(out) if out.status.success() => Ok(found),
        Ok(out) => Err(Failure::Stuck(format!(
            "could not unmount {}: {}",
            mnt.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))),
        Err(e) => Err(Failure::Stuck(format!("could not run umount: {e}"))),
    }
}

/// Record what the filesystem mounted at `root` has to say.
fn see(partition: &mut Partition, root: &Path, release: ReleaseIn) {
    partition.used = used_bytes(root);
    let (holds, peios) = what_holds(root, release);
    partition.holds = holds;
    partition.peios = peios;
}

/// Where `device` is mounted, by `/proc/mounts`.
pub fn mount_point_of(mounts: &str, device: &str) -> Option<String> {
    mounts.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let (source, target) = (fields.next()?, fields.next()?);
        // The kernel writes a space in a path as \040.
        (source == device).then(|| target.replace("\\040", " "))
    })
}

/// Bytes in use on the filesystem at `path`.
pub fn used_bytes(path: &Path) -> Option<u64> {
    room(path).map(|(used, _)| used)
}

/// Bytes in use on the filesystem at `path`, and the size of the blocks
/// it hands out.
pub fn room(path: &Path) -> Option<(u64, u64)> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    // SAFETY: `path` is a NUL-terminated string and `stat` is room for
    // what statvfs writes; it is read only once statvfs says it wrote it.
    let stat = unsafe {
        if libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) != 0 {
            return None;
        }
        stat.assume_init()
    };
    let block = stat.f_frsize as u64;
    Some((
        (stat.f_blocks as u64).saturating_sub(stat.f_bfree as u64) * block,
        block,
    ))
}

/// `relative` under `root`, if every step of the way is really there.
///
/// A symbolic link on a disk being looked at points wherever whoever
/// made the disk liked, and followed from here it resolves against
/// *this* system: `etc/os-release -> /usr/lib/os-release` would read
/// the medium's own and report every Linux disk as Peios. So no link
/// is followed, at any step.
pub fn plain(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut at = root.to_path_buf();
    for step in Path::new(relative).components() {
        let Component::Normal(name) = step else {
            return None;
        };
        at.push(name);
        if std::fs::symlink_metadata(&at)
            .ok()?
            .file_type()
            .is_symlink()
        {
            return None;
        }
    }
    Some(at)
}

/// What an os-release calls the system it describes.
pub fn pretty_name(os_release: &str) -> Option<String> {
    let value = |key: &str| {
        os_release
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|v| !v.is_empty())
    };
    value("PRETTY_NAME=").or_else(|| value("NAME="))
}

/// Where Windows keeps its kernel. NTFS keeps the case a name was
/// created with and the driver matches it exactly, so both spellings
/// Windows has used are tried.
const WINDOWS_KERNEL: &[&str] = &[
    "Windows/System32/ntoskrnl.exe",
    "WINDOWS/system32/ntoskrnl.exe",
    "Windows/system32/ntoskrnl.exe",
    "WINDOWS/System32/ntoskrnl.exe",
];
const WINDOWS_BOOT_MANAGER: &str = "EFI/Microsoft/Boot/bootmgfw.efi";

/// What the filesystem at `root` holds that has a name, and the Peios
/// release on it when that is what it is.
pub fn what_holds(root: &Path, release: ReleaseIn) -> (Option<String>, Option<Release>) {
    let os_release = ["usr/lib/os-release", "etc/os-release"]
        .iter()
        .find_map(|at| plain(root, at))
        .and_then(|at| std::fs::read_to_string(at).ok());
    if let Some(text) = os_release {
        if edition_of(&text).is_ok() {
            return match release(root) {
                Ok(release) => (Some(release.text()), Some(release)),
                // A Peios tree whose package database will not say which
                // release it is: named, and not offered as one to upgrade.
                Err(_) => (Some(UNVERSIONED_PEIOS.into()), None),
            };
        }
        if let Some(name) = pretty_name(&text) {
            return (Some(name), None);
        }
    }
    if WINDOWS_KERNEL.iter().any(|at| plain(root, at).is_some()) {
        return (Some("Windows".into()), None);
    }
    if plain(root, WINDOWS_BOOT_MANAGER).is_some() {
        return (Some("Windows Boot Manager".into()), None);
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("installerd-look-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(root: &Path, relative: &str, text: &str) {
        let at = root.join(relative);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }

    fn no_release(_: &Path) -> Result<Release, String> {
        Err("not asked for".into())
    }

    #[test]
    fn only_filesystems_whose_read_was_tried_are_looked_into() {
        assert_eq!(mount_options("ext4"), Some("noload,"));
        assert_eq!(mount_options("NTFS"), Some(""));
        assert_eq!(mount_options("FAT32"), Some(""));
        assert_eq!(mount_options("swap"), None);
        assert_eq!(mount_options("LUKS"), None);
        assert_eq!(mount_options(""), None);
    }

    #[test]
    fn a_mounted_device_is_found_by_its_own_name_only() {
        let mounts = "/dev/sda2 / ext4 rw 0 0\n/dev/sda1 /boot\\040files vfat ro 0 0\n";
        assert_eq!(mount_point_of(mounts, "/dev/sda2").as_deref(), Some("/"));
        assert_eq!(
            mount_point_of(mounts, "/dev/sda1").as_deref(),
            Some("/boot files")
        );
        assert_eq!(mount_point_of(mounts, "/dev/sda"), None);
        assert_eq!(mount_point_of(mounts, "/dev/sda22"), None);
    }

    #[test]
    fn a_peios_system_is_named_by_its_release() {
        let root = tree("peios");
        put(
            &root,
            "usr/lib/os-release",
            "ID=peios\nVARIANT_ID=experimental\n",
        );
        let said = what_holds(&root, &|_| {
            Ok(Release {
                edition: "dev.peios.peios-experimental".into(),
                version: "2026.8-7".into(),
            })
        });
        assert_eq!(said.0.as_deref(), Some("Peios 2026.8-7 (experimental)"));
        assert_eq!(said.1.unwrap().version, "2026.8-7");
        // One whose package database will not answer is still Peios.
        assert_eq!(what_holds(&root, &no_release), (Some("Peios".into()), None));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn another_linux_is_named_as_it_names_itself() {
        let root = tree("linux");
        put(
            &root,
            "etc/os-release",
            "NAME=\"Debian GNU/Linux\"\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nID=debian\n",
        );
        assert_eq!(
            what_holds(&root, &no_release),
            (Some("Debian GNU/Linux 13 (trixie)".into()), None)
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn windows_is_known_by_its_kernel_and_its_boot_manager() {
        let system = tree("windows");
        put(&system, "Windows/System32/ntoskrnl.exe", "");
        assert_eq!(
            what_holds(&system, &no_release),
            (Some("Windows".into()), None)
        );
        let esp = tree("esp");
        put(&esp, "EFI/Microsoft/Boot/bootmgfw.efi", "");
        assert_eq!(
            what_holds(&esp, &no_release),
            (Some("Windows Boot Manager".into()), None)
        );
        let data = tree("data");
        put(&data, "Photos/2019/beach.jpg", "");
        assert_eq!(what_holds(&data, &no_release), (None, None));
        for dir in [system, esp, data] {
            std::fs::remove_dir_all(dir).ok();
        }
    }

    /// The link is the usual one on a merged-/usr Linux, made absolute:
    /// followed, it would read whatever system this test runs on.
    #[test]
    fn a_link_on_the_disk_is_never_followed() {
        let root = tree("link");
        std::fs::create_dir_all(root.join("etc")).unwrap();
        std::os::unix::fs::symlink("/usr/lib/os-release", root.join("etc/os-release")).unwrap();
        assert_eq!(plain(&root, "etc/os-release"), None);
        assert_eq!(what_holds(&root, &no_release), (None, None));
        // Nor a link part of the way there.
        let elsewhere = tree("elsewhere");
        put(&elsewhere, "System32/ntoskrnl.exe", "");
        std::os::unix::fs::symlink(&elsewhere, root.join("Windows")).unwrap();
        assert_eq!(what_holds(&root, &no_release), (None, None));
        assert_eq!(plain(&root, "../etc/passwd"), None);
        for dir in [root, elsewhere] {
            std::fs::remove_dir_all(dir).ok();
        }
    }

    /// What can be refused before peipkg is asked anything is.
    #[test]
    fn a_release_is_read_only_from_a_peios_tree_with_a_package_database() {
        let scratch = tree("scratch");
        let other = tree("other");
        put(&other, "usr/lib/os-release", "ID=debian\n");
        assert!(
            release_at(&other, &scratch)
                .unwrap_err()
                .contains("not a Peios system")
        );
        let bare = tree("bare");
        assert!(
            release_at(&bare, &scratch)
                .unwrap_err()
                .contains("no usr/lib/os-release")
        );
        put(
            &bare,
            "usr/lib/os-release",
            "ID=peios\nVARIANT_ID=experimental\n",
        );
        assert!(
            release_at(&bare, &scratch)
                .unwrap_err()
                .contains("no package database")
        );
        // A database that is a link out of the disk is no database.
        std::fs::create_dir_all(bare.join("var/state/peipkg")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", bare.join("var/state/peipkg/db.sqlite")).unwrap();
        assert!(
            release_at(&bare, &scratch)
                .unwrap_err()
                .contains("no package database")
        );
        // Nothing is left behind of what was copied to be read.
        assert!(!scratch.join("var").exists());
        for dir in [scratch, other, bare] {
            std::fs::remove_dir_all(dir).ok();
        }
    }

    #[test]
    fn what_is_in_use_is_read_from_the_filesystem() {
        assert!(used_bytes(&std::env::temp_dir()).is_some());
        assert_eq!(used_bytes(Path::new("/nonexistent/installerd")), None);
    }
}
