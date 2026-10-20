---
title: Preparing a PC
type: how-to
description: Prepare a raw installation medium, verify the target machine, and reach the supported installation workflow.
---

Use an x86-64 UEFI image appropriate for the machine and a USB device large
enough for that exact image. Preserve needed data from both the USB device and
the intended installation disk. Writing the medium and installing the system
are two separate destructive operations.

## Prepare and boot the medium

Use a raw image-writing workflow, not a file copy onto an existing filesystem.
Check the whole USB device by model, size, and resolved identity; unmount its
filesystems before writing and wait for a successful flushed write. A stable
name does not replace checking the target.

The retained live-boot hook expects a medium exposing the image filesystem
and `rootfs.squashfs`. A multi-image loader that merely exposes an ISO file is
not established as supported. If the boot hook cannot find its medium, check
the image write and device discovery before rewriting or changing firmware.

Choose the UEFI entry in the one-time boot menu. Check the image's signing
instructions against firmware trust; a development image is not automatically
trusted by Secure Boot. Record firmware changes, particularly
storage-controller
mode changes that could prevent another installed system from booting.

Keep the live system on an appropriate setup network: installation and pending
first-boot surfaces can be available before ordinary user sign-in. Use the
image's documented live credentials rather than assuming every image has the
same account or password policy.

## Confirm the machine before installation

Check the detected storage controller, intended disk, and network state.
An unreadable or unrecognized filesystem is not empty. A visible wireless
interface does not prove a usable wireless connection; use the deployed
network tool's supported configuration path.

If using a browser, verify its certificate fingerprint against a trusted local
console before submitting sensitive information. Certificate transfer to the
installed system is best-effort, so repeat that verification if the
fingerprint
changes after installation. A browser reconnect failure is not proof that the
installation failed; verify the local boot and address.

Continue with [Installing to Disk](~peios/installing-peios/installing-to-disk).
For an existing installation to preserve, use the documented repair or upgrade
workflow instead of fresh installation. Keep the medium connected through the
job, then remove it before selecting disk boot. Returning to the installer can
mean firmware selected the USB device again, not that another installation
is required.

After disk boot, complete the image's first-boot setup and verify sign-in.
Hardware reports are evidence for their exact image and configuration, not a
general compatibility certification for every model sharing a brand name.
