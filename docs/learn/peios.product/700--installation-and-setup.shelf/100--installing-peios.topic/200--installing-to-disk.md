---
title: Installing to Disk
type: how-to
description: Selecting a target disk, confirming erasure, and checking an installation.
---

Use the installer to replace a whole disk with Peios. The installation
copies the medium's pristine system and creates the files needed to boot
from that disk.

## Before you start

Prepare these prerequisites:

- A booted Peios installation medium appropriate for the machine.
- A backup of every file you need from the target disk.
- A way to identify the target by its device, model, and size.
- Access to the terminal interface or the medium's GXWI browser overlay.

> [!WARNING]
> A fresh installation erases the entire selected disk. It does not preserve
> another operating system or provide a custom partitioning workflow.

## Install the system

1. Select **Install Peios** on the first page.
2. Select the target disk after checking its device, model, and size.
3. Select **Next** to review the disk and the data that will be erased.
4. Confirm the installation. In the browser, hold the confirmation button
   until its indicator completes.
5. Wait for partitioning, formatting, copying, and boot preparation to
   finish. If a phase fails, read the reported error before retrying.
6. Remove the installation medium when the machine is ready to boot from
   its disk, then use the offered restart action.

The installer refuses to partition a disk it finds mounted. The daemon's
`--force` option only permits replacement of an unrecognized partition
table; it does not bypass that guard or the confirmation page.

## Check the result

Confirm that the machine boots from the installed disk. An installation
completion message reports that the job finished; it does not prove that
firmware selected that disk or that the system booted successfully.

If the installed image includes first-boot setup, continue with
[Completing Setup](~peios/first-boot/completing-setup). An image can instead
provision accounts through its own configuration.

The browser saves job output through its log download. Keep that output if
installation fails. Reconnecting to a running daemon can recover the
conversation, but restarting the daemon loses its in-memory state.
