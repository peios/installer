---
title: Service Lifecycle
type: concept
description: How registry seeds start installer services and retire first-boot setup.
---

Installer packages carry registry seeds that define service startup.
A seed is inert until an image or administrator applies it. Installing a
package alone does not select the machine's setup lifecycle.

The `installerd-service` seed starts `installerd` as SYSTEM and restarts it
after failure. A restart restores a listening service, but does not recover
its previous in-memory conversation.

The `oobe-service` seed is intended for the installed system. It starts
`oobed` after the local account service and waits for its readiness signal
before starting the terminal frontend. `oobed` sends readiness after its
socket is bound and its browser setup has been prepared.

The terminal frontend takes `/dev/console` ahead of the normal sign-in
prompt. It also runs as SYSTEM in the supplied seed because the first
administrator has not yet been created. Splitting the frontend from the
engine separates responsibilities; the seed still determines authority.

On successful setup, `oobed` removes its service definition and the terminal
setup definition. On failure, it keeps them so another boot can retry.
Releasing the console lets the normal prompt appear even if setup failed.

## Socket access

The intended installation socket descriptor admits SYSTEM and
Administrators. Setup additionally admits its temporary browser account
when browser setup is offered.

Applying the descriptor is best effort: if `sd` is unavailable or fails,
the daemon warns and retains inherited access. A listening message does
not prove that the intended descriptor was applied. Verify the service's
runtime environment and socket protection before granting access.

## Console output

While a conversation is active, the privileged service suppresses routine
kernel console messages and restores the prior setting afterward.
Emergency kernel messages remain visible. Other services can still write
to the console; <kbd>Ctrl</kbd>+<kbd>L</kbd> asks the terminal frontend to
repaint if those messages disturb the form.
