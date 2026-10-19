---
title: Completing Setup
type: how-to
description: Creating an administrator account, choosing a machine name, and checking first-boot completion.
---

First-boot setup creates an administrator account and names the machine.
The terminal and browser interfaces use the same conversation, so you can
change interfaces without starting a second setup.

## Before you start

Make sure you have these prerequisites:

- An installed image that includes and activates the first-boot services.
- Access to the machine's console, or its trusted GXWI browser connection.
- A password for the administrator account you will create.

For browser setup, verify the machine's certificate through a trusted
console before entering a password. See
[Browser Setup](~peios/first-boot/browser-setup).

## Complete the conversation

1. Continue from the welcome page. Language and keyboard choices shown as
   unavailable are configured after setup in System Settings.
2. Review the network state. Continue without networking on the console,
   or supply a manual address for a wired interface if needed.
3. Enter the administrator account name and matching passwords, then select
   **Next**. Correct any field errors before continuing.
4. Enter a machine name or accept the suggestion.
5. Select **Finish** to create the account, set the machine name, and apply
   any manual network setting.
6. Sign in with the account and password you created.

The machine name is a single label of up to 63 letters, digits, and hyphens.
It cannot start or end with a hyphen or be `localhost`. Account names are
trimmed and must fit the local account store's rules. Reserved group names
and `peios-oobe-setup` are rejected.

Passwords cannot contain a carriage return or line feed. Setup rejects
those characters before starting account creation; it preserves Unicode,
spaces, and tabs in accepted passwords.

The browser's password strength meter offers advice. It does not replace
the daemon's validation or guarantee that a password is hard to guess.

## Check the result

Confirm that your account can sign in and that the machine uses the name
you chose. If you configured a manual address, check reachability from a
device on that network. A subsequent boot should reach sign-in without
repeating setup.

Setup removes its service definitions only after successful completion.
Closing the interface or encountering an error leaves setup available for
another boot. A sign-in prompt after an abandoned attempt does not prove
that an account was created.

If no usable account exists and setup repeatedly fails, use the installation
medium for diagnosis. Repair operations have specific limits; they are not
a general account-recovery mechanism.
