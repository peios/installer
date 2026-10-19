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

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::copying;
use crate::executor::{Disk, ESP_MIB, Executor, JobKind, Phase, Progress, Release, size_text};
use crate::executor::{
    INSTALL_PHASES, REPAIR_BOOT_PHASES, REPAIR_FSCK_PHASES, REPAIR_SD_PHASES, UPGRADE_PHASES,
};
use crate::inspect;
use crate::medium_packages::{self, LIVE_BOOT_PACKAGE, MEDIUM_REPO, REMOVALS_REL};

/// How often a command that is being watched is looked in on.
const WATCH_EVERY: Duration = Duration::from_millis(200);

/// Written to the root inode at format time, and seeded onto the ESP's
/// mount. Kept identical to the script's and to live-boot's, because a
/// tree whose top-level descriptor differs from the one the image was
/// built against is administrable by a different set of principals.
const ROOT_SDDL: &str =
    "O:SYG:SYD:(A;OICI;GA;;;SY)(A;OICI;GA;;;BA)(A;OICI;GRGX;;;WD)(A;OICIIO;GA;;;S-1-3-0)";

const CMDLINE_TEMPLATE_REL: &str = "usr/share/disk-boot/cmdline";

/// Where gxwid keeps this machine's key and certificate (gxwid(1)).
const GXWI_KEYS: &str = "/var/state/gxwi";

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
    /// Where what has to be copied off a disk to be read is copied to:
    /// under the scratch, so on the tmpfs and never on a disk.
    fn inspect_scratch(&self) -> PathBuf {
        self.work.join("inspect-scratch")
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

    /// Run a command as [`Real::run`] does, calling `watch` every so often
    /// while it runs and once more when it is over. For a command that
    /// takes long enough to be worth saying how far along it is, and does
    /// not say so itself.
    fn run_watched(
        &self,
        p: &dyn Progress,
        program: &str,
        args: &[&str],
        watch: &mut dyn FnMut(),
    ) -> Result<(), String> {
        p.log(format!("$ {program} {}", args.join(" ")));
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not run {program}: {e}"))?;
        // Both streams are read as they come. Left until the command is
        // over, one that said more than a pipe holds would wait for ever
        // to be read, and this would wait for ever for it to finish.
        let said: Vec<_> = [
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|mut stream| {
            std::thread::spawn(move || {
                let mut all = Vec::new();
                let _ = stream.read_to_end(&mut all);
                all
            })
        })
        .collect();
        // The wait is another thread's, so that the end is heard the
        // moment it comes and not at the next look.
        let (ended, end) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = ended.send(child.wait());
        });
        let status = loop {
            match end.recv_timeout(WATCH_EVERY) {
                Ok(status) => break status,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => watch(),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(format!("lost track of {program}"));
                }
            }
        };
        watch();
        for stream in said {
            let stream = stream.join().unwrap_or_default();
            for line in String::from_utf8_lossy(&stream).lines() {
                if !line.trim().is_empty() {
                    p.log(format!("  {line}"));
                }
            }
        }
        match status
            .map_err(|e| format!("could not wait for {program}: {e}"))?
            .code()
        {
            Some(0) => Ok(()),
            Some(c) => Err(format!("{program} failed (exit {c})")),
            None => Err(format!("{program} was killed")),
        }
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
                &format!("{ESP_MIB}M"),
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
        //
        // The phase follows how much has been written, not which entry
        // is being copied: /usr is nearly all of it and comes almost
        // last (see `copying`). What the image will come to is worked
        // out first, and the target says how much of that it holds.
        let target = self.root_mnt();
        let measured = inspect::room(&target).and_then(|(before, block)| {
            let total = copying::room_for(&self.lower_mnt(), block);
            (total > 0).then_some((before, total))
        });
        match measured {
            Some((_, total)) => p.log(format!("{} to copy", size_text(total))),
            None => p.log("could not measure the image; the copy is reported by entry".into()),
        }
        let count = entries.len();
        let mut reached = 0;
        for (i, entry) in entries.iter().enumerate() {
            if measured.is_none() {
                p.phase("phase.copy", ((i * 100) / count) as u8);
            }
            let name = entry
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            p.log(format!("copying /{name}"));
            self.run_watched(
                p,
                "cp",
                &[
                    "-ax",
                    entry.to_string_lossy().as_ref(),
                    &format!("{}/", target.display()),
                ],
                &mut || {
                    let Some((before, total)) = measured else {
                        return;
                    };
                    let Some(now) = inspect::used_bytes(&target) else {
                        return;
                    };
                    // Only ever onward, and only when it has moved: each
                    // report goes to every surface watching.
                    let at = copying::percent(now.saturating_sub(before), total);
                    if at > reached {
                        reached = at;
                        p.phase("phase.copy", at);
                    }
                },
            )
            .map_err(|e| format!("copying /{name}: {e}"))?;
        }
        p.phase("phase.copy", 100);
        self.run(p, "umount", &[self.lower_mnt().to_string_lossy().as_ref()])
    }

    /// GXWI's key and certificate, from this medium into the system it
    /// installs, so the browser that installed the machine goes on trusting
    /// it there and setup carries on in the same tab without a second
    /// warning. Every install carries it, whichever surface asked: the
    /// console's installer serves the same GXWI with the same key.
    ///
    /// `cp -a`, as the system itself is copied, so the directory arrives with
    /// its descriptor: SYSTEM's alone, as gxwid made it.
    ///
    /// Never a reason to fail the install. A medium without GXWI has no key
    /// to carry, and one that cannot be carried costs only a new key, which
    /// gxwid makes on the installed system's first start.
    fn carry_machine_key(&self, p: &dyn Progress, from: &Path) {
        if !from.join("key.pem").is_file() {
            p.log("no GXWI key on this medium; the installed system makes its own".into());
            return;
        }
        let into = self.root_mnt().join("var/state");
        let carried = std::fs::create_dir_all(&into)
            .map_err(|e| format!("creating {}: {e}", into.display()))
            .and_then(|()| {
                self.run(
                    p,
                    "cp",
                    &[
                        "-a",
                        from.to_string_lossy().as_ref(),
                        &format!("{}/", into.display()),
                    ],
                )
            });
        match carried {
            Ok(()) => p.log("carried GXWI's key and certificate to the installed system".into()),
            Err(e) => p.log(format!(
                "could not carry GXWI's key and certificate ({e}); the installed system makes its own, \
                 and a browser will warn about it once"
            )),
        }
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
    /// install, where the copy landed and boot setup died. Its removal
    /// checkpoint survives even if live-boot has already been removed;
    /// older media are also recognised by the live-boot package.
    fn repair_boot(&self, p: &dyn Progress, root_part: &str) -> Result<(), String> {
        let root_s = self.root_mnt().to_string_lossy().into_owned();
        p.phase("phase.boot", 5);
        if self
            .root_mnt()
            .join(REMOVALS_REL)
            .try_exists()
            .map_err(|e| format!("checking medium-only package checkpoint: {e}"))?
            || self.package_installed(&root_s, LIVE_BOOT_PACKAGE)
        {
            p.log("the disk still carries medium installation state: finishing the install".into());
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

    /// Remove the medium's declared temporary packages and install disk-boot.
    fn swap_boot_packages(&self, p: &dyn Progress) -> Result<(), String> {
        medium_packages::finish_copy(&self.root_mnt(), |args| {
            if args.get(2) == Some(&"list") {
                self.capture("peipkg", args)
            } else {
                self.run(p, "peipkg", args).map(|()| String::new())
            }
        })
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

    /// Every filesystem on every disk but the medium, read through a
    /// read-only loop device so that none of them is written to (see
    /// [`crate::inspect`]). The medium is left out: it is what this is
    /// running from, and nobody is choosing it.
    fn read_contents(&self, disks: &mut [Disk]) {
        let (mnt, scratch) = (self.inspect_mnt(), self.inspect_scratch());
        for disk in disks.iter_mut().filter(|d| !d.medium) {
            if !inspect::read_disk(disk, &mnt, &|root| inspect::release_at(root, &scratch)) {
                break;
            }
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
        // The medium is the running system: what it carries is what it
        // booted, and its package database says which release that is.
        Self::release_in(Path::new("/"))
    }

    /// Mount the disk's root where nothing can write to it, read which
    /// release it holds, unmount it. Looking at a disk changes nothing
    /// on it -- not even a journal replay -- so the page can be reached,
    /// and backed out of, without the disk having been touched. It is
    /// the reading the disk page makes of every disk, so the two cannot
    /// say different things of one ([`crate::inspect`]).
    fn installed_release(&self, target: &str) -> Result<Release, String> {
        let (_esp, root) = Self::partitions(target)?;
        if let Some(what) = Self::anything_mounted_on(target) {
            return Err(format!("{what}; unmount it first"));
        }
        let scratch = self.inspect_scratch();
        // The root an install makes is ext4, whose journal is left alone.
        inspect::mounted(Path::new(&root), "noload,", &self.inspect_mnt(), |mnt| {
            inspect::release_at(mnt, &scratch)
        })
        .map_err(|failure| format!("could not read {root}: {}", failure.why()))?
    }

    fn run(&self, kind: JobKind, target: &str, p: &dyn Progress) -> Result<Option<String>, String> {
        let result = self.run_inner(kind, target, p);
        self.unmount_all(p);
        result
    }

    /// Asks peinit for a reboot, as `reboot` does. peinit answers once it
    /// has taken the request and before it stops anything, so there is
    /// time to say so; its control descriptor grants SYSTEM the right.
    fn restart(&self) -> Result<(), String> {
        let out = Command::new("svctl")
            .args(["shutdown", "reboot"])
            .output()
            .map_err(|e| format!("could not run svctl: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if said.is_empty() {
            "peinit did not take the request to restart the machine.".into()
        } else {
            format!("peinit did not take the request to restart the machine: {said}")
        })
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
                self.carry_machine_key(p, Path::new(GXWI_KEYS));
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
mod tests;
