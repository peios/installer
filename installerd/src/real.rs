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

use crate::executor::{Disk, ESP_MIB, Executor, JobKind, Phase, Progress, Release};
use crate::executor::{
    INSTALL_PHASES, REPAIR_BOOT_PHASES, REPAIR_FSCK_PHASES, REPAIR_SD_PHASES, UPGRADE_PHASES,
};
use crate::inspect;

/// Written to the root inode at format time, and seeded onto the ESP's
/// mount. Kept identical to the script's and to live-boot's, because a
/// tree whose top-level descriptor differs from the one the image was
/// built against is administrable by a different set of principals.
const ROOT_SDDL: &str =
    "O:SYG:SYD:(A;OICI;GA;;;SY)(A;OICI;GA;;;BA)(A;OICI;GRGX;;;WD)(A;OICIIO;GA;;;S-1-3-0)";

/// The installed system's package store, where its closures are, relative
/// to its root.
const STORE_REL: &str = "system/artifacts";

/// Where the installed system keeps what its boot files are built from:
/// its initramfs's profile and the initramfs image.
const BOOT_REL: &str = "system/boot";

/// What the medium says an installed system is composed from (peiso):
/// the profiles of its SYSTEM and INITRAMFS closures.
const INSTALL_SYSTEM: &str = "install/system.toml";
const INSTALL_INITRAMFS: &str = "install/initramfs.toml";
/// How many packages are copied from the medium at once.
const COPY_BATCH: usize = 20;

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

    /// The release the `/usr` tree at `usr` holds: its edition from the
    /// os-release the edition package ships, and its version from the
    /// closure's inventory of that package. A tree with no os-release, such
    /// as a development image without an edition, is named by its closure.
    fn release_in(usr: &Path) -> Result<Release, String> {
        let inventory = ppk::read_inventory(usr)
            .map_err(|e| format!("{}: {e:#}", usr.display()))?
            .ok_or_else(|| format!("no Peios system here (no {}/inventory)", usr.display()))?;
        let os_release = std::fs::read_to_string(usr.join("lib/os-release")).ok();
        release_from(os_release.as_deref(), &inventory)
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
        for m in [self.esp_mnt(), self.root_mnt()] {
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

    /// The target's package store, where its closures are too.
    fn target_store(&self) -> PathBuf {
        self.root_mnt().join(STORE_REL)
    }

    /// Compose the installed system on the mounted target from the
    /// medium: fetch every package its two closures need from the medium's
    /// store into the target's, build the closures and name them `SYSTEM`
    /// and `INITRAMFS` (PSPU 5.27), give the root what its skeleton file
    /// says it holds outside `/usr`, and carry the medium's local policy.
    fn install_system(&self, p: &dyn Progress) -> Result<(), String> {
        let medium_store = self.medium.join("artifacts");
        let store = self.target_store();
        let closures = store.join(ppk::CLOSURE_SUBDIR);
        let mut places = Vec::new();
        for (name, file) in [
            (ppk::SYSTEM, INSTALL_SYSTEM),
            (ppk::INITRAMFS, INSTALL_INITRAMFS),
        ] {
            let path = self.medium.join(file);
            let profile = ppk::Profile::read(&path).map_err(|_| {
                format!(
                    "this medium holds no {}; it was built from a spec with nothing to install",
                    path.display()
                )
            })?;
            let ids = ppk::resolve(
                &profile.packages,
                std::slice::from_ref(&medium_store),
                profile.arch(),
            )
            .map_err(|e| format!("resolving {}: {e:#}", path.display()))?
            .ids();
            places.push((name, profile, ids));
        }
        let mut all: Vec<ppk::PackageId> = Vec::new();
        for id in places.iter().flat_map(|(_, _, ids)| ids) {
            if !all.contains(id) {
                all.push(id.clone());
            }
        }
        p.log(format!("copying {} packages from the medium", all.len()));
        // In batches: ppk syncs the disk a few times for each batch, and
        // the progress moves once for each.
        let mut copied = 0;
        for batch in all.chunks(COPY_BATCH) {
            ppk::fetch_all(batch, &medium_store, &store)
                .map_err(|e| format!("copying from the medium: {e:#}"))?;
            copied += batch.len();
            p.phase("phase.copy", (copied * 90 / all.len().max(1)) as u8);
        }
        for (name, profile, ids) in &places {
            let closed = ppk::closure_for(
                ids,
                &profile.packages,
                &store,
                &closures,
                ppk::UsrFiles::Hardlink,
            )
            .map_err(|e| format!("composing {name}: {e:#}"))?;
            let identity: ppk::Identity = closed.identity.parse().map_err(|e| format!("{e:#}"))?;
            ppk::name_closure(&closures, name, &identity)
                .map_err(|e| format!("naming {name}: {e:#}"))?;
            p.log(format!("{name} is {identity}"));
        }
        // The initramfs's profile stays on the disk, so that repairing the
        // boot files can compose the initramfs again from the disk alone.
        let boot = self.root_mnt().join(BOOT_REL);
        std::fs::create_dir_all(&boot).map_err(|e| format!("{}: {e}", boot.display()))?;
        std::fs::copy(
            self.medium.join(INSTALL_INITRAMFS),
            boot.join("initramfs.toml"),
        )
        .map_err(|e| format!("keeping the initramfs's profile: {e}"))?;

        // /usr is where the boot hook mounts SYSTEM; the rest of the root
        // is the skeleton file's (PSPU 11).
        let root = self.root_mnt();
        std::fs::create_dir_all(root.join("usr")).map_err(|e| e.to_string())?;
        let system = closures.join(ppk::SYSTEM);
        self.run(
            p,
            "skeleton",
            &[
                "apply",
                "--file",
                system.join("share/skeleton").to_string_lossy().as_ref(),
                root.to_string_lossy().as_ref(),
            ],
        )?;
        let lcl = self.medium.join("lcl");
        if lcl.is_dir() {
            p.log("copying the medium's local policy".into());
            self.run(
                p,
                "cp",
                &[
                    "-a",
                    lcl.to_string_lossy().as_ref(),
                    &format!("{}/", root.display()),
                ],
            )?;
        }
        p.phase("phase.copy", 100);
        Ok(())
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

    /// Turn a composed target into a system that boots from disk.
    fn make_bootable(&self, p: &dyn Progress, root_part: &str) -> Result<(), String> {
        p.phase("phase.boot", 5);
        self.settle_seed_queues(p)?;
        self.write_boot_files(p, root_part)
    }

    /// Rewrite the boot files of a system already on the disk, from what the
    /// disk holds: its `INITRAMFS` closure's profile and `SYSTEM`'s kernel.
    fn repair_boot(&self, p: &dyn Progress, root_part: &str) -> Result<(), String> {
        p.phase("phase.boot", 5);
        self.write_boot_files(p, root_part)
    }

    /// Write the cmdline, build the initramfs and place the UKI, all from
    /// what is on the disk.
    fn write_boot_files(&self, p: &dyn Progress, root_part: &str) -> Result<(), String> {
        let root = self.root_mnt();
        p.phase("phase.boot", 20);
        // The kernel has no udev here, so /dev/disk/by-uuid is empty and
        // the UUID has to be read from the device itself.
        let uuid = self
            .capture("lsblk", &["-n", "-o", "UUID", root_part])?
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .ok_or_else(|| format!("could not read the UUID of {root_part}"))?;
        // The consoles this medium was booted with, which the installed
        // system keeps, then the root.
        let consoles: Vec<String> = std::fs::read_to_string("/proc/cmdline")
            .unwrap_or_default()
            .split_whitespace()
            .filter(|word| word.starts_with("console="))
            .map(str::to_owned)
            .collect();
        let mut cmdline = consoles.join(" ");
        if !cmdline.is_empty() {
            cmdline.push(' ');
        }
        cmdline.push_str(&format!("root=UUID={uuid}\n"));
        std::fs::create_dir_all(root.join("lcl/etc/boot")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("lcl/etc/boot/cmdline"), &cmdline).map_err(|e| e.to_string())?;
        p.log(format!("cmdline: {}", cmdline.trim()));

        // The initramfs, composed in scratch from the disk's store. Copied,
        // not linked: the scratch is a tmpfs, the store is on the disk.
        p.phase("phase.boot", 40);
        let store = self.target_store();
        let profile_path = root.join(BOOT_REL).join("initramfs.toml");
        let profile = ppk::Profile::read(&profile_path)
            .map_err(|e| format!("{}: {e:#}", profile_path.display()))?;
        let ids = ppk::resolve(
            &profile.packages,
            std::slice::from_ref(&store),
            profile.arch(),
        )
        .map_err(|e| format!("resolving the initramfs from the disk: {e:#}"))?
        .ids();
        let irf = self.work.join("initramfs");
        let _ = std::fs::remove_dir_all(&irf);
        let extras = ppk::Extras {
            symlinks: profile.symlinks.clone(),
            dirs: profile.dirs.clone(),
            files: ppk::UsrFiles::Copy,
            ..ppk::Extras::default()
        };
        ppk::compose_env_for(&ids, &profile.packages, &store, &irf, &extras)
            .map_err(|e| format!("composing the initramfs: {e:#}"))?;
        let skeleton = irf.join("usr/share/skeleton");
        if skeleton.is_file() {
            self.run(
                p,
                "skeleton",
                &[
                    "apply",
                    "--file",
                    skeleton.to_string_lossy().as_ref(),
                    irf.to_string_lossy().as_ref(),
                ],
            )?;
        }
        let hooks = irf.join("usr/libexec/prelude/hooks.d");
        if !hooks.join("ppk-disk-root").is_file() {
            return Err("the installed system's initramfs has no disk-root boot hook".into());
        }
        if hooks.join("ppk-live-root").is_file() {
            return Err(
                "the installed system's initramfs holds the live medium's boot hook".into(),
            );
        }
        p.phase("phase.boot", 60);
        let image = root.join(BOOT_REL).join("initramfs.cpio.zst");
        self.run(
            p,
            "mkirf",
            &[
                irf.to_string_lossy().as_ref(),
                image.to_string_lossy().as_ref(),
                "--compress",
                "zstd",
            ],
        )?;
        let _ = std::fs::remove_dir_all(&irf);

        p.phase("phase.boot", 85);
        // One kernel per closure, at a fixed path (PSPU 5.14).
        let kernel = store
            .join(ppk::CLOSURE_SUBDIR)
            .join(ppk::SYSTEM)
            .join("lib/modules/vmlinuz");
        if !kernel.is_file() {
            return Err(format!(
                "the SYSTEM closure has no kernel at {}",
                kernel.display()
            ));
        }
        std::fs::create_dir_all(self.esp_mnt().join("EFI/BOOT")).map_err(|e| e.to_string())?;
        self.run(
            p,
            "mkuki",
            &[
                "--kernel",
                kernel.to_string_lossy().as_ref(),
                "--initramfs",
                image.to_string_lossy().as_ref(),
                "--cmdline-file",
                &format!("{}/lcl/etc/boot/cmdline", root.display()),
                "--out",
                &format!("{}/EFI/BOOT/BOOTX64.EFI", self.esp_mnt().display()),
            ],
        )?;
        self.run(p, "sync", &[])?;
        p.phase("phase.boot", 100);
        Ok(())
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
        // booted, and its closure's inventory says which release that is.
        Self::release_in(Path::new("/usr"))
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

/// The release a closure is: its edition from `os_release`, and the
/// edition package's version from `inventory`. A closure with no
/// os-release, such as a development image's without an edition, is named
/// by its identity.
pub fn release_from(
    os_release: Option<&str>,
    inventory: &ppk::Inventory,
) -> Result<Release, String> {
    let Some(text) = os_release else {
        return Ok(Release {
            edition: "Peios".to_owned(),
            version: inventory.identity.hex()[..12].to_owned(),
        });
    };
    let edition = edition_of(text)?;
    let version = (inventory.packages.iter())
        .find(|package| package.id.name() == edition)
        .map(|package| package.id.version().to_owned())
        .ok_or_else(|| format!("{edition} is not in this system's closure"))?;
    Ok(Release { edition, version })
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
                self.install_system(p)?;
                self.carry_machine_key(p, Path::new(GXWI_KEYS));
                self.make_bootable(p, &root)?;
                p.log("done — reboot with the install medium removed".into());
                Ok(None)
            }
            JobKind::Upgrade => Err(
                "upgrading an installed system from a medium isn't supported yet; it comes with \
                 the upgrade tooling"
                    .to_owned(),
            ),
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
