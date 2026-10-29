# Peios Installer

Peios Installer installs the operating system and completes first-boot setup.
This repository contains the privileged engines, terminal interfaces, and
GXWI browser interfaces, with one component version and separate packages.

`installerd` owns installation, upgrade, and repair operations. `oobed` owns
account creation and machine setup. Frontends present their conversations
through the Multi-Surface Interaction Protocol (MSIP); the protocol remains
an independent dependency pinned in `Cargo.toml` and `Cargo.lock`.

The shipped programs are:

| Engine | Terminal interface | Browser interface |
|---|---|---|
| `installerd` | `install-tui` | `installer-gxwi` |
| `oobed` | `oobe-tui` | `oobe-gxwi` |

`msip-drive` supplies scripted answers. `msip-serve` and `msip-tui` are
internal Rust libraries. Browser assets are embedded in the frontend binaries.
The browser interfaces link to libpeios; they do not need the Rust GXWI SDK.

## Build and check

Pekit builds the programs and runs their tests, as
[Building](docs/building.md) describes:

```sh
pekit build installerd install_tui msip_drive oobed oobe_tui installer_gxwi oobe_gxwi
pekit test
```

The build guide separates development builds from qualification in a
disposable guest. Passing tests do not install Peios or establish that an
image boots.

## Documentation

User and integrator guidance lives in [Learn](docs/learn/), with command
references in [docs/man](docs/man/). Historical material is kept outside
published Learn in [docs/archive](docs/archive/).

## License

Source code is licensed under the MIT License. Embedded fonts retain their
licenses in `installer-gxwi/page/fonts/`. Dependency notices accompany the
package output.
