//! The real executor: the phases of `peios-install(8)` moved into the
//! engine, reporting progress instead of echoing.
//!
//! Every step here was a paragraph of `pkgs/peios-install/src/
//! peios-install.sh`, and the reasoning that made each one what it is
//! has been carried across with it. Two things are deliberately
//! different, and both are consequences of decisions recorded on
//! PEI-52:
//!
//! **The copy source is the pristine image, not the running root.**
//! The script copies `/` entry by entry, excluding mountpoints,
//! because on a live system `/` is an overlay whose lower is the
//! shipped squashfs. That drags the live session's writes along with
//! it: logs, whatever a shell touched, and the development account
//! lpsd-first-account provisioned at boot. Here the squashfs is
//! mounted from the medium and copied directly, so what lands on the
//! disk is exactly what the image shipped.
//!
//! **First-account retirement is a property of the image, not a step
//! here.** The script removed the `lpsd-first-account` service from the
//! *live* registry so its merged-view copy could not carry it. Copying
//! the pristine image cannot produce that situation — registry state is
//! not shipped as a hive at all, but as `.reg` seeds under
//! `/lcl/policy/` that peinit drains on first boot — but it does copy
//! the seeds themselves, so an installed system used to provision the
//! development account and its public password exactly as a live image
//! does, and then crash the provisioner against that account on every
//! subsequent boot.
//!
//! The edition now says where each seed belongs and peiso stages it
//! accordingly, so all this has to do is act on that: delete the
//! medium's queue from the target and promote the installed machine's.
//! See [`Real::settle_seed_queues`].

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::executor::{Disk, Executor, JobKind, Phase, Progress, Release};
use crate::executor::{
    INSTALL_PHASES, REPAIR_BOOT_PHASES, REPAIR_FSCK_PHASES, REPAIR_SD_PHASES, UPGRADE_PHASES,
};

/// Written to the root inode at format time, and seeded onto the ESP's
/// mount. Kept identical to the script's and to live-boot's, because a
/// tree whose top-level descriptor differs from the one the image was
/// built against is administrable by a different set of principals.
const ROOT_SDDL: &str =
    "O:SYG:SYD:(A;OICI;GA;;;SY)(A;OICI;GA;;;BA)(A;OICI;GRGX;;;WD)(A;OICIIO;GA;;;S-1-3-0)";

const ESP_SIZE: &str = "512M";
const MEDIUM_REPO: &str = "peios-medium";
const CMDLINE_TEMPLATE_REL: &str = "usr/share/disk-boot/cmdline";
const LIVE_BOOT_PACKAGE: &str = "dev.peios.live-boot";
const LIVE_BOOT_IRF_PACKAGE: &str = "dev.peios.live-boot-irf";
const DISK_BOOT_PACKAGE: &str = "dev.peios.disk-boot";

pub struct Real {
    /// Where the boot medium is mounted; live-boot mount-moves it here.
    pub medium: PathBuf,
    /// Scratch, on a tmpfs so nothing here can end up on the target.
    pub work: PathBuf,
    /// Pass --force to `part`, to replace a table it did not create.
    pub force: bool,
}

impl Default for Real {
    fn default() -> Real {
        Real {
            medium: PathBuf::from("/media/peios"),
            work: PathBuf::from("/run/peios-install"),
            force: false,
        }
    }
}

impl Real {
    fn esp_mnt(&self) -> PathBuf {
        self.work.join("esp")
    }
    fn root_mnt(&self) -> PathBuf {
        self.work.join("root")
    }
    fn lower_mnt(&self) -> PathBuf {
        self.work.join("lower")
    }
    /// Where a disk is mounted read-only to be looked at, which is a
    /// different place from where it is mounted to be changed so that
    /// an inspection can never be mistaken for the start of a job.
    fn inspect_mnt(&self) -> PathBuf {
        self.work.join("inspect")
    }

    /// Run a command, sending each line of its output to the log and
    /// turning a non-zero exit into an error naming what failed.
    fn run(&self, p: &dyn Progress, program: &str, args: &[&str]) -> Result<(), String> {
        match self.run_status(p, program, args)? {
            0 => Ok(()),
            c => Err(format!("{program} failed (exit {c})")),
        }
    }

