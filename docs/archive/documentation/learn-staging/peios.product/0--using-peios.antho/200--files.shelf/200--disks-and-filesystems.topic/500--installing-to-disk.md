---
title: Install, upgrade, or repair a disk
type: how-to
description: Choose an installer operation, verify the target disk, follow progress, and recover from failures, with the older peios-install script documented separately.
related:
  - peios/installing-peios/on-a-pc
  - peios/msip-daemons/rehearse-the-headless-installer
  - peios/disks-and-filesystems/first-boot-setup
  - peios/disks-and-filesystems/overview
  - peios/disks-and-filesystems/formatting-with-security-descriptors
  - peios/disks-and-filesystems/mke2fs
  - peios/developing-for-peios/writing-boot-hooks/writing-a-boot-hook
  - peios/boot-and-trust-establishment/initramfs-stage
  - peios/mount-policies/policy-classes
---

Boot the Peios install medium to install a fresh system, upgrade a disk to the medium's release, or repair an existing installation. For writing a USB stick and configuring PC firmware, start with [Install Peios on a PC](~peios/installing-peios/on-a-pc).

A live system discards its writes at reboot. An installed system uses a writable filesystem on disk, so its changes persist.

## Before you start

- Back up the target disk before changing it, and identify it by model, size, and device name.
- Keep the machine powered and the medium connected while a job runs.
- Use a trusted network if you use the browser installer. On media that enable it, anyone who can reach the address can open the installer without signing in.
- Choose the operation that matches your goal:

| Goal | Operation | Effect |
|---|---|---|
| Replace everything on a disk with a new Peios system | Install | Erases the disk; first-boot setup creates the administrator account. |
| Keep an installed system and move to the medium's newer release | **Upgrade an installation** | Updates eligible packages and boot files; does not run first-boot setup again. |
| Restore boot files, check the root filesystem, or reseed its root descriptor | **Repair an existing system** | Runs the repair you select; it is not a reinstall. |

> [!CAUTION]
> Installation erases the **whole selected disk**, including other operating systems, recovery partitions, encrypted volumes, and files. A filesystem the installer cannot inspect may have no contents listed in the confirmation; that does not protect it from erasure. A failed install after partitioning has already erased the disk.

