# fast-sync

`fast-sync` copies files from the [WiFi File Transfer Android app](https://play.google.com/store/apps/details?id=com.techprd.filetransfer) to your computer over Wi-Fi.

## Sync files

Start the app's HTTP server and use its device IP address:

```sh
fast-sync --ip 192.168.1.111 pull
fast-sync --ip 192.168.1.111 pull --storage /storage/ABCD-1234 --directory Pictures --output phone-backup
fast-sync --ip 192.168.1.111 storage-info
```

`pull` downloads missing or changed files from one directory. Files with matching size
and modification time are skipped; changed files are replaced and local-only files
are left alone. Sync does not recurse into subdirectories or delete local files.
For example, syncing `Pictures` with the default output updates `out/Pictures`.

Defaults are port `1234`, directory `DCIM/Camera`, output directory `out`, and the
storage root with the largest reported capacity. `storage-info` shows server-reported
storage and media sizes.

## Build and run

Run these commands from the repository root:

```sh
nix build .#fast-sync
nix run .#fast-sync -- --ip 192.168.1.111 pull --directory Pictures
```

For local development, follow the [workspace setup](../../README.md#development), then run:

```sh
cargo run -p fast-sync -- --ip 192.168.1.111 pull
```
