---
title: Complete first-boot setup
type: how-to
description: Create the first administrator account, name the machine, optionally set a wired address, and verify sign-in after installing Peios.
related:
  - peios/installing-peios/on-a-pc
  - peios/disks-and-filesystems/installing-to-disk
  - peios/managing-local-principals/creating-accounts
  - peios/networking/configuring-profiles
  - peios/signing-in-from-a-browser/the-certificate
  - peios/services-and-jobs/triggers-and-timers
  - peios/peiso/editions-and-upgrades/release-toml
---

Complete first-boot setup after installing Peios to disk. It creates the first administrator account, sets the machine's name, and optionally applies a manual wired-network address. When it succeeds, you can sign in with your new account and setup does not run again.

The installer copies the system first; these questions are asked on the installed system's first boot. Setup is available on the console and, on images that include `oobe-gxwi`, in a browser.

## Before you start

- Boot from the installed disk, with the install medium removed after installation has completed.
- Choose an account name, a non-empty password, and a machine name. Their rules are listed below.
- Keep the install medium for recovery until you have successfully signed in.
- You do not need a network for console setup. For browser setup, use a wired connection on a trusted network and verify the machine's certificate.
- Expect an English setup interface and US console keyboard layout. Language and keyboard choices are made after setup, so take care when entering a password with a different physical keyboard layout.

> [!WARNING]
> Until setup is complete, anyone who can open the machine's address or use its console can choose the administrator account and password. Keep an unconfigured machine off networks you do not trust.

## What it asks

### 1. Language and keyboard

The page shows the current language and keyboard information but does not change them. Setup is in English and the console uses the US layout until setup finishes. Afterwards, open **System Settings → Language & Keyboard**. See [the settings overview](~peios/desktop-settings/overview) for the distinction between System Settings, Desktop Settings, and personal settings.

In the terminal, the Language and Keyboard choices are disabled, but each
shows its explanation beneath it without needing focus, so this guidance is
visible there too.

### 2. Network

The network page is informational; lack of connectivity does not block setup. It shows what `net status` reports for each interface: connected, not connected, or not used. Choose **Check again** after connecting a cable. In the browser, the interface carrying your current connection is marked.

Wi-Fi joining is shown but disabled. Continue without network configuration, or choose **Configure manually** for a wired interface.

For a manual address:

1. Enter an address with its network prefix, such as `192.168.1.20/24`.
2. Add a gateway and name servers if needed. A gateway must be on the address's network.
3. Save and check the network page's confirmation that the setting is kept for later.

Saving validates the values but **does not change the active address yet**. Setup applies the address after creating the account and setting the machine name, so the browser can keep reaching the current address while you answer the remaining questions. Note the new address before finishing.

### 3. Account and password

This account is the machine's first **administrator**. Choose a password you intend to use for sign-in; the setup page refuses an empty password or mismatched confirmation.

Account names must:

- Be non-empty, plain ASCII, and no longer than 256 characters.
- Contain none of these characters: `@`, `\`, `/`, `:`, or `,`.
- Not be `Administrators`, `Users`, `Guests`, `Everyone`, or `Authenticated Users`.
- Not be `peios-oobe-setup`, the temporary account reserved for browser setup.

Spaces inside a name are allowed; spaces around it are trimmed. Validation happens when you choose **Next**, before the account is created. A rejected field explains what to change.

In a browser, typed account details remain in your browser until **Next** and are not shown to other people viewing the setup conversation. The page previews the account name, checks matching passwords, and rates apparent password strength from **Easily guessed** to **Very strong**. The strength meter is advice, not an acceptance rule: any non-empty matching password is accepted.

### 4. Machine name

Accept the suggestion or enter one label with:

- Letters, digits, and hyphens only, up to 63 characters.
- No leading or trailing hyphen, dots, or spaces.
- A name other than `localhost`.

Spaces around the name are trimmed. This is the name the machine uses for itself and supplies to DHCP. Domain joining is shown but disabled.

The browser validates as you type, previews a prompt such as `jack@workshop:~$`, and offers suggested names. **Shuffle** generates more suggestions. The daemon validates the name when you choose **Finish**.

### Finish and verify sign-in

Choose **Finish** once the account, name, and any manual network setting are correct. Setup shows each change as it applies:

1. Create the account.
2. Set the machine name.
3. Apply a manually configured network address, if supplied.

On the console, setup releases the terminal and the login prompt appears. In a browser, **Setup is complete** is shown briefly before the page transitions to sign-in, retaining the same backdrop. Sign in with the account and password you just created.

Check that:

- The new account can sign in.
- The machine name is the one you chose.
- If you configured a manual address, the machine is reachable there from a device on the appropriate network.
- A later boot goes directly to sign-in rather than repeating setup.

For additional accounts and account administration, continue to [Creating accounts](~peios/managing-local-principals/creating-accounts).

## In a browser

Open `https://<machine-address>:7780`, unless the image uses a different GXWI port. The graphical surface must be installed at `/bin/oobe-gxwi`; without it, use the console.

