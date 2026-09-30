# installer-gxwi

The graphical Peios installer, and first-boot setup in a browser.

`installer-gxwi` is what GXWI runs as its overlay on an install medium: one
process for the machine, which everyone who opens the machine's address in a
browser is sent to, with no logon. It installs nothing itself. `installerd`
does that, as SYSTEM, and asks its questions over a socket (MSIP). This
program holds that conversation, keeps what it has come to as one state, and
serves a page that draws it.

`oobe-gxwi` is the same for `oobed`, first-boot setup, on a machine that has
been installed and not set up. `oobed` makes it GXWI's overlay while setup is
pending, running as a passwordless account it makes for the purpose, and
takes it away again when setup is done. The two are one program built twice,
because they are one page: a restart from the installer lands in setup.

- **The process holds the state.** Which page is showing and what is on it
  live here, in `src/`. Every browser is sent the same thing over a websocket
  and sent it again whenever it changes.
- **The page draws it.** `page/` is the whole front end: how a page arrives
  and leaves, and the intro, are the browser's business. Everything it needs
  is inside the binary, because a machine being installed may have no route
  out.
- **What is pressed goes to the process.** A browser asks the process to press
  an action or choose a disk, and the process answers `installerd`. Nothing
  comes back but the state, so two people looking see the same disk chosen
  and are moved on together.
- **A job that fails ends the conversation.** What it ended with stays on the
  page until someone asks to start again, and then the process opens another
  conversation. A job that finishes goes on to `installerd`'s offer to
  restart the machine, which `installerd` does, not this.
- **The page outlives the machine.** A restart takes this process down with
  the machine, and the page, a browser elsewhere, is what is left: it goes
  down with it, waits, and says what came back.
- **It is the installer's and no other's.** It knows `installerd`'s pages by
  their ids and draws each as the page it is. It is not a general front end
  to MSIP; `install-tui` in the `installer` repository is the one that draws
  any conversation.

## Where it has got to

The intro, and an installation from its first page to the machine restarted
into what it installed: what to do with this machine, which disk, whether the
disk is really to be erased, the installation as it runs, and the restart.

- **The disk page** shows each disk, what is on it, and what an install would
  make of it or which Peios system it holds.
- **The confirmation** shows the disk chosen and what would be missed of it.
  Its button is held down rather than pressed, and holding it all the way
  begins the installation.
- **The installation** shows the phases `installerd` reports and one figure
  for the whole, the disk being made, and what the job says of itself. It
  ends finished or stopped, in `installerd`'s words, and what the job said
  can be saved whole from `/log.txt`. Finished, it offers Reboot now.
- **The restart**: the page leaves and the stars gather back into the mark,
  which sleeps while the machine is away. The page asks the address for
  `/hello`, which only this installer and first-boot setup answer, with the
  machine's boot id, and lands on what comes back: first-boot setup, whose
  page is loaded in this one's place and wakes the sleeping mark; Peios,
  with first-boot setup on the machine's own screen only; the installer
  again, on another boot, which is the machine starting from the medium; or,
  after three minutes of nothing, what the machine's screen might be showing.

Of first-boot setup, the welcome and the network are drawn.

- **The welcome**: the greeting, going round the languages, and the language,
  which `oobed` shows and cannot yet let be chosen. The keyboard layout
  `oobed` asks about is the console's, and is not drawn: in a browser the
  keyboard is the browser's.
- **The network**: each interface, where it has got to (connected, not
  connected, not used), and its addresses, way out and name servers, as
  `oobed` reads them from `net status`. The one whose address the page was
  opened by is marked as the way in. Check again has `oobed` ask again and
  changes the page in place; joining a wireless network is shown greyed,
  with why.
- **An address by hand**: a wired interface, its address, gateway and name
  servers, which `oobed` checks and keeps for the end of setup. The page
  says it is applied then and not now, and the network page shows what is
  kept, marks the interface, and offers to change it or give it up. What is
  typed is the browser's own until Save, and goes with it.

