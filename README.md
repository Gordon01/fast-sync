# fast-sync

A Rust workspace for accessing files from the [WiFi File Transfer Android app](https://play.google.com/store/apps/details?id=com.techprd.filetransfer) over Wi-Fi.

## Crates

- [`wft`](src/wft/README.md): WiFi File Transfer protocol client.
- [`fast-sync`](src/fast-sync/README.md): CLI for file sync.
- [`fs_plugin`](src/fs-plugin/README.md): Total/Double Commander WFX plugin.

## Development

Install [Nix](https://nixos.org/) and [direnv](https://direnv.net/), then run
`direnv allow` from the repository root to load the development toolchain.
Alternatively, enter the shell with `nix develop`.

See each crate's README for usage and build instructions.
