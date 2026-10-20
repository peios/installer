---
title: Installation Process
type: concept
description: Disk discovery, pristine-system copying, and preparation for disk boot.
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

To identify a Peios release, the daemon copies the package database and its
available companion files into scratch space. It asks the package manager
about that copy, so SQLite does not need to create files on the inspected
filesystem. The release metadata and database files must open as regular
files; direct symbolic links and special files are rejected.
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

## Copying the pristine system

The copy source is the medium's `rootfs.squashfs`, not the live session's
writable root. Changes made while running the installer, including live
account changes, are not a general source for the installed system.

The installer handles the installed system's package transition and registry
seed queues. Seeds staged only for installation are promoted into the target
queue; live-session provisioning is not copied as an instruction to repeat
on every installed boot.

The GXWI key and certificate are a deliberate exception to the pristine
copy. Their transfer preserves browser continuity when it succeeds, but
installation is allowed to continue if that transfer fails.

## Preparing disk boot

The installer replaces the medium's temporary boot components with the disk
boot components. It generates the kernel command line using the root
filesystem's universally unique identifier (UUID), so a later device-name
change does not select a different root by accident.

Boot preparation creates the initramfs and unified kernel image at the EFI
fallback location. It checks that the initramfs includes the disk root-mount
hook and excludes the live root-mount hook.

These checks validate generated content. Firmware selection and a successful
boot still need to be observed on the target machine or a representative
disposable guest.
