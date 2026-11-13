---
title: Installation Process
type: concept
description: Disk discovery, composing the installed system from the medium's packages, and preparation for disk boot.
---

A fresh installation builds a disk-based system from the installation
medium. Understanding the stages helps distinguish a discovery problem
from a failure after the target has already changed.

## Discovering disks

The daemon lists devices, partitions, and recognizable systems so you can
identify the target. When it inspects an unmounted filesystem, it uses a
read-only loop device. Ext filesystems also use the `noload` option to avoid
journal replay during inspection. An already mounted filesystem is read
through its existing mount.

To identify a Peios release, the daemon reads the closure the disk's package
store names `SYSTEM`: its edition from its `os-release`, and the edition's
version from its inventory. The name must name a closure in the store, and
the inventory and `os-release` must open as regular files; symbolic links and
special files are rejected. A system with no edition is named by its
closure's identity.
Unsupported or unreadable content is not proof that a disk is empty.

## Creating the layout

After confirmation, the daemon creates an Extensible Firmware Interface
(EFI) system partition and a root partition using the remaining space. It
formats the first as FAT32 and the
second as ext4. The root filesystem receives its security descriptor during
formatting, before normal access to the copied system begins.

This workflow replaces the whole disk. It does not configure disk
encryption, a separate home partition, or another operating system's boot
menu.

## Composing the installed system

The installed system is composed from the medium's packages, not copied from
the live session. The medium says what an installed system holds, in
`/install/system.toml` and `/install/initramfs.toml`. The daemon copies the
packages those need from the medium's store into the target's
`/system/artifacts`, builds them into two closures, and names them `SYSTEM`,
the installed system's `/usr`, and `INITRAMFS`, what its initramfs is built
from. The root's directories outside `/usr` come from the root skeleton file.
Changes made while running the installer, including live account changes,
don't reach the installed system.

The installer carries the medium's local policy and settles its registry seed
queues. Seeds staged only for installation are promoted into the target
queue; live-session provisioning is not copied as an instruction to repeat
on every installed boot.

The GXWI key and certificate are carried from the live session. Their
transfer preserves browser continuity when it succeeds, but installation is
allowed to continue if that transfer fails.

## Preparing disk boot

The installer generates the kernel command line using the root filesystem's
universally unique identifier (UUID), so a later device-name change does not
select a different root by accident, and keeps the consoles the medium was
booted with.

Boot preparation composes the initramfs from `INITRAMFS` and builds the
unified kernel image from it and `SYSTEM`'s kernel, at the EFI fallback
location. It checks that the initramfs includes the disk root-mount hook and
excludes the live one. At boot, that hook mounts the root partition and
mounts the `SYSTEM` closure at `/usr`.

These checks validate generated content. Firmware selection and a successful
boot still need to be observed on the target machine or a representative
disposable guest.
