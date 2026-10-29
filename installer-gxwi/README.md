# Installer Browser Interfaces

`installer-gxwi` and `oobe-gxwi` draw the installation and first-boot
conversations owned by `installerd` and `oobed`. Their Rust service holds
shared conversation state, and `page/` contains embedded browser assets.
All connected browsers observe the same conversation.

Both programs belong to the installer workspace and share its version.
Use the [repository build guide](../docs/building.md) for pekit build,
test, and development-harness instructions. The maintained command
references are in [docs/man](../docs/man/), and user guidance is in
[Learn](../docs/learn/).

The graphical installation flow supports a fresh installation. Upgrade and
repair use the terminal interface. Browser setup supports account creation,
machine naming, and a manual wired-network address.