While setup is pending, GXWI shows setup instead of its sign-in page. The console and browser share one conversation, so you can start on one and finish on the other. Coordinate with anyone else who has it open.

### Verify the certificate before entering a password

GXWI uses HTTPS with a certificate the machine makes for itself. Check its SHA-256 fingerprint against a trusted reading from the machine's own console:

```sh
cat /var/state/gxwi/certificate.sha256
```

Do not use a page delivered over the unverified connection as the source of that fingerprint. If the fingerprints differ, do not enter a password.

`installerd` attempts to carry the medium's GXWI certificate into the installed
system from either surface, but installation can succeed even if the transfer
is skipped or fails. Compare the certificate actually presented with the
fingerprint you verified on the medium; do not treat installation success as
proof that it is unchanged. A changed fingerprint needs a fresh check through
the installed machine's trusted console before you enter credentials. See
[certificate carry-over](~peios/signing-in-from-a-browser/the-certificate#installation-and-first-boot).

The older `peios-install` path, or a medium without GXWI, does not carry that key across. If you have not already verified the installed machine and cannot obtain its fingerprint through trusted console access before creating an account, complete setup on the console first. Then sign in locally to verify the certificate before using browser sign-in. See [the machine's certificate](~peios/signing-in-from-a-browser/the-certificate) for the full trust procedure.

### If the network address changes

Setup keeps the current address until its final apply step. If you change the interface your browser uses, the page then checks both the old and new addresses and follows the machine when it answers. At the new address, the sign-in page starts a new backdrop rather than retaining the old page's animation.

If the browser cannot reach the new network, the page reports it; use the machine's screen to check its address. Open the new address from a device that can reach it.

> [!NOTE]
> The PC installation guide records a **2026.8** HTTPS limitation: after a manual address change, the page can remain on its starfield instead of reconnecting. Once setup finishes, open `https://<new-address>:7780` manually. A browser may warn again for the new address; verify the fingerprint rather than assuming that any new warning is safe. See [First boot on a PC](~peios/installing-peios/on-a-pc#first-boot).

The browser's waiting page also depends on certificate trust. If the certificate was only bypassed at a warning rather than imported, a restart can produce a browser error instead of an automatic reconnect. Reopen the address manually and follow the certificate guide.

### The temporary setup account

Before setup, the machine has no normal account for a graphical session. `oobed` creates `peios-oobe-setup` with no password and no groups, and its socket admits that account. The browser overlay uses it to draw the setup conversation.

On successful setup, `oobed` removes the overlay, restores GXWI sign-in, and removes the temporary account and its home directory. Failed setup leaves them in place for the next attempt.

> [!WARNING]
> While the temporary account exists, any sign-in mechanism accepting a passwordless account can sign in as it. It has no groups; its intended access is setup itself, which is already available at the machine's address. Network isolation remains important until setup has finished.

The account-name page reserves `peios-oobe-setup` because setup preserves an account that already exists; choosing that name would otherwise leave no usable account after cleanup.

## If setup does not finish

| Symptom | Next step |
|---|---|
| Account, password, or machine name is rejected | Read the field error and compare it with the rules above. Validation happens before final application. |
| No network or Wi-Fi joining is disabled | Continue on the console; a network is not required. Use wired networking for browser access. |
| Browser stays on the old address or a starfield after a manual network change | Check the address on the machine's screen and open the new HTTPS address yourself. See the 2026.8 caveat above. |
| Browser setup is absent | The image may not include `oobe-gxwi`; use the console. If setup itself is absent, check the edition's provisioning choice below. |
| Setup was closed with Esc or failed | It is not retired. Restart to be offered setup again. A returned login prompt does not prove account creation succeeded. |
| Setup repeatedly fails and no usable account exists | Boot the install medium for recovery. Do not assume you can sign in to repair setup locally. |
| Console text is damaged | Press **Ctrl+L** to repaint. |
| Form no longer fits after resizing a serial-terminal window | The size was read once at startup. Restart with the desired terminal size, or use the current form size. |

### It runs once

Only a successful setup removes the `oobed` and `oobe-tui` service definitions. A failed or abandoned run leaves them so the next boot can retry. It does not retire the only setup path before an account has been successfully provisioned.

A machine whose setup keeps failing may have no account you can use. The install medium is the recovery route, as for a system that will not boot. [Installer repair operations](~peios/disks-and-filesystems/installing-to-disk#repairing-an-installed-system) describe what the medium can repair and their limits; they are not a guarantee that every setup failure is automatically repairable.

## How it gets the console

This section explains service behavior useful when diagnosing startup or display problems.

| Service | Responsibility |
|---|---|
| `oobed` | Runs as SYSTEM to create the first account and set the machine name. Serves the [MSIP](~peios/services-and-jobs/overview) conversation on `/run/oobed.sock`. |
| `oobe-tui` | Draws the console form without privileged rights of its own. |

Setup and the [login prompt](~peios/signing-in/the-login-command) both use `/dev/console`. Setup has higher [`TTYPrecedence`](~peios/services-and-jobs/triggers-and-timers), so peinit gives it the console and does not start the login prompt at that point.

When the surface exits, whether successfully, through Esc, or after a crash, it releases the console. The login prompt's `tty:released` trigger then starts it. The terminal is not left without an owner, even though an unsuccessful setup may still leave no usable account.

### How big the form is

A serial byte stream carries no terminal geometry. The surface moves the cursor beyond the bottom-right corner and reads back its clamped position to discover the size. The installer uses the same renderer.

- The size is read once when the form starts. Later window resizing is not reported to the guest and does not resize the form.
- A terminal that does not answer falls back to **80×24**, including captured output or a serial port with no responding terminal.

### The console goes quiet while a form is open

The daemon lowers kernel console output while the conversation is open and restores it when the conversation ends, including if the daemon dies. It does this in the privileged engine rather than in the unprivileged surface. `installerd` uses the same approach.

Only `KERN_EMERG` kernel messages continue to print, so a panic is not hidden. Peinit and other services can still write directly to the console. Those writes can disrupt the form, so page changes repaint it in full and **Ctrl+L** requests a full repaint.

## Where it comes from

`dev.peios.oobe` ships the inert `oobe-service` registry seed. An edition opts into it through [`install_autoapply`](~peios/peiso/editions-and-upgrades/release-toml), so it runs on the installed system rather than taking the console away from the live installer.

An image without `dev.peios.oobe`, or an edition that omits that seed, boots directly to a login prompt using whatever accounts its own seeds provisioned. Do not assume every custom image provides this setup flow.

A manual network choice is saved as an ordinary [profile and rule](~peios/networking/configuring-profiles): `Profiles\default\manual-<interface>` plus an exception under `Rules\Interface\wired`, naming the interface by stable ID. After setup, you can change or delete those settings like any other network profile and rule.
