---
title: Installer Interfaces
type: concept
description: The responsibilities of the installer engines and their terminal and browser interfaces.
---

Peios Installer separates the work that changes the machine from the
interface that asks you questions. You can use a terminal or browser while
the same service owns the installation conversation.

`installerd` selects and modifies disks. `oobed` creates the first account
and applies machine settings after installation. The services send questions,
progress, and results through the Multi-Surface Interaction Protocol
(MSIP), an independent protocol shared with their frontends.

The available interfaces have these roles:

| Task | Terminal | Browser |
|---|---|---|
| Fresh installation | `install-tui` | `installer-gxwi` |
| Upgrade or repair | `install-tui` | Not supported |
| First-boot setup | `oobe-tui` | `oobe-gxwi` |

The browser interfaces are GXWI overlays. Everyone who reaches the overlay
sees the same conversation, without first signing in. Coordinate with other
people using it and keep the machine on a trusted network until setup ends.
Closing a browser or terminal does not cancel a running installation.

The interface process does not perform disk or account operations itself.
Its configured identity still matters: the supplied first-boot terminal
service runs as SYSTEM so it can reach setup before any administrator exists.

See [Installing to Disk](~peios/installing-peios/installing-to-disk) for a
fresh installation and [Conversation Lifetime](~peios/setup-conversations/conversation-lifetime)
for reconnect and failure behavior. Command details belong to
`installerd(8)`, `install-tui(1)`, `installer-gxwi(1)`, and `oobed(8)`.
