---
title: Rehearse the headless installer in a lab
type: how-to
description: Exercise an installerd conversation with msip-drive, a private socket, and a synthetic inventory, without installing a real disk.
related:
  - peios/msip-daemons/presentation-hints
  - peios/msip/turns
  - peios/disks-and-filesystems/installing-to-disk
---

Use this exercise to understand how `msip-drive` answers an `installerd`
conversation. It simulates an installation against an invented disk. For an
actual installation, use [Installing to disk](~peios/disks-and-filesystems/installing-to-disk).

> [!IMPORTANT]
> This is a source-derived rehearsal for installer commit
> [`0b687372ad41132668824216da51558bbfe53c01`](https://github.com/peios/installer/tree/0b687372ad41132668824216da51558bbfe53c01).
> The commands below have not been run to validate this guide against an image
> or released binaries. They are not a validated unattended-install recipe.

## 1. Prepare a private, disposable lab

Use a disposable lab instance with the matching `installerd` and `msip-drive`
binaries and an account allowed to use its lab socket. Create a **new private
directory**, restrict access using the lab system's access controls, and verify
that protection before starting. In each terminal, set `LAB` to that directory's
absolute path. The examples do not create or protect the directory for you.
Keep any saved output there and use only synthetic, nonsensitive data.

> [!WARNING]
> Never use the default `/run/installerd.sock`, a production socket, or an old
> lab socket. Even with `--dry-run`, daemon startup attempts to remove the
> supplied socket path, binds a socket there, and calls `sd set` to apply its
> descriptor. Descriptor failures only warn; they do not stop startup. A wrong
> path can disrupt access to an existing service. Inspect warnings and verify
> the private directory's protection; do not infer protection from a listening
> message alone.

These are [daemon startup effects](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/installerd/src/main.rs#L93-L105),
including [warning-only socket security](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/msip-serve/src/lib.rs#L649-L685).
Dry-run does not partition, format, or copy to the target disk; it is not a
side-effect-free or fully isolated mode.

Save this JSON as `inventory.json` in that directory:

```json
{"disks":[{"device":"/dev/msip-lab-only","model":"Synthetic MSIP lab disk","size":8589934592}],"controllers":[]}
```

The device is deliberately invented, not a real target to substitute later.
Omitted fields have [schema defaults](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/installerd/src/executor.rs#L18-L42).
Always supply the inventory: without it, dry-run still probes this machine's
block devices and controllers. With it, the executor uses the supplied
[synthetic inventory](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/installerd/src/executor.rs#L325-L402).

## 2. Start only the lab daemon

In the first terminal, with `LAB` set and verified:

```sh
installerd --dry-run --socket "${LAB:?Set LAB to the new private directory}/installerd.sock" \
  --inventory "$LAB/inventory.json"
```

Before starting a client, check that daemon output explicitly says
`installerd (dry run) listening on` followed by the exact private socket path.
Stop if the mode, path, or security checks are wrong. Leave the daemon running;
do not connect any other surface to it. Use a fresh directory and daemon for
each attempt rather than reusing an uncertain conversation.

## 3. Drive the synthetic conversation

In the second terminal, with `LAB` set to the same verified directory:

```sh
msip-drive --socket "${LAB:?Set LAB to the new private directory}/installerd.sock" --kind install \
  --set disk.target=/dev/msip-lab-only \
  --press act.install --press nav.next --press act.begin --press act.reboot
```

The last action is intentional: successful job execution produces a finish
page, not an ended conversation. The [dry-run executor's restart](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/installerd/src/executor.rs#L404-L452)
returns success **without rebooting the machine**. Do not remove `--dry-run`
or aim this sequence at a real service.

The expected sequence below is inferred from the
[flow](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/installerd/src/flow.rs#L621-L756)
and [client](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/msip-drive/src/main.rs#L141-L237),
not captured test output:

1. `bound:` reports a conversation ID and `attached=false`.
2. Turns advance through `mode`, `disk.choose`, `confirm`, `install.progress`,
   and `install.done`. The client waits on the progress turn.
3. `act.reboot` produces `== end: Complete Restarting the machine.` and the
   client exits with status `0` for that completed conversation.

`install.done` means the executor reported job success. `End Complete` means
the restart request was accepted or simulated; neither proves a restart or
successful disk boot. A real restart request is accepted when
[`svctl shutdown reboot` returns success](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/installerd/src/real.rs#L977-L1001).
In this lab, no disk installation or reboot takes place. Simulated percentages
do not verify partitioning, copying, image integrity, cleanup, or bootability.

## If it stalls or reconnects

**Do not blindly replay the command.** The client's `Start` implicitly joins
the daemon's one live, in-memory conversation and replays its current turn.
It has no conversation-ID guard and can consume a queued action immediately;
checking printed `attached=true` is not a barrier before an action runs.
If that appears in this fresh-lab exercise, stop the attempt and investigate
the unexpected state. The new private daemon and exclusive socket are the
precondition, not a check performed by the client.

Disconnecting the client **does not cancel the daemon's job**. A daemon restart
loses the in-memory conversation; a failed or completed conversation is also
cleared, so a later `Start` opens a new one. This is not durable recovery.
See [job lifetime](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/msip-serve/src/lib.rs#L239-L290)
and [disconnect and Start handling](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/msip-serve/src/lib.rs#L317-L508).

The driver consumes actions only on `NewTurn`, not same-turn `Updated` events.
A rescan or rejected input/reboot can therefore leave it waiting indefinitely,
even with more presses queued. It has no built-in timeout. An external watchdog
that stops the client neither cancels the job nor undoes changes. A client with
no presses is not a persistent observer: it exits with status `1` when an
actionable turn arrives. These limits follow from the
[client loop](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/msip-drive/src/main.rs#L141-L237)
and [same-turn rejection/patch handling](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/msip-serve/src/lib.rs#L558-L639).

## Keep the evidence in scope

- Protect inventory, console output, and logs. Do not add credentials or
  first-boot account setup to this exercise. `--set` values are command-line
  arguments; only daemon-marked secret elements are masked in the client trace.
  The client also prints only the last three stored log lines per update, so
  its output is not a unique, exhaustive audit journal. See
  [trace handling](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/msip-drive/src/main.rs#L191-L220)
  and [log rendering](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/msip-drive/src/main.rs#L258-L280).
- The checked-in [VM queue-inspection script](https://github.com/peios/installer/blob/0b687372ad41132668824216da51558bbfe53c01/tests/vm/install-and-check-queues.sh#L1-L45)
  omits `act.reboot` and logs, rather than asserts, the client's exit status.
  At this revision, a successful job reaches the actionable done turn with no
  presses left, so the client exits `1`. That script is not proof of a completed
  headless conversation; do not copy its real-device or mounting commands here.
- When finished, stop only the dedicated lab daemon. A real unattended-install
  guide still needs runtime validation of the shipped binaries, failure and
  reconnect handling, and independent checks of the installed disk and boot.
