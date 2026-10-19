---
title: Rehearsing Installer Conversations
type: concept
description: The scope and limits of synthetic installer and first-boot rehearsals.
---

Dry-run mode exercises the installation or setup conversation without
performing its disk or account operations. It helps check frontend behavior
and error handling, but it does not qualify a boot medium or installed
operating system.

Use a private directory reserved for the rehearsal and an explicit socket
path.
Daemon startup removes the existing entry at that path, binds its socket,
and attempts to apply its access descriptor even in dry-run mode. Do not
point a rehearsal at a production service socket.

For installation, supply a synthetic inventory as well as `--dry-run`.
Without an inventory, the daemon still probes the host's disk inventory.
The checked-in frontend fixtures describe invented machines; the developer
harness selects them explicitly.

A simulated phase failure can exercise an error page without damaging a
filesystem. A simulated restart acknowledges the request without restarting
the machine. Neither checks the real executor's cleanup, disk state, or
boot behavior.

`msip-drive` can answer named inputs and queue actions, but it does not guard
against joining an existing conversation. It waits on progress and consumes
actions on new turns. It has no built-in deadline, and a rejected answer or
same-turn update can leave it waiting. Stopping the driver does not stop
the daemon's job.

Keep synthetic inputs and captured output in the disposable fixture.
Credentials passed through `--set` are exposed in process arguments; the
`--set-file` option avoids that exposure but still requires a protected
input file. Do not use real credentials in a rehearsal.

The repository's build guide describes its host and VM harnesses. For the
complete command interfaces, read `installerd(8)`, `oobed(8)`, and
`msip-drive(1)`.
