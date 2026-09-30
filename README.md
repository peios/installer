# installer-gxwi

The graphical Peios installer.

It is what GXWI runs as its overlay on an install medium: one process for the
machine, which everyone who opens the machine's address in a browser is sent
to, with no logon. It installs nothing itself. `installerd` does that, as
SYSTEM, and asks its questions over a socket (MSIP). This program holds that
conversation, keeps what it has come to as one state, and serves a page that
draws it.

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
- **It is the installer's and no other's.** It knows `installerd`'s pages by
  their ids and draws each as the page it is. It is not a general front end
  to MSIP; `install-tui` in the `installer` repository is the one that draws
  any conversation.

## Where it has got to

The intro, and two pages: what to do with this machine, and which disk. The
disk page shows each disk, what is on it, and what an install would make of
it or which Peios system it holds. Its Next goes nowhere yet: the pages
after it are built one at a time, and an action that leads to a page not
drawn here says so instead of moving everyone on to it.

The disk page draws what `installerd` has only lately learned to say (each
row's `detail`), so it wants an `installerd` from a current `../installer`.
Against an older one it shows the rows and leaves the rest out.

It is not packaged and not on any image yet.

## Working on it

Rust 1.98.1 or newer, and sibling checkouts of `libpeios`, `pkm` and
`installer` beside this one (`dev/env.sh` says where it looks).

On the host, with no VM, against an `installerd` that only pretends to
install, and pretends to be the machine `dev/desktop.json` describes:

```sh
dev/host.sh                                        # http://127.0.0.1:7790/
node dev/browser/intro.mjs http://127.0.0.1:7790/  # the intro and the first page
node dev/browser/disk.mjs                          # the disk page
node dev/browser/states.mjs                        # what it shows when things go away
cargo +1.98.1 test
```

In the GXWI dev VM, as the overlay it really is, with disks for `installerd`
to find (`dev/disks.sh` makes them: a Windows disk, another Linux, a disk
holding a Peios release, an empty stick):

```sh
dev/boot.sh             # in another terminal; it stays up
dev/push.sh             # build this and installerd, and put both in the VM
dev/overlay.sh on       # http://127.0.0.1:7780/ is now the installer, with no logon
node dev/browser/intro.mjs
node dev/browser/machine.mjs   # the disk page, of the VM's own disks
dev/overlay.sh off      # the logon page and the desktop again
```

The conversation is `installerd`'s and outlives a browser, so a check that
stops part way leaves it on whatever page it had reached. `dev/push.sh`
starts it again from the first.

## License

MIT. The fonts in `page/fonts/` are under the licences beside them.
