---
title: Recovery and Upgrades
type: concept
description: The supported repair operations and the conditions for an upgrade from a medium.
---

The terminal installer can repair or upgrade a Peios system found on a
disk. These operations have narrower purposes than a fresh installation;
choose the operation that matches the fault.

## Repair operations

The repair flow provides three operations:

| Operation | Effect | Limit |
|---|---|---|
| Boot repair | Rebuilds the initramfs and unified kernel image from the installed system | Does not replace the system with the medium's release |
| Filesystem check | Runs a forced ext filesystem check with automatic safe repairs | Refuses a mounted target and can require a restart |
| Permissions repair | Replaces the root directory's security descriptor | Does not restore every explicitly set descriptor in the tree |

Repair changes the selected system. Back up recoverable data before using
it. A filesystem check can report a fault it cannot repair automatically;
read the job result before using the disk again.

The supported layout has an Extensible Firmware Interface (EFI) system
partition followed by the root partition. Repair is not a general disk-layout
reconstruction tool. It does
not guarantee recovery from every failed installation or setup attempt.

## Upgrading from a medium

The upgrade flow compares the installed release with the release carried
by the medium. It requires the same edition and a newer version it can
order. The flow rejects an equal version, a downgrade, an edition change,
or versions it cannot compare.

An upgrade reconciles packages against the medium and applies the installed
system's upgrade seeds, then regenerates boot files. It preserves the
installed machine's GXWI key rather than carrying the medium's key over.

Use `install-tui(1)` for either workflow. The graphical installer does not
render the later upgrade and repair pages and refuses to advance into them.
