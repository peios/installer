# Peios installer and OOBE

This workspace contains the privileged services and presentation surfaces used
to install Peios and complete first-boot setup. They communicate over the
Message Surface Interaction Protocol (MSIP).

The deployable components are deliberately split by privilege and lifecycle:

- `installerd` owns installation, repair, and upgrade operations;
- `install-tui` is its terminal presentation surface;
- `msip-drive` drives an MSIP conversation non-interactively;
- `oobed` performs first-boot account and machine setup; and
- `oobe-tui` is the first-boot terminal surface.

`msip-serve` and `msip-tui` are internal workspace libraries. The protocol
types live in the separate `msip` repository and are pinned exactly by this
workspace.

## Development

The workspace requires Rust 1.98.1 or newer. Cargo resolves MSIP from its exact
public Git revision; no sibling checkout participates in a release build. Run:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The source-owned Pekit recipe builds the production packages offline after its
vendor stage:

```sh
pekit package --all --version 0.1.3
pekit lint --version 0.1.3
```

Run any shipped program with `--help` for its command-line interface. Manual
pages are maintained in `man/` and installed by the corresponding packages.

## License

Peios installer and OOBE sources are licensed under the MIT License. Packaged
binaries also carry the complete notices for their vendored dependencies.