An upgrade's and a repair's pages are not drawn yet, nor the rest of
first-boot setup. An action that leads to a page not drawn here says so
instead of moving everyone on to it: today, the disk page's Next for an
upgrade or a repair, and the network page's Next.

The pages draw what `installerd` has only lately learned to say (the
`detail` of each row of disks, of the confirmation's sentence and of the
running job's), so they want an `installerd` from a current `../installer`.
Against an older one they show what it says in words and leave the rest out.

Neither is packaged or on any image yet. Until `oobe-gxwi` is installed at
`/bin/oobe-gxwi`, `oobed` offers setup on the console only.

## Working on it

Rust 1.98.1 or newer, and sibling checkouts of `libpeios`, `pkm` and
`installer` beside this one (`dev/env.sh` says where it looks).

On the host, with no VM, against an `installerd` that only pretends to
install, and pretends to be the machine `dev/desktop.json` describes:

```sh
dev/host.sh                                        # http://127.0.0.1:7790/
node dev/browser/intro.mjs http://127.0.0.1:7790/  # the intro and the first page
node dev/browser/disk.mjs                          # the disk page
node dev/browser/confirm.mjs                       # the confirmation
node dev/browser/progress.mjs                      # an installation, finishing and failing
node dev/browser/restart.mjs                       # the restart, and what comes back (LOST=1: and nothing)
node dev/browser/states.mjs                        # what it shows when things go away
cargo +1.98.1 test
```

First-boot setup the same way, against an `oobed` that only pretends:

```sh
dev/host-oobe.sh                                   # http://127.0.0.1:7791/
node dev/browser/welcome.mjs                       # the welcome, and waking into it
node dev/browser/network.mjs                       # the network page
```

The network the pretended `oobed` reports is what `dev/net-status.txt` says
`net status` prints (`NET_STATUS=FILE dev/host-oobe.sh` for another). It is
read again each time the page checks, so editing it and pressing Check again
is a cable plugged in or pulled. `network.mjs` starts its own `oobed` and
`oobe-gxwi`, and needs nothing else running.

`restart.mjs` starts its own `oobed` and `oobe-gxwi` to come back as, so it
wants `oobed` built too (`cargo +1.98.1 build -p installerd -p oobed -p
msip-drive` in `../installer`).

A pretended installation takes about a quarter of a minute.
`FAIL_AT=copy dev/host.sh` makes each one fail part way through that phase,
to look at an installation that goes wrong.

In the GXWI dev VM, as the overlay it really is, with disks for `installerd`
to find (`dev/disks.sh` makes them: a Windows disk, another Linux, a disk
holding a Peios release, an empty stick). It starts through the firmware, the
stick first and the medium last, and a restart is a restart, so what is
installed on the stick can be restarted into:

```sh
dev/boot.sh             # in another terminal; it stays up
dev/push.sh             # build this and installerd, and put both in the VM
dev/overlay.sh on       # http://127.0.0.1:7780/ is now the installer, with no logon
node dev/browser/intro.mjs
node dev/browser/machine.mjs   # the disk page and the confirmation, of the VM's own disks
node dev/browser/install.mjs   # a real installation, onto the empty stick: it erases it
node dev/browser/install.mjs --reboot   # the same, and then restarted into
dev/overlay.sh off      # the logon page and the desktop again
```

Restarted into, the VM runs what was installed on the stick, and is an
installer again only once the stick is empty (delete `target/disks/blank.img`
while no VM has it) and `dev/boot.sh` has started it over.

What is installed is the medium's own system, and the release medium has no
first-boot setup in a browser yet. `dev/image.sh` builds one that does, with
this checkout's programs and those of `../installer` and `../gxwi` injected
into it (`dev/image.toml`), and `IMAGE=target/image dev/boot.sh` boots it.
Installed from that and restarted into, the page goes on to setup's welcome.

The conversation is `installerd`'s and outlives a browser, so a check that
stops part way leaves it on whatever page it had reached. `dev/push.sh`
starts it again from the first.

## License

MIT. The fonts in `page/fonts/` are under the licences beside them.
