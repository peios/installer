---
title: Browser Setup
type: concept
description: Shared browser state, certificate verification, and temporary setup access.
---

The browser setup page is a GXWI overlay available before normal sign-in.
`oobed` owns the setup operation, while `oobe-gxwi` presents the conversation
and forwards your answers.

## A shared conversation

All connected browsers observe the same page and completed choices.
Coordinate with anyone else using setup. Typed account fields stay in the
browser until submission, and the frontend does not broadcast the password
to other viewers.

The overlay is available without normal sign-in. Before offering it,
`oobed` creates or reuses the passwordless account `peios-oobe-setup` and
includes that account in the setup socket's intended access descriptor.
The account has no groups. A sign-in mechanism that accepts passwordless
accounts can also accept this account while it exists.

Successful setup removes the overlay and the temporary account and its
home directory. An unfinished setup keeps them for another attempt.
Keep the machine on a trusted network during this period.

## Check the connection

Use the address advertised on the machine's own console. When GXWI uses a
self-signed certificate, compare its SHA-256 fingerprint with the trusted
console's reading before sending credentials. A page received over the
unverified connection cannot authenticate that same connection.

The console reads the fingerprint from
`/var/state/gxwi/certificate.sha256`. The browser installer deliberately
does not present that value as a source of trust.

A fresh installation attempts to copy the medium's GXWI key and certificate
to the installed system. Failure to copy them does not fail installation.
Verify the certificate actually presented after restart; installation
success does not establish that the certificate stayed the same.

Direct development listeners created with `--listen` serve HTTP without
GXWI's TLS termination. Use those listeners only with synthetic credentials
in an isolated development environment. Production certificate handling
belongs to GXWI.

## Returning to sign-in

After setup completes, `oobed` removes the overlay and GXWI restores its
sign-in page. The browser waits for that transition. If automatic return
fails, check the machine's console and reopen its verified address.

A manual network change is applied at the end of setup. The browser can
lose access if its device cannot reach the resulting network, or if the
new address needs a fresh certificate trust decision. Verify the address
and certificate before entering credentials again.
