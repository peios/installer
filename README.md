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
- **It is the installer's and no other's.** It knows `installerd`'s pages by
  their ids and draws each as the page it is. It is not a general front end
  to MSIP; `install-tui` in the `installer` repository is the one that draws
  any conversation.

## Where it has got to

The intro, and the first page: what to do with this machine. Choosing does
nothing yet. The pages after it are built one at a time, and until a page is
drawn here the installer says so instead of moving the conversation on.

It is not packaged and not on any image yet.

## Working on it

Rust 1.98.1 or newer, and sibling checkouts of `libpeios`, `pkm` and
`installer` beside this one (`dev/env.sh` says where it looks).

On the host, with no VM, against an `installerd` that only pretends to
install:

```sh
dev/host.sh                                        # http://127.0.0.1:7790/
node dev/browser/intro.mjs http://127.0.0.1:7790/  # the intro and the first page
node dev/browser/states.mjs                        # what it shows when things go away
cargo +1.98.1 test
```

In the GXWI dev VM, as the overlay it really is:

```sh
../gxwi/dev/boot.sh     # in another terminal; it stays up
dev/push.sh             # build, and put it in the VM
dev/overlay.sh on       # http://127.0.0.1:7780/ is now the installer, with no logon
node dev/browser/intro.mjs
dev/overlay.sh off      # the logon page and the desktop again
```

## License

MIT. The fonts in `page/fonts/` are under the licences beside them.
