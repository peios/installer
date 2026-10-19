# Building Peios Installer

Bazel builds all seven programs, runs the repository checks, and assembles
canonical package blobs and manifests. Linux development builds and native
Peios builds are separate configurations.

## Prepare a development SDK

Use Linux on x86-64 and the pinned launcher at the repository root. The
browser programs require libpeios and matching kernel user API headers.
Create an SDK from explicitly selected source revisions:

```sh
tools/prepare-dev-sdk <libpeios-source> <kernel-uapi> <sdk-output>
```

Replace the placeholders with absolute paths. `<kernel-uapi>` contains the
exported `pkm/` headers, and `<sdk-output>` must not exist. Set `PEIOS_SDK`
to that output directory before invoking Bazel. The helper does not establish
compatibility between the SDK and a running Peios kernel.

## Build and test

From the repository root, run:

```sh
./bazel build //:artifacts
./bazel test //...
./bazel build //:packages
```

The five core programs use static musl builds. `installer-gxwi` and
`oobe-gxwi` use the GNU target and dynamically link to libpeios. Internal
Rust libraries and the pinned MSIP dependency are not separate installer
shared libraries.

The package store is `bazel-bin/packages.store`. It separates runtime
programs, debugging data, source, and the installer and first-boot feature
packages. Feature packages carry registry seeds; producing these packages
does not apply those seeds or modify the host.

Run `tools/check` for style and code checks. Set `TRAIL` to a Trail executable
and run `tools/check-docs` for documentation validation. Package assembly,
source tests, and documentation checks do not establish runtime installation
or first-boot qualification.

## Build for Peios

Native builds use the patched Peios Rust toolchain and an explicitly
prepared native SDK. Within that environment, `tools/prepare-native-sdk`
creates an SDK view from installed libpeios and kernel headers. Set
`PEIOS_SDK` and `PEIOS_RUSTC`, then select `--config=peios` in Bazel.

Native payload checks require PIE, full RELRO, non-executable stacks, packed
relative relocations and split debugging information. The graphical programs
also require CET properties. The existing static-musl CET-note exemption is
preserved: Rust code is instrumented, but musl does not enable shadow stacks
at startup (PEI-1159).

The environment also needs the Rust musl target libraries and musl sysroot
for the five static programs. Preserve the exact SDK, compiler, dependency,
and source identities with each build's logs and package hashes.

Acquire dependencies before entering an offline environment:

```sh
tools/offline-build acquire <snapshot-directory>
```

Use an absolute directory outside the checkout. Inside the prepared Peios
userspace, build and test that snapshot in an isolated work directory:

```sh
tools/offline-build build <snapshot-directory> <work-directory>
tools/offline-build test <snapshot-directory> <work-directory>
```

The work directory is writable scratch space. These commands require the
native environment described here; they are not a VM provisioning command.

## Check release reproducibility

Build the same frozen source and dependency snapshot in two new native work
roots. Preserve both source manifests and build logs. The package stage records
the unstripped programs used to produce the shipped binaries:

```sh
tools/offline-build build <snapshot-directory> <work-a>
tools/offline-build build <snapshot-directory> <work-b>
python3 tools/check-reproducibility.py \
    <work-a>/output/execroot/_main <work-b>/output/execroot/_main
```

The comparison requires all seven programs, matching versions, and identical
unstripped bytes, including debugging information. It rejects reuse of the same
build root. Build provenance also records the selected GNU and musl Rust
toolchain inputs, compiler version, and SDK input identity. These checks do not
replace installation and first-boot qualification in a disposable Peios guest.

## Exercise browser interfaces

The host helpers use private sockets and dry-run daemons. From the repository
root, `installer-gxwi/dev/host.sh` serves the installation at
`http://127.0.0.1:7790/`, and `installer-gxwi/dev/host-oobe.sh` serves setup
at `http://127.0.0.1:7791/`. They require the same explicit SDK as the build.

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
