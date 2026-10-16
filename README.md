# fast-sync

`fast-sync` copies files from the [WiFi File Transfer Android app](https://play.google.com/store/apps/details?id=com.techprd.filetransfer) to your computer over Wi-Fi.

## Sync files

`pull` downloads missing or changed files from one device directory. Files with matching size and modification time are skipped. For example, syncing `Pictures` updates `out/Pictures`: changed files are replaced, new files are added, and local-only files are left alone.

The default storage is the one with the largest reported capacity. Defaults are port `1234`, directory `DCIM/Camera`, and output directory `out`. Sync does not recurse into subdirectories or delete local files.

```sh
fast-sync --ip 192.168.1.111 pull
fast-sync --ip 192.168.1.111 pull --storage /storage/ABCD-1234 --directory Pictures --output phone-backup
fast-sync --ip 192.168.1.111 storage-info
```

`storage-info` shows storage and media sizes reported by the server.

## Build and run

With Nix:

```sh
nix build .#fast-sync
nix run .#fast-sync -- --ip 192.168.1.111 pull --directory Pictures
```

For local development, install [Nix](https://nixos.org/) and [direnv](https://direnv.net/), then run `direnv allow` and `cargo run -- --ip 192.168.1.111 pull`.

The Rust API is documented in [`src/fast_sync/src/wft/README.md`](src/fast_sync/src/wft/README.md).
