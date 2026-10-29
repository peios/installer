# Building Peios Installer

Use pekit to build the installer's seven programs and run their tests.
Pekit builds each crate on its own, so a change rebuilds only the crates it
reaches. The supported development host is x86-64 Linux.

## Before you start

You need pekit and Git. Everything else is pinned and fetched by pekit on
the first build: the Bootlin C toolchains, Rust 1.98.1 with Clippy and the
musl standard library, busybox, Node.js 22, libclang, and the crates
`Cargo.lock` selects. The first build needs network access; later builds
find what they need in pekit's store, and every step runs without network
access in a sandbox that holds only its declared inputs.

The build files use the gcslib libraries named in `workspace.gcsl` and the
build files, and the libraries of peios-rs and the Peios SDK. `pekit lock`
pins their commits in `pekit.lock`.

## Build

```sh
pekit build installerd install_tui msip_drive oobed oobe_tui installer_gxwi oobe_gxwi
```

| Target | Output |
|---|---|
| `installerd` | Installation daemon |
| `install_tui` | Terminal installation interface |
| `msip_drive` | Scripted answers for an installation |
| `oobed` | First-boot setup daemon |
| `oobe_tui` | Terminal first-boot interface |
| `installer_gxwi` | Browser installation interface |
| `oobe_gxwi` | Browser first-boot interface |

Pekit prints where each output is, under `pekit-out/<target>/`. The five
daemons and terminal programs are static, position-independent musl
programs with no ELF interpreter, linked with full RELRO and immediate
binding. `installer-gxwi` and `oobe-gxwi` are built for glibc and load
`libpeios.so.0` by its shared object name (SONAME). Internal Rust
libraries and the pinned MSIP dependency are not separate installer shared
libraries.

## Test

```sh
pekit test
```

| Test | Check |
|---|---|
| `installerd`, `install_tui`, `msip_drive`, `msip_tui`, `msip_serve`, `oobe_tui`, `oobed`, `installer_gxwi` | Each member's unit and integration tests |
| `musl` | The five static programs' tests, compiled as the static musl programs they ship as |
| `doctests` | Each member's documentation tests |
| `clippy` | Clippy's checks of every target of every member, with every warning denied |
| `browser_harness` | The development scripts' wiring, with stand-in programs, and the browser driver's own tests, with Node.js |

installerd's tests that run shell commands are given busybox's `/bin/sh`.
A test whose inputs did not change is found in the store and not run again.
Unit tests, the browser harness's tests, and documentation checks do not
establish runtime installation or first-boot qualification.

For the full contributor check, install the pinned PSC tools into a
directory outside the repository, then run the shared entry point:

```sh
python3 tools/prepare-style-tools.py "$HOME/.cache/installer-style"
export PEIOS_STYLE_BIN="$HOME/.cache/installer-style/bin"
tools/check
```

`tools/check` checks owned source formatting and lint with rustfmt,
Prettier, ESLint, Ruff, ShellCheck, and shfmt, then runs `pekit test`. The
style checks use the host's Rust 1.98.1 formatter through rustup. The tool
versions and hashes are recorded in `tools/style-tools.lock.json`.
GitHub's workflow runs the style checks alone; builds and tests run with
pekit. Set `TRAIL` to a Trail executable and run `tools/check-docs` for
documentation validation.

To check that the programs build reproducibly, build them twice from the
same inputs and compare the outputs:

```sh
pekit build --rebuild --check-reproducible installerd install_tui msip_drive oobed oobe_tui installer_gxwi oobe_gxwi
```

## How the build is planned

`build.gcsl` plans the Cargo workspace twice with the gcslib rust library's
`workspace`: for `x86_64-unknown-linux-musl`, for the static programs, and
for glibc, for the browser interfaces. Cargo works out each program's
build, unit by unit, and pekit compiles each unit as a step. Planning reads
the manifests, `Cargo.lock`, and which files exist, never the sources, so
editing a source plans nothing again.

Keep Rust dependency versions in the Cargo manifests and `Cargo.lock`, and
update the lockfile deliberately with Cargo. `peios-sys` generates its
bindings from the SDK's headers with bindgen and links libpeios, so it, and
every crate that depends on it, is given what peios-rs's library says it
needs, `crate_settings()`: libpeios and its headers, built from the Peios
SDK's own files. The rust library fails a build whose `Cargo.lock` takes
peios-rs's crates at another commit than `pekit.lock` pins its library at.

## Not built with pekit yet

Packages, debug information, and builds with the native Peios toolchain,
which hardens programs with control-flow enforcement technology (CET), are
not yet built with pekit. The package definitions, their payload checks,
and the native build settings from the Bazel build are kept outside this
repository until they are. The native payload checks required PIE, full
RELRO, non-executable stacks, packed relative relocations, split debugging
information, and, for the graphical programs, CET properties. Static musl
programs keep their CET-note exemption: Rust code is instrumented, but musl
does not enable shadow stacks at startup (PEI-1159).

## Exercise browser interfaces

The host helpers use private sockets and dry-run daemons. From the repository
root, `installer-gxwi/dev/host.sh` serves the installation at
`http://127.0.0.1:7790/`, and `installer-gxwi/dev/host-oobe.sh` serves setup
at `http://127.0.0.1:7791/`. They build the programs, and libpeios for the
browser interfaces to load, with pekit.

The installation helper reads `installer-gxwi/dev/desktop.json` by default.
Set `MACHINE` to another synthetic inventory or `FAIL_AT` to a simulated
phase failure. The setup helper reads `installer-gxwi/dev/net-status.txt`;
set `NET_STATUS` to another fixture. Dry-run conversations do not verify
partitioning, account creation, or bootability.

Browser checks live in `installer-gxwi/dev/browser/`. Tests that start a
browser require Playwright and a browser installation. Deterministic
recovery-controller tests use Node.js without a browser. Check each script's
inputs before running it; the VM installation checks intentionally erase
disposable guest disks.

## Qualify a disposable guest

The VM harness accepts an explicit `IMAGE` and executable `VM_RUNNER`.
The runner receives the image path followed by individual QEMU arguments;
it must isolate writes from the supplied image and support guest restarts.
`installer-gxwi/dev/boot.sh` creates disposable fixture disks and passes them
to that runner. No image composer is part of this repository's maintained
build interface.

Keep real disks, credentials, and production service sockets out of the
fixture. Record the medium, guest, source, SDK, and package identities.
Verify installation results independently: package state, registry queues,
boot files, disk boot, first-boot completion, and subsequent sign-in each
need evidence. A completed simulated conversation proves none of these.

The optional `RELEASE_ROOT` supplies an unpacked system containing
`usr/lib/os-release` and `var/state/peipkg/db.sqlite` for fixture preparation.
`INSTALLED_DISK` supplies a read-only backing disk for a disposable qcow
overlay. Keep those inputs separate from disks whose changes must persist.

`installer-gxwi/dev/push.sh` requires `VM_SHARE`, a host directory used by
the guest's hot-reload contract. It builds and copies the matching installer
daemon and browser frontend there. `installer-gxwi/dev/overlay.sh` requires
`VM_GUEST`, an executable that accepts a guest command string, to change the
development guest's overlay configuration. These helpers target an explicitly
chosen disposable VM; they do not discover or provision a release image.