    /// Run a command, log its output, and hand back the exit code
    /// without judging it. For the tools whose exit status is a report
    /// rather than a verdict -- e2fsck says what it did in its exit
    /// code, and 1 there is a success.
    fn run_status(&self, p: &dyn Progress, program: &str, args: &[&str]) -> Result<i32, String> {
        p.log(format!("$ {program} {}", args.join(" ")));
        let out = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("could not run {program}: {e}"))?;
        for stream in [&out.stdout, &out.stderr] {
            for line in String::from_utf8_lossy(stream).lines() {
                if !line.trim().is_empty() {
                    p.log(format!("  {line}"));
                }
            }
        }
        out.status
            .code()
            .ok_or_else(|| format!("{program} was killed"))
    }

    /// Whether `name` is installed in the peipkg root at `root`.
    ///
    /// `peipkg info` answers with its exit status; its output is not
    /// wanted here and is not logged, so a missing package is a quiet
    /// `false` rather than a red line in the log for a question this
    /// only asked to decide what to do next.
    fn package_installed(&self, root: &str, name: &str) -> bool {
        Command::new("peipkg")
            .args(["--root", root, "info", name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn capture(&self, program: &str, args: &[&str]) -> Result<String, String> {
        let out = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("could not run {program}: {e}"))?;
        if !out.status.success() {
            return Err(format!("{program} failed"));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// The release a peipkg root holds: its edition from the os-release
    /// the edition package wrote, its version from peipkg's own record
    /// of that package. `root` is `/` for the running medium.
    ///
    /// Derived the way `upgrade-peios` derives it, so the two agree on
    /// which package an edition is.
    fn release_in(root: &Path) -> Result<Release, String> {
        let os_release = root.join("usr/lib/os-release");
        let text = std::fs::read_to_string(&os_release)
            .map_err(|_| "no Peios system here (no usr/lib/os-release)".to_string())?;
        let edition = edition_of(&text)?;
        let mut cmd = Command::new("peipkg");
        if root != Path::new("/") {
            cmd.arg("--root").arg(root);
        }
        let out = cmd
            .args(["info", &edition])
            .output()
            .map_err(|e| format!("could not run peipkg: {e}"))?;
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

    fn is_block_device(path: &str) -> bool {
        use std::os::unix::fs::FileTypeExt;
        std::fs::metadata(path)
            .map(|m| m.file_type().is_block_device())
            .unwrap_or(false)
    }

    /// Whether any partition of `disk` (or the disk itself) is mounted,
    /// given the contents of /proc/mounts. Pure /proc parsing, as in the
    /// script: the image ships no grep.
    pub fn mounted_in(mounts: &str, disk: &str) -> Option<String> {
        for line in mounts.lines() {
            let mut fields = line.split_whitespace();
            let (Some(source), Some(target)) = (fields.next(), fields.next()) else {
                continue;
            };
            let is_this_disk = source == disk
                || (source.len() > disk.len()
                    && source.starts_with(disk)
                    && source[disk.len()..]
                        .trim_start_matches('p')
                        .chars()
                        .all(|c| c.is_ascii_digit())
                    && source[disk.len()..].chars().any(|c| c.is_ascii_digit()));
            if is_this_disk {
                return Some(format!("{source} is mounted on {target}"));
            }
        }
        None
    }

    fn anything_mounted_on(disk: &str) -> Option<String> {
        let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
        Self::mounted_in(&mounts, disk)
    }

    /// The partition nodes of a disk, probing for the suffix the kernel
    /// actually created: sd*/vd* number directly (vdb1) while
    /// nvme*/mmcblk* interpose a `p` (nvme0n1p1).
    pub fn partitions_of(
        disk: &str,
        exists: impl Fn(&str) -> bool,
    ) -> Result<(String, String), String> {
        for prefix in ["", "p"] {
            let esp = format!("{disk}{prefix}1");
            let root = format!("{disk}{prefix}2");
            if exists(&esp) && exists(&root) {
                return Ok((esp, root));
            }
        }
        Err(format!(
            "{disk}1 and {disk}p1 are both absent; the kernel did not pick up the table"
        ))
    }

    fn partitions(disk: &str) -> Result<(String, String), String> {
        Self::partitions_of(disk, Self::is_block_device)
    }

    fn mount(&self, p: &dyn Progress, args: &[&str]) -> Result<(), String> {
        self.run(p, "mount", args)
    }

    fn unmount_all(&self, p: &dyn Progress) {
        for m in [self.esp_mnt(), self.root_mnt(), self.lower_mnt()] {
            let _ = self.run(p, "umount", &[m.to_string_lossy().as_ref()]);
        }
    }

    fn partition(&self, p: &dyn Progress, disk: &str) -> Result<(), String> {
        if !Self::is_block_device(disk) {
            return Err(format!(
                "{disk} is not a block device the kernel knows about"
            ));
        }
        if let Some(what) = Self::anything_mounted_on(disk) {
            return Err(format!("refusing to partition {disk}: {what}"));
        }
        p.phase("phase.partition", 10);
        // `part` does its own refusing — not a partition, nothing
        // mounted, no table it did not create without --force — so this
        // does not re-implement those checks, it just does not swallow
        // them. Exit 3 is specifically a refusal, worth its own message.
        let mut create: Vec<&str> = vec!["create", disk, "--yes"];
        if self.force {
            create.push("--force");
        }
        if let Err(e) = self.run(p, "part", &create) {
            return Err(if e.contains("exit 3") {
                format!("{disk} carries a partition table part will not replace")
            } else {
                e
            });
        }
        p.phase("phase.partition", 55);
        self.run(
            p,
            "part",
            &[
                "add",
                disk,
                "--size",
                ESP_SIZE,
                "--type",
                "esp",
                "--name",
                "EFI system partition",
                "--yes",
            ],
        )?;
        p.phase("phase.partition", 85);
        self.run(
            p,
            "part",
            &[
                "add",
                disk,
                "--size",
                "max",
                "--type",
                "linux",
                "--name",
                "Peios root",
                "--yes",
            ],
        )?;
        p.phase("phase.partition", 100);
        Ok(())
    }

    fn format(&self, p: &dyn Progress, esp: &str, root: &str) -> Result<(), String> {
        p.phase("phase.format", 20);
        self.run(p, "mkfs.vfat", &["-F", "32", "-n", "PEIOSESP", esp])?;
        p.phase("phase.format", 60);
        // -E root_sddl= writes the descriptor to the root inode at format
        // time, so the filesystem is administrable from the instant it
        // exists and mounts under deny-missing with no mount-level
        // template. -F because this must not stop to ask: the person
        // already confirmed, and there is nobody at a prompt.
        self.run(
            p,
            "mke2fs",
            &[
                "-qF",
                "-t",
                "ext4",
                "-L",
                "peios-root",
                "-E",
                &format!("root_sddl={ROOT_SDDL}"),
                root,
            ],
        )?;
        p.phase("phase.format", 100);
        Ok(())
    }

    fn mount_target(&self, p: &dyn Progress, esp: &str, root: &str) -> Result<(), String> {
        std::fs::create_dir_all(self.esp_mnt()).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(self.root_mnt()).map_err(|e| e.to_string())?;
        // deny-missing on the target, matching how it will be mounted at
        // boot: the descriptor is already on it, nothing to synthesise.
        self.mount(
            p,
            &[
                "-o",
                "policy=deny-missing",
                root,
                self.root_mnt().to_string_lossy().as_ref(),
            ],
        )?;
        // FAT holds no descriptors and never can, so its policy comes
        // from the mount. Ephemeral: synthesise in memory, write nothing.
        self.mount(
            p,
            &[
                "-o",
                "policy=synth-ephemeral",
                "--synth-sddl",
                ROOT_SDDL,
                esp,
                self.esp_mnt().to_string_lossy().as_ref(),
            ],
        )
    }

    /// Mount the shipped image read-only and copy it onto the target.
    fn copy_system(&self, p: &dyn Progress) -> Result<(), String> {
        let image = self.medium.join("rootfs.squashfs");
        if !image.exists() {
            return Err(format!(
                "{} is not there; the install medium must be mounted to copy from",
                image.display()
            ));
        }
        std::fs::create_dir_all(self.lower_mnt()).map_err(|e| e.to_string())?;

        // Our own loop device, not `mount -o loop`: the live system
        // booted from this very file, so a loop device is already bound
        // to it and mount would reuse that one, whose superblock is
        // already mounted where we cannot reach it (see loopdev).
        let loopdev = crate::loopdev::LoopDevice::attach_readonly(&image)
            .map_err(|e| format!("could not attach a loop device to {}: {e}", image.display()))?;
        p.log(format!(
            "{} attached to {}",
            image.display(),
            loopdev.path().display()
        ));

        // `policy=synth-ephemeral` is what live-boot mounts the lower
        // with: the image ships no descriptors, so KACS synthesises one
        // per inode in memory. That is what `cp -a` then carries onto
        // the target -- the same descriptors the merged-view copy
        // carried before.
        self.mount(
            p,
            &[
                "-o",
                "ro,policy=synth-ephemeral",
                "-t",
                "squashfs",
                loopdev.path().to_string_lossy().as_ref(),
                self.lower_mnt().to_string_lossy().as_ref(),
            ],
        )?;

        let mut entries: Vec<PathBuf> = std::fs::read_dir(self.lower_mnt())
            .map_err(|e| format!("cannot read the mounted image: {e}"))?
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        if entries.is_empty() {
            return Err("the mounted image is empty".into());
        }

        // -a and nothing else: owner, DACL, SACL, timestamps, links,
        // exec and the xattr namespaces, all required — so anything that
        // cannot be carried across is an error rather than a silent
        // downgrade. -x for nested mounts, though a freshly-mounted
        // squashfs has none: the image ships /proc, /sys, /dev and /run
        // as the empty directories the boot will mount over.
        let total = entries.len();
        for (i, entry) in entries.iter().enumerate() {
            p.phase("phase.copy", ((i * 100) / total) as u8);
            let name = entry
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            p.log(format!("copying /{name}"));
            self.run(
                p,
                "cp",
                &[
                    "-ax",
                    entry.to_string_lossy().as_ref(),
                    &format!("{}/", self.root_mnt().display()),
                ],
            )
            .map_err(|e| format!("copying /{name}: {e}"))?;
        }
        p.phase("phase.copy", 100);
        self.run(p, "umount", &[self.lower_mnt().to_string_lossy().as_ref()])
    }

    /// Point the target's registry-seed queues at a machine rather than
    /// a medium.
    ///
    /// The copy is verbatim, so the target arrives carrying every queue
    /// the image shipped — including the two peiso stages for exactly
    /// this moment:
    ///
    /// - `autoapply.live.d/` is what only the boot medium applies. Its
    ///   contents are things like the development account whose password
    ///   ships in the image and is therefore public. Deleted.
    /// - `autoapply.install.d/` is what only an installed machine
    ///   applies, and peinit does not drain it — first-boot setup taking
    ///   the console on the live image would take it from the installer.
    ///   Promoted into `autoapply.d/`, which peinit does drain.
    ///
    /// Both are unconditional and neither is an error when absent: an
    /// edition that names no live or install seeds ships no such
    /// directory, and there is nothing to do.
    ///
    /// Deliberately not a phase of its own. It is the same act as the
    /// package swap below — this tree is no longer a medium — and a
    /// progress bar that flashed past for two directory operations would
    /// say less than the log line does.
    fn settle_seed_queues(&self, p: &dyn Progress) -> Result<(), String> {
        let policy = self.root_mnt().join("lcl/policy");
        let live = policy.join("autoapply.live.d");
        if live.is_dir() {
            p.log("removing the medium's own registry seeds".into());
            std::fs::remove_dir_all(&live)
                .map_err(|e| format!("removing {}: {e}", live.display()))?;
        }

        let staged = policy.join("autoapply.install.d");
        let Ok(entries) = std::fs::read_dir(&staged) else {
            return Ok(());
        };
        let queue = policy.join("autoapply.d");
        std::fs::create_dir_all(&queue)
            .map_err(|e| format!("creating {}: {e}", queue.display()))?;
        for entry in entries.flatten() {
            let name = entry.file_name();
            p.log(format!(
                "staging {} for the first boot",
                name.to_string_lossy()
            ));
            // Renamed rather than copied: same filesystem, so it is one
            // operation that cannot half-happen, and it empties the
            // source as it goes.
            std::fs::rename(entry.path(), queue.join(&name))
                .map_err(|e| format!("staging {}: {e}", name.to_string_lossy()))?;
        }
        std::fs::remove_dir_all(&staged).map_err(|e| format!("removing {}: {e}", staged.display()))
    }

    /// Turn a copy of the medium into a system that boots from disk:
    /// the package swap, then the boot files.
    fn make_bootable(&self, p: &dyn Progress, root_part: &str) -> Result<(), String> {
        p.phase("phase.boot", 5);
        self.settle_seed_queues(p)?;
        self.swap_boot_packages(p)?;
        self.write_boot_files(p, root_part)
    }

    /// Rewrite the boot files of a system already on the disk.
    ///
    /// Only the files: the kernel and the initramfs tree are packages
    /// on the disk, and this regenerates the cmdline, the initramfs
    /// image and the UKI from them. It does not reinstall disk-boot
    /// from the medium -- a machine upgraded since it was installed
    /// may hold a newer one than the medium does, and touching the
    /// installed system's repository list is not what "repair boot
    /// files" asked for.
    ///
    /// The one disk that still wants the swap is a half-finished
    /// install, where the copy landed and boot setup died. That disk
    /// still carries live-boot, and that is exactly the test.
    fn repair_boot(&self, p: &dyn Progress, root_part: &str) -> Result<(), String> {
        let root_s = self.root_mnt().to_string_lossy().into_owned();
        p.phase("phase.boot", 5);
        if self.package_installed(&root_s, LIVE_BOOT_PACKAGE) {
            p.log("the disk still carries the medium's boot package: finishing the install".into());
            self.settle_seed_queues(p)?;
            self.swap_boot_packages(p)?;
        } else {
            p.log("disk-boot is in place; regenerating the boot files from the disk".into());
        }
        self.write_boot_files(p, root_part)
    }

    /// Move the mounted system to the release this medium carries.
    ///
    /// The edition first, then everything else. The edition step is
    /// `upgrade-peios`' first step done here rather than by it, for one
    /// reason: a medium is a read-only artifact whose repository index
    /// is fixed at manufacture, so an image older than the trusted age
    /// is stale and cannot become fresh, and `--allow-stale` is how the
    /// one operation that knows that is expected says so. upgrade-peios
    /// has no way to pass it on. Its second half -- staging the new
    /// release's seeds for the next boot -- is exactly what
    /// `--seeds-only` runs, so that part is still its.
    ///
    /// The second `peipkg upgrade` names nothing and so reconciles
    /// every root under the target, the initramfs included, against
    /// the medium: the edition floors its dependencies rather than
    /// pinning them, so upgrading it alone brings forward only what its
    /// floors demand, and the point of an upgrade from a fresh build is
    /// to see the whole of it.
    fn upgrade(&self, p: &dyn Progress) -> Result<Release, String> {
        let root = self.root_mnt();
        let root_s = root.to_string_lossy().into_owned();
        p.phase("phase.upgrade", 2);
        let have = Self::release_in(&root)?;
        let carry = Self::release_in(Path::new("/"))?;
        if have.edition != carry.edition {
            return Err(format!(
                "{} cannot be moved to {}",
                have.edition, carry.edition
            ));
        }
        if !crate::version::is_newer(&carry.version, &have.version)? {
            return Err(format!(
                "nothing to do: the disk holds {} and this medium carries {}",
                have.version, carry.version
            ));
        }
        p.log(format!("upgrading {} to {}", have.text(), carry.text()));
        self.run(
            p,
            "peipkg",
            &["--root", &root_s, "repo", "add", MEDIUM_REPO],
        )?;
        p.phase("phase.upgrade", 10);
        let result = (|| {
            self.run(
                p,
                "peipkg",
                &[
                    "--root",
                    &root_s,
                    "upgrade",
                    &carry.edition,
                    "--bypass-alternate-upgrade",
                    "--yes",
                    "--allow-stale",
                ],
            )?;
            p.phase("phase.upgrade", 45);
            self.run(p, "upgrade-peios", &["--root", &root_s, "--seeds-only"])?;
            p.phase("phase.upgrade", 55);
            self.run(
                p,
                "peipkg",
                &["--root", &root_s, "upgrade", "--yes", "--allow-stale"],
            )?;
            p.phase("phase.upgrade", 95);
            Ok(())
        })();
        // The medium will not be there when the target boots, so its
        // repository must not stay configured whether or not the
        // upgrade got through.
        let removed = self.run(
            p,
            "peipkg",
            &["--root", &root_s, "repo", "remove", MEDIUM_REPO],
        );
        result.and(removed)?;
        let now = Self::release_in(&root)?;
        p.phase("phase.upgrade", 100);
        Ok(now)
    }

    /// Swap live-boot for disk-boot, from the medium's repository.
    fn swap_boot_packages(&self, p: &dyn Progress) -> Result<(), String> {
        let root_s = self.root_mnt().to_string_lossy().into_owned();
        let irf_root = format!("{root_s}/boot/initramfs");
        self.run(
            p,
            "peipkg",
            &["--root", &root_s, "repo", "add", MEDIUM_REPO],
        )?;
        self.run(
            p,
            "peipkg",
            &["--root", &root_s, "uninstall", LIVE_BOOT_PACKAGE, "--yes"],
        )?;
        self.run(
            p,
            "peipkg",
            &[
                "--root",
                &irf_root,
                "uninstall",
                LIVE_BOOT_IRF_PACKAGE,
                "--yes",
            ],
        )?;
        self.run(
            p,
            "peipkg",
            &[
                "--root",
                &root_s,
                "install",
                DISK_BOOT_PACKAGE,
                "--yes",
                "--allow-stale",
            ],
        )?;
        self.run(
            p,
            "peipkg",
            &["--root", &root_s, "repo", "remove", MEDIUM_REPO],
        )
    }

    /// Write the cmdline, rebuild the initramfs and place the UKI, all
    /// from what is on the disk.
    fn write_boot_files(&self, p: &dyn Progress, root_part: &str) -> Result<(), String> {
        let root = self.root_mnt();
        let root_s = root.to_string_lossy().into_owned();
        let irf_root = format!("{root_s}/boot/initramfs");

        p.phase("phase.boot", 45);
        // The kernel has no udev here, so /dev/disk/by-uuid is empty and
        // the UUID has to be read from the device itself.
        let uuid = self
            .capture("lsblk", &["-n", "-o", "UUID", root_part])?
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .ok_or_else(|| format!("could not read the UUID of {root_part}"))?;

        let template_path = root.join(CMDLINE_TEMPLATE_REL);
        let template = std::fs::read_to_string(&template_path).map_err(|_| {
            format!("the target has no {CMDLINE_TEMPLATE_REL}; the disk-boot install did not land")
        })?;
        std::fs::create_dir_all(root.join("lcl/etc/boot")).map_err(|e| e.to_string())?;
        let cmdline = format!("{} root=UUID={}\n", template.trim(), uuid);
        std::fs::write(root.join("lcl/etc/boot/cmdline"), &cmdline).map_err(|e| e.to_string())?;
        p.log(format!("cmdline: {}", cmdline.trim()));

        // The swap is verified rather than assumed: an installer that
        // silently half does its job is worse than one that stops.
        let hooks = Path::new(&irf_root).join("usr/libexec/prelude/hooks.d");
        if !hooks.join("mount-root-disk.sh").is_file() {
            return Err("the target's initramfs has no disk-boot root-mount hook".into());
        }
        if hooks.join("mount-root.sh").is_file() {
            return Err("the target's initramfs still carries live-boot's root-mount hook".into());
        }

        p.phase("phase.boot", 60);
        let target_initramfs = format!("{root_s}/system/boot/initramfs.cpio.zst");
        self.run(
            p,
            "mkirf",
            &[
                &irf_root,
                &target_initramfs,
                "--compress",
                "zstd",
                "--exclude",
                "var/state/peipkg",
                "--exclude",
                "lcl/conf/peipkg",
            ],
        )?;

        p.phase("phase.boot", 85);
        let kernel = self.kernel_path(&root_s)?;
        std::fs::create_dir_all(self.esp_mnt().join("EFI/BOOT")).map_err(|e| e.to_string())?;
        self.run(
            p,
            "mkuki",
            &[
                "--kernel",
                &kernel,
                "--initramfs",
                &target_initramfs,
                "--cmdline-file",
                &format!("{root_s}/lcl/etc/boot/cmdline"),
                "--out",
                &format!("{}/EFI/BOOT/BOOTX64.EFI", self.esp_mnt().display()),
            ],
        )?;
        self.run(p, "sync", &[])?;
        p.phase("phase.boot", 100);
        Ok(())
    }

    /// The kernel to bundle into the UKI, found on the TARGET at
    /// `usr/lib/modules/*/vmlinuz-*`. Globbed rather than hardcoded --
    /// the release is in both the directory and the filename -- and an
    /// image carrying two is a refusal rather than a guess.
    fn kernel_path(&self, root: &str) -> Result<String, String> {
        let modules = format!("{root}/usr/lib/modules");
        let mut found: Vec<PathBuf> = Vec::new();
        for release in std::fs::read_dir(&modules)
            .map_err(|e| format!("cannot read {modules}: {e}"))?
            .flatten()
        {
            let Ok(entries) = std::fs::read_dir(release.path()) else {
                continue;
            };
            for f in entries.flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                if name.starts_with("vmlinuz-") && f.path().is_file() {
                    found.push(f.path());
                }
            }
        }
        match found.len() {
            0 => Err(format!("no kernel found under {modules}/*/vmlinuz-*")),
            1 => Ok(found.remove(0).to_string_lossy().into_owned()),
            n => Err(format!(
                "{n} kernels found under {modules}; refusing to guess"
            )),
        }
    }
}

impl Executor for Real {
    fn probe_disks(&self) -> Vec<Disk> {
        crate::executor::probe_sys_block()
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
        // The medium is the running system: what it carries is what it
        // booted, and its package database says which release that is.
        Self::release_in(Path::new("/"))
    }

    /// Mount the disk's root read-only, read it, unmount it. Read-only
    /// so that looking at a disk changes nothing on it -- not even a
    /// journal replay -- and so that the page can be reached, and
    /// backed out of, without the disk having been touched.
    fn installed_release(&self, target: &str) -> Result<Release, String> {
        let (_esp, root) = Self::partitions(target)?;
        if let Some(what) = Self::anything_mounted_on(target) {
            return Err(format!("{what}; unmount it first"));
        }
        let mnt = self.inspect_mnt();
        std::fs::create_dir_all(&mnt).map_err(|e| e.to_string())?;
        let mnt_s = mnt.to_string_lossy().into_owned();
        let mounted = Command::new("mount")
            .args(["--read-only", "-o", "policy=deny-missing", &root, &mnt_s])
            .output()
            .map_err(|e| format!("could not run mount: {e}"))?;
        if !mounted.status.success() {
            return Err(format!(
                "could not read {root}: {}",
                String::from_utf8_lossy(&mounted.stderr).trim()
            ));
        }
        let release = Self::release_in(&mnt);
        let _ = Command::new("umount").arg(&mnt_s).output();
        release
    }

    fn run(&self, kind: JobKind, target: &str, p: &dyn Progress) -> Result<Option<String>, String> {
        let result = self.run_inner(kind, target, p);
        self.unmount_all(p);
        result
    }
}

/// `dev.peios.peios-<VARIANT_ID>` from an os-release, refusing anything that is
/// not a Peios one.
pub fn edition_of(os_release: &str) -> Result<String, String> {
    let unquote = |v: &str| v.trim().trim_matches('"').to_string();
    let mut id = None;
    let mut variant = None;
    for line in os_release.lines() {
        if let Some(v) = line.strip_prefix("ID=") {
            id = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("VARIANT_ID=") {
            variant = Some(unquote(v));
        }
    }
    if id.as_deref() != Some("peios") {
        return Err("this is not a Peios system (os-release ID is not peios)".into());
    }
    match variant {
        Some(v) if !v.is_empty() => Ok(format!("dev.peios.peios-{v}")),
        _ => Err("os-release has no VARIANT_ID; cannot tell which edition is installed".into()),
    }
}

/// The `version:` line of `peipkg info`.
pub fn version_of(info: &str) -> Option<String> {
    info.lines()
        .find_map(|l| l.strip_prefix("version:"))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// What e2fsck's exit status says, as the sentence for the finish
/// message or the reason for failing.
///
/// The status is a bitmask, not a pass/fail: 1 is "errors corrected",
/// which under `-p` is the tool doing precisely what it was asked to,
/// and 2 adds "reboot before using this filesystem". Only 4 and above
/// -- left uncorrected, could not run, bad invocation, cancelled -- are
/// failures. Treating anything but 0 as one turned every successful
/// repair of a disk that needed repairing into a red screen.
pub fn fsck_verdict(status: i32) -> Result<Option<String>, String> {
    if status & !3 != 0 {
        return Err(match status {
            s if s & 4 != 0 => {
                "e2fsck found problems it could not fix on its own; run it by hand from a shell"
                    .into()
            }
            s if s & 8 != 0 => "e2fsck could not run (operational error)".into(),
            s if s & 32 != 0 => "e2fsck was cancelled".into(),
            s => format!("e2fsck failed (exit {s})"),
        });
    }
    Ok(Some(match status {
        0 => "The filesystem is clean.".into(),
        1 => "Problems were found and fixed.".into(),
        _ => "Problems were found and fixed. Reboot before using the system.".into(),
    }))
}

impl Real {
    fn run_inner(
        &self,
        kind: JobKind,
        target: &str,
        p: &dyn Progress,
    ) -> Result<Option<String>, String> {
        std::fs::create_dir_all(&self.work).map_err(|e| e.to_string())?;
        match kind {
            JobKind::Install => {
                self.partition(p, target)?;
                let (esp, root) = Self::partitions(target)?;
                p.log(format!("created {esp} (ESP) and {root} (root)"));
                self.format(p, &esp, &root)?;
                self.mount_target(p, &esp, &root)?;
                self.copy_system(p)?;
                self.make_bootable(p, &root)?;
                p.log("done — reboot with the install medium removed".into());
                Ok(None)
            }
            JobKind::Upgrade => {
                let (esp, root) = Self::partitions(target)?;
                self.mount_target(p, &esp, &root)?;
                let to = self.upgrade(p)?;
                p.phase("phase.boot", 5);
                self.write_boot_files(p, &root)?;
                p.log("done — reboot with the install medium removed".into());
                Ok(Some(format!("Reboot to start {}.", to.text())))
            }
            JobKind::RepairBoot => {
                let (esp, root) = Self::partitions(target)?;
                self.mount_target(p, &esp, &root)?;
                self.repair_boot(p, &root)?;
                Ok(None)
            }
            JobKind::RepairFsck => {
                let (_esp, root) = Self::partitions(target)?;
                if let Some(what) = Self::anything_mounted_on(target) {
                    return Err(format!("refusing to check a mounted filesystem: {what}"));
                }
                p.phase("phase.fsck", 20);
                // -p: fix what is safe to fix without asking; there is
                // nobody to answer. -f forces a check of a filesystem
                // that believes it is clean, which is the point of
                // running this on purpose.
                let status = self.run_status(p, "e2fsck", &["-pf", &root])?;
                p.phase("phase.fsck", 100);
                fsck_verdict(status)
            }
            JobKind::RepairSd => {
                let (esp, root) = Self::partitions(target)?;
                self.mount_target(p, &esp, &root)?;
                p.phase("phase.sd", 40);
                // One inheritable ACE at the top is the whole tree's access
                // policy, so re-stamping it is what "repair permissions"
                // means on Peios.
                //
                // `sd set`, not `seed-sd`: seed-sd exists only in the
                // initramfs (verified against the shipped image), and the
                // two are for different situations anyway. seed-sd writes a
                // descriptor where there is none, which needs
                // SeRestorePrivilege; here mke2fs already stamped one at
                // format time, so this replaces an existing descriptor.
                let r = self.run(
                    p,
                    "sd",
                    &["set", self.root_mnt().to_string_lossy().as_ref(), ROOT_SDDL],
                );
                p.phase("phase.sd", 100);
                r.map(|()| None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Real, edition_of, fsck_verdict, version_of};
    use crate::executor::Progress;
    use std::path::PathBuf;

    #[test]
    fn the_edition_is_read_the_way_upgrade_peios_reads_it() {
        let text = "NAME=\"Peios\"\nID=peios\nVERSION_ID=\"2026.8\"\nVARIANT_ID=experimental\n";
        assert_eq!(edition_of(text).unwrap(), "dev.peios.peios-experimental");
        assert!(edition_of("ID=debian\nVARIANT_ID=x\n").is_err());
        assert!(edition_of("ID=peios\n").is_err());
        assert_eq!(
            version_of(
                "name:         dev.peios.peios-experimental\nversion:      2026.8-7\narchitecture: noarch\n"
            )
            .as_deref(),
            Some("2026.8-7")
        );
        assert_eq!(version_of("name: x\n"), None);
    }

    /// e2fsck's exit status is a bitmask: 1 and 2 are the tool having
    /// done its job, and a repair that reports them as failure is one
    /// that fails precisely when the disk needed it.
    #[test]
    fn fsck_exit_status_is_read_as_the_bitmask_it_is() {
        assert_eq!(
            fsck_verdict(0).unwrap().unwrap(),
            "The filesystem is clean."
        );
        assert_eq!(
            fsck_verdict(1).unwrap().unwrap(),
            "Problems were found and fixed."
        );
        assert!(fsck_verdict(2).unwrap().unwrap().contains("Reboot before"));
        assert!(fsck_verdict(3).unwrap().unwrap().contains("Reboot before"));
        assert!(fsck_verdict(4).unwrap_err().contains("could not fix"));
        // Corrected some and left some: the uncorrected ones win.
        assert!(fsck_verdict(5).unwrap_err().contains("could not fix"));
        assert!(fsck_verdict(8).unwrap_err().contains("could not run"));
        assert!(fsck_verdict(16).unwrap_err().contains("exit 16"));
        assert!(fsck_verdict(32).unwrap_err().contains("cancelled"));
    }

    /// A target root with the queues peiso would have staged into it.
    struct Target {
        dir: PathBuf,
    }

    impl Target {
        fn new(name: &str, queues: &[(&str, &[&str])]) -> Target {
            let dir =
                std::env::temp_dir().join(format!("peios-seedq-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&dir).ok();
            for (queue, seeds) in queues {
                let path = dir.join("root/lcl/policy").join(queue);
                std::fs::create_dir_all(&path).unwrap();
                for seed in *seeds {
                    std::fs::write(path.join(format!("{seed}.reg")), b"{}").unwrap();
                }
            }
            std::fs::create_dir_all(dir.join("root/lcl/policy")).unwrap();
            Target { dir }
        }

        fn settle(&self) -> Result<(), String> {
            struct Quiet;
            impl Progress for Quiet {
                fn phase(&self, _: &str, _: u8) {}
                fn log(&self, _: String) {}
            }
            let real = Real {
                medium: PathBuf::from("/nonexistent"),
                work: self.dir.clone(),
                force: false,
            };
            real.settle_seed_queues(&Quiet)
        }

        fn queue(&self, name: &str) -> Option<Vec<String>> {
            let path = self.dir.join("root/lcl/policy").join(name);
            let entries = std::fs::read_dir(path).ok()?;
            let mut names: Vec<String> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            Some(names)
        }
    }

    impl Drop for Target {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn root_mnt_is(target: &Target) -> PathBuf {
        target.dir.join("root")
    }

    /// The whole reason the queues exist: what only the medium applies
    /// goes, and what only a machine applies arrives.
    #[test]
    fn settling_deletes_the_live_queue_and_promotes_the_install_one() {
        let t = Target::new(
            "both",
            &[
                ("autoapply.d", &["login-console"][..]),
                ("autoapply.live.d", &["lpsd-first-account"][..]),
                ("autoapply.install.d", &["oobe-service"][..]),
            ],
        );
        assert!(root_mnt_is(&t).is_dir());

        t.settle().unwrap();

        assert_eq!(
            t.queue("autoapply.d").unwrap(),
            vec!["login-console.reg", "oobe-service.reg"],
        );
        // Not emptied — gone. An empty directory would say an image
        // might have staged something there, and this one cannot.
        assert!(
            t.queue("autoapply.live.d").is_none(),
            "the live queue must be removed"
        );
        assert!(
            t.queue("autoapply.install.d").is_none(),
            "the install queue must be removed"
        );
    }

    /// An edition that names no live or install seeds ships no such
    /// directory. Absent is not a failure; a target that could not be
    /// installed because it had nothing to delete would be absurd.
    #[test]
    fn settling_an_image_with_neither_queue_does_nothing() {
        let t = Target::new("neither", &[("autoapply.d", &["login-console"][..])]);

        t.settle().unwrap();

        assert_eq!(t.queue("autoapply.d").unwrap(), vec!["login-console.reg"]);
    }

    /// A first boot has to reach the install seeds even on an edition
    /// that stages nothing into the ordinary queue.
    #[test]
    fn the_install_queue_is_promoted_even_with_no_base_queue() {
        let t = Target::new(
            "install-only",
            &[("autoapply.install.d", &["oobe-service"][..])],
        );

        t.settle().unwrap();

        assert_eq!(t.queue("autoapply.d").unwrap(), vec!["oobe-service.reg"]);
    }

    /// A target with no policy directory at all is not an error either:
    /// the queues are the edition's to ship, and this runs before
    /// anything depends on them existing.
    #[test]
    fn settling_a_target_with_no_policy_directory_succeeds() {
        let dir = std::env::temp_dir().join(format!("peios-seedq-bare-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("root")).unwrap();
        struct Quiet;
        impl Progress for Quiet {
            fn phase(&self, _: &str, _: u8) {}
            fn log(&self, _: String) {}
        }
        let real = Real {
            medium: PathBuf::from("/nonexistent"),
            work: dir.clone(),
            force: false,
        };

        assert!(real.settle_seed_queues(&Quiet).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn partition_suffix_is_probed_not_assumed() {
        let sd = |p: &str| matches!(p, "/dev/sda1" | "/dev/sda2");
        assert_eq!(
            Real::partitions_of("/dev/sda", sd).unwrap(),
            ("/dev/sda1".into(), "/dev/sda2".into())
        );
        let nvme = |p: &str| matches!(p, "/dev/nvme0n1p1" | "/dev/nvme0n1p2");
        assert_eq!(
            Real::partitions_of("/dev/nvme0n1", nvme).unwrap(),
            ("/dev/nvme0n1p1".into(), "/dev/nvme0n1p2".into())
        );
        // Only the first partition present is not enough.
        assert!(Real::partitions_of("/dev/sda", |p| p == "/dev/sda1").is_err());
    }

    #[test]
    fn mounted_detection_matches_partitions_but_not_lookalikes() {
        let mounts = "/dev/sda2 / ext4 rw 0 0\n/dev/sdb1 /media/peios iso9660 ro 0 0\n";
        assert!(Real::mounted_in(mounts, "/dev/sda").is_some());
        assert!(Real::mounted_in(mounts, "/dev/sdb").is_some());
        // /dev/sd is not a disk here, and /dev/sdc is untouched.
        assert!(Real::mounted_in(mounts, "/dev/sdc").is_none());
        let nvme = "/dev/nvme0n1p2 / ext4 rw 0 0\n";
        assert!(Real::mounted_in(nvme, "/dev/nvme0n1").is_some());
        assert!(Real::mounted_in(nvme, "/dev/nvme1n1").is_none());
    }
}