The current installer is `installerd`, with console and browser surfaces. The older [`peios-install` script](#two-ways-to-invoke-it) is a separate path: it copies the running system and its accounts. Do not assume the two paths have the same account or certificate behavior.

## The installer

### Select and verify the disk

1. Choose the operation on the first page.
2. Select a disk using the **device, model, size, and bus** columns. The boot medium is listed and marked but cannot be selected; other removable disks are marked too. Floppy drives are not listed.
3. Use **Rescan** if the hardware has changed. If disks are missing, read any unclaimed-controller warning beside the table. It identifies the controller kind, PCI vendor/device, and bus location. RAID or VMD mode is a common cause; check firmware settings rather than selecting another disk by guesswork.
4. Read the confirmation page. For installation, it names the disk, warns that all of it will be erased, and lists recognised systems and other filesystems containing files, with labels and used space. **Back** returns to disk selection with your choice retained.
5. Start only when the disk and operation are correct. In the browser, hold down the installation button rather than clicking it once.

The confirmation uses a fresh read of the disk. EFI system and Windows recovery partitions are not separately called out, and uninspectable filesystems are not claimed to be empty. A disk that has been removed, or a script request naming the boot medium, is rejected with a reason.

Selecting and inspecting disks does not write to them. See [how the scan stays read-only](#how-the-disk-scan-stays-read-only) for the supported filesystems and safeguards.

### Follow progress and check the result

The progress page identifies the target, lists phases, and shows every command and its output. It asks for no input. Leave the machine and medium alone until it finishes.

Copy progress measures data written, rather than directory count. The installer estimates the image's size first, for example `1.2 GiB to copy`, then checks filesystem usage several times a second. `/usr` is most of the copy. The estimate runs slightly low, so progress waits just short of complete until copying actually finishes.

**Closing the form does not cancel the job.** `installerd` owns the work. Another console or browser surface can attach to the same conversation and watch it. Both surfaces share choices, including the selected disk; coordinate with anyone else using them.

A successful install job reaches **Installation complete. Reboot to start Peios.** The conversation stays open on this finish page, which offers **Reboot now** and **Back to the start**. After the job completes, remove the USB medium before restarting so firmware boots the installed disk. If you land in the installer again, remove the medium and restart; do not reinstall.

In the [reviewed installer source](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/installerd/src/flow_impl.rs#L104-L131), choosing **Reboot now** ends the conversation only after the restart request is accepted. That is not confirmation that the machine restarted or booted successfully; check the installed system separately.

Check that the installed disk reaches [first-boot setup](~peios/disks-and-filesystems/first-boot-setup), then complete it and sign in. The current installer copies the shipped image, so live-session accounts are not carried over. Editions without first-boot setup use their own account-provisioning seeds instead.

### If a job fails

- Read the failed phase and its command output. A failed job ends the conversation; it does not resume from that point. The next conversation begins at the first page.
- Do not treat failure as a rollback. Partitioning and formatting may already have changed the disk.
- If copying finished but boot setup failed, [Repair boot files](#repairing-an-installed-system) can complete that part of a half-finished install.
- If **Reboot now** fails, the finish page stays open and reports why. The daemon asks peinit to restart the machine, as `reboot` does; the surface itself has no restart privilege.

### Console controls

| Key | Action |
|---|---|
| Tab / Shift+Tab | Move between fields, including disabled fields. The footer explains why a field is disabled. |
| Up / Down | Move through a list; at either end, move to the next field. |
| Enter | Press the highlighted action or advance from a field. |
| Space | Toggle a checkbox. |
| Esc | Ask before leaving. |
| F1 | Show key help. |
| Ctrl+L | Repaint the whole screen. |

A page focuses its first answerable field. The console form reads terminal size once at startup; first-boot setup uses the same renderer. See [console sizing and output](~peios/disks-and-filesystems/first-boot-setup#how-it-gets-the-console) if the form is small or its display is damaged.

On Linux virtual terminals the renderer uses the kernel font's supported characters and sets the sixteen-colour palette. On a serial connection it assumes a modern emulator with rounded corners, ticks, and a spinner. `--plain` forces conservative characters for terminals that need them.

### In a browser

Open `https://<machine-address>:7780` from another device, unless the image uses a different GXWI port. Browser installation requires `installer-gxwi` from `dev.peios.installer-gxwi` and a medium that enables it as GXWI's overlay. Such a medium opens the installer instead of a sign-in page.

The medium generates its own HTTPS certificate. Before trusting a browser warning, compare the browser's SHA-256 fingerprint with the one read at the machine's own console. The live medium's `peios` account needs no password:

```sh
cat /var/state/gxwi/certificate.sha256
```

Do not sign in or enter a password if the fingerprints differ. A page delivered through the connection you are checking is not an independent source for the fingerprint. See [the machine's certificate](~peios/signing-in-from-a-browser/the-certificate).

`installerd` attempts to carry the medium's GXWI key and certificate into the
installed system from either surface. This is best-effort: a skipped or failed
transfer does not fail the installation. Existing browser trust can continue
only if the installed system presents the verified certificate and its other
certificate checks pass. If the fingerprint changes, verify it through the
installed machine's trusted console before entering credentials; see
[certificate carry-over](~peios/signing-in-from-a-browser/the-certificate#installation-and-first-boot).
The older `peios-install` script, or a medium without GXWI, leaves the installed
system to generate its own key when GXWI starts.

After restarting, the browser page waits and follows the machine to setup, the desktop, or the installer if it booted from the medium again. Browser trust affects that waiting page: if the certificate was only bypassed at a warning rather than imported as trusted, the browser may show its own connection error instead. Reopen the address manually in that case.

## Upgrading from the medium

Use **Upgrade an installation** to move an existing disk to the release on the medium without a network or a fresh installation.

1. Select the disk and compare the installed and medium releases on the confirmation page. Both show the edition and full package version.
2. Check that **Upgrade** is enabled. It is enabled only for a newer version of the same edition, using peipkg's ordering including revision.
3. Confirm, follow the phase output, and restart from the installed disk when the job completes.

A disabled button explains the mismatch: already current, a newer installed release, another edition, or no recognised Peios installation. Backing out leaves the disk unchanged. To inspect the release without writing to the target, the installer copies its package database to tmpfs and asks peipkg there.

The upgrade follows [`upgrade-peios`](~peios/peiso/editions-and-upgrades/upgrading-peios) on the mounted target:

1. Upgrade the **edition and its dependency closure**, using the alternate-upgrade bypass for the medium's fixed, intentionally stale repository index.
2. Stage the release's shared `autoapply` **seeds** with `upgrade-peios --seeds-only`. Install-only setup seeds are not repeated.
3. Upgrade **other packages** for which the medium carries a newer revision, in every root including the initramfs. Edition dependencies are floors rather than pins.
4. Regenerate the **boot files** from the upgraded disk, as boot repair does.

The medium's repository is removed from the target whether the upgrade succeeds or fails. Operator customisations under `/lcl` are retained, with the generated boot command line under `/lcl/etc/boot/` rebuilt as part of boot setup.

> [!IMPORTANT]
> A rebuild without a version or revision increase is not an upgrade. Peipkg orders by version and then revision. If the edition package has the same version and revision as the installed one, the medium is already current even if other build contents changed. A build intended for upgrade must bump the edition and every changed package it wants upgraded.

## Repairing an installed system

Choose **Repair an existing system**, select the installed disk, and choose one operation. Repairs target that disk, never the boot medium.

| Operation | What it does | Limit or next step |
|---|---|---|
| **Repair boot files** | Regenerates the command line under `lcl/etc/boot/`, initramfs image, and UKI on the ESP from the disk's kernel and initramfs tree. | It cannot repair a damaged kernel or initramfs tree; that requires reinstalling. |
| **Check the filesystem** | Runs `e2fsck -pf` on the root: forced checking with automatic preen repairs. | Refuses if anything on the disk is mounted. Unfixed problems require a shell and manual `e2fsck`. |
| **Reseed security descriptors** | Re-stamps the root directory's inheritable descriptor with the value used by the installer. | This is the root policy; it does not define a different policy for every subtree. |

Boot repair uses the installed `disk-boot`, not a possibly older copy from the medium, and leaves the installed repository list alone. For a half-finished installation whose copy completed but boot setup failed, it detects the medium-only package checkpoint or the remaining `live-boot` package and finishes the installation's package swap. The checkpoint also covers a failure after live-boot was removed. Packages declared temporary by the medium, including the DWE package on newly built DWE media, are removed; the checkpoint disappears only after that swap succeeds.

Filesystem checking reports whether the filesystem was clean, problems were fixed, or a reboot is required before reuse. Only problems it cannot fix automatically count as failure.

Each repair ends at the finish page. Use **Back to the start** to perform another repair in the same boot, or **Reboot now** when finished.

## Two ways to invoke it

This section describes the older **`peios-install` script**, not a command-line interface to the current installer conversation. Run this legacy script from a **SYSTEM session**; direct use from an administrator session is unsupported because its ordinary copy must read SYSTEM-only files and preserve their security metadata. For ordinary console or browser installation, use the current installer described above, whose privileged operations run in `installerd`.

> [!CAUTION]
> These commands format their targets. Device names below are examples, not recommendations. Check the actual disks and partitions with [`part list`](~peios/disks-and-filesystems/partitioning) before substituting your targets. The script copies live accounts and credentials; read the [account warning](#the-first-account-service-is-retired-the-account-is-not) before using the result.

```sh
peios-install --yes --whole-disk /dev/vda      # partition the disk, then install
peios-install --yes /dev/vda1 /dev/vda2        # format and install onto existing partitions
```

The whole-disk form runs [`part`](~peios/disks-and-filesystems/partitioning) to create a fresh GPT, a 512 MiB EFI system partition (ESP), and a root partition using the remainder. It then follows the two-partition form. **The whole disk is erased.**

A partition table not created by `part` is refused before formatting; `--force` explicitly overrides that protection. Do not use it merely to make an unexplained refusal disappear.

For another partition layout, create it with `part`, then supply the ESP and root partitions to the second form. Both named partitions will be formatted. The installer does not offer separate `/home`, swap, or encryption setup.

### What it refuses before it starts

Before formatting the named partitions, the script rejects a target that is not a block device, the same partition supplied twice, or either partition currently mounted:

```text
# peios-install --yes /dev/vda1 /dev/vda2
peios-install: /dev/vda2 is mounted; refusing to format it
```

These checks prevent formatting a mounted running system. A refusal before the first `mkfs` has not formatted either partition; do not confuse that with a later failure after a whole-disk partitioning step has already changed the disk.

### The first-account service is retired, the account is not

The script copies the running system, including lpsd's account store at `/var/state/lpsd/principals`. It removes the `lpsd-first-account` service so that the live-image provisioner does not run against the populated store every boot:

```sh
reg del 'Machine\System\Services\lpsd-first-account'
```

This deletion is made in the **live source registry**, through the running `registryd`, before the copy and before either partition is formatted. The script verifies it; a deletion failure stops the install. If the key is already absent, it reports that and continues. Editing the source through its registry service avoids modifying an open database behind `registryd`'s back.

Retiring the service does not remove or secure the copied accounts. Otherwise its non-idempotent `lps add` would fail on the existing name, and a provisioner that reasserted an image credential could overwrite an operator's later choice.

> [!CAUTION]
> Treat a copied development account as publicly accessible: it may retain a known image password or a passwordless policy. Secure it before using the machine on a network that matters. For the live `peios` account, set a password **and then require password sign-in**:
>
> ```sh
> lps password peios
> lps policy peios password
> ```
>
> Setting the password alone does not change a passwordless account's sign-in policy. See [Creating accounts](~peios/managing-local-principals/creating-accounts).

The current `installerd` path avoids this transfer: it copies the **shipped image**, omits the live development-account seed, and uses the edition's first-boot setup to create an administrator.

## What the installer does

This section explains the disk and boot artifacts so you can interpret failures. It is not a sequence to run on top of an installation already in progress.

### 1. Format the EFI system partition

`mkfs.vfat -F 32` creates the FAT ESP required by UEFI. FAT cannot store Peios security descriptors, so its access policy comes from a synthesising mount policy.

### 2. Format the root with a security descriptor

The root is ext4, stamped at format time with the system's inheritable descriptor. The example target is `/dev/vdb2`; formatting it is destructive:

```sh
mke2fs -t ext4 -E root_sddl="O:SYG:SYD:(A;OICI;GA;;;SY)(A;OICI;GA;;;BA)(A;OICI;GRGX;;;WD)(A;OICIIO;GA;;;S-1-3-0)" /dev/vdb2
```

The root's `security.peios.sd` makes SYSTEM the owner, grants SYSTEM and Administrators full control, and grants Everyone read and execute. Execute also means directory traversal, so non-SYSTEM services can reach their working directories. The final `CREATOR OWNER` ACE is inherit-only and grants a newly created object's creator full control.

The live system uses the same root policy. Without the creator-owner ACE, a non-administrator could create a file it could not read back: an inherited non-empty DACL takes precedence over the token's default DACL. See [Formatting with security descriptors](~peios/disks-and-filesystems/formatting-with-security-descriptors).

### 3. Copy the system and preserve its security

The older script uses `cp -ax` to copy the running root; the current installer takes the shipped image. `cp -a` requests owner, DACL, SACL, timestamps, links and extended attributes, but its [recursive source-symlink path can suppress a returned copy error](~peios/files-and-directories/cp#when-a-preserve-cannot-be-honoured). Do not treat a successful status from that older copy step as proof that every link and required attribute was preserved. This limitation does not establish how the current installer reports a failed image copy.

The copy stays within filesystem boundaries. Mountpoints are recreated as empty directories: prelude mount-moves `/proc`, `/sys`, and `/dev` at boot, and StrataFS mounts views over `/bin`, `/etc`, `/lib`, and the other view directories. Their backing content in `/usr` and `/lcl` is ordinary root-filesystem content and is copied.

### 4. Write the kernel command line

`dev.peios.disk-boot` supplies the stable template at `/usr/share/disk-boot/cmdline`. The installer appends `root=UUID=<the new root>` and writes the per-install command line to `/lcl/etc/boot/cmdline` on the target.

### 5. Build the boot artifact

`mkuki` bundles the kernel, initramfs, and command line into `EFI/BOOT/BOOTX64.EFI` on the ESP. UEFI can boot this fallback path without an NVRAM entry. There is no separate bootloader or boot manager between firmware and kernel.

## Use UUIDs, not device names

The installed command line identifies the root by UUID because device names can change. A disk seen as `/dev/vdb` behind the install medium may become `/dev/vda` when the medium is removed.

The initramfs probes block devices directly to resolve the UUID. It cannot rely on `/dev/disk/by-uuid`, because no device manager has created those paths there. See [Stable device names](~peios/disks-and-filesystems/stable-device-names).

## How the installed system boots

The reviewed installer replaces the live boot components with the disk-boot
components before rebuilding the installed initramfs. It then verifies that
`mount-root-disk.sh` is present and that the live `mount-root.sh` is absent.
An installed image therefore does not select between both hooks at boot.

For disk boot, `root=` identifies the partition to mount; it is not a live/disk
mode switch. A missing or empty `root=` makes the disk hook fail rather than
silently leave the work to another hook. The disk hook mounts that partition
directly, without the live medium's writable overlay.

This follows the pinned [installer checks](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/installerd/src/real.rs#L774-L852)
and [disk-root hook](https://github.com/peios/pkgs/blob/92b0caf88e87c72931eee188a07ac87883d913c7/dev.peios.disk-boot/src/mount-root-disk.sh#L6-L70).
Check the image you operate; these are source-level descriptions, not a boot
repair procedure. For authoring and ordering, see
[Writing a boot hook](~peios/developing-for-peios/writing-boot-hooks/writing-a-boot-hook).

The disk hook uses `policy=deny-missing` and does not run `seed-sd`. The installed filesystem already carries descriptors, so a missing one is an error to investigate. The live squashfs has none: it uses `synth-ephemeral` and seeds an inheritable descriptor on the tmpfs above it.

To inspect the installed root descriptor:

```sh
sd show /
```

Check the owner and actual ACEs against the root policy above. `ID` means `INHERITED_ACE`; the root's explicit format-time ACEs should not carry it. The installed filesystem carries this access policy itself rather than relying on a mount-time template.

## How the disk scan stays read-only

`installerd` discovers partitions, used space, and recognised Peios, Linux, or Windows systems without writing to the disks. A read-only filesystem mount alone is not sufficient for ext4, which may replay its journal.

Instead, each inspected partition is bound to a read-only loop device, and that device is mounted. Ext3 and ext4 also use `noload`. The mount uses `synth-ephemeral`, allowing filesystems from other systems to be read without writing security descriptors to them.

FAT, exFAT, NTFS, ext2, ext3, and ext4 are inspected this way. Swap, encrypted volumes, and other filesystems are listed by size and partition-table type without opening them. Symbolic links on the inspected disk are never followed, because they could resolve against the install medium. The daemon supplies this detail to surfaces; the console disk page itself shows the table.

## Installer services and image configuration

`installerd` runs as SYSTEM and performs privileged disk operations. `install-tui` only draws the console form and has no privileges of its own. They speak [MSIP](~peios/services-and-jobs/overview), which also allows scripted interaction through `msip-drive` and surfaces attaching to an existing job. To study the driver, use [Rehearse the headless installer in a lab](~peios/msip-daemons/rehearse-the-headless-installer), a source-derived synthetic dry-run exercise with a private socket. It is not a validated recipe for unattended installation on real disks.

For a browser-enabled medium, the `installer-gxwi-overlay` seed sets these values under `Machine\Software\GXWI`:

| Value | Setting |
|---|---|
| `OverlayUsername` | `peios`, the medium's passwordless account |
| `OverlaySession` | `/usr/bin/installer-gxwi` |

The edition must list this seed under [`live_autoapply`](~peios/peiso/editions-and-upgrades/release-toml), **not `autoapply`**. `installerd` deletes the live queue from the installed disk. That leaves the graphical installer program installed without making it the permanent browser entry point. If the seed is shared through `autoapply`, the installed machine keeps opening the installer; GXWI does not fall back from a requested overlay to sign-in.

## What the installer does not do

- **Offer multiple automatic layouts.** The whole-disk layout is one 512 MiB ESP and one root using the remainder. Separate `/home`, swap, and encryption are not installer options. Use the older script's two-partition form only when you have deliberately prepared another supported layout.
- **Make every directory's policy the same as its intended use.** The root descriptor supplies a baseline; a private subtree needs its own protected descriptor. Account home directories have that separate policy, created at [first sign-in](~peios/managing-local-principals/creating-accounts#what-a-new-account-gets).
- **Recover every failed system.** Boot repair needs an intact kernel and initramfs tree; automatic filesystem checking cannot fix every error. Read the reported failure before choosing a destructive reinstall.

## Where to go next

- [Complete first-boot setup](~peios/disks-and-filesystems/first-boot-setup).
- [Understand format-time descriptors](~peios/disks-and-filesystems/formatting-with-security-descriptors).
- [Inspect the boot hooks](~peios/developing-for-peios/writing-boot-hooks/writing-a-boot-hook).
- [Understand missing-descriptor failures](~peios/mount-policies/policy-classes).
