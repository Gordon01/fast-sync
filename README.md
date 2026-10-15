# fast-sync

`fast-sync` downloads files from the HTTP server provided by the [WiFi File Transfer Android app](https://play.google.com/store/apps/details?id=com.techprd.filetransfer). It is useful for copying files from a phone to a computer over the local network without selecting and downloading each file individually.

## How synchronization works

The program selects the storage root with the largest reported total capacity by default; use
`--storage` to choose another storage root.
The `--directory` argument identifies a directory within that root (default: `DCIM/Camera`). For
the requested remote directory, the program:

1. Requests the directory listing from the device.
2. Creates the corresponding directory under the local output root.
3. Checks each listed file against the local file with the same name:
   - Missing locally: download it.
   - Same size: leave it unchanged.
   - Different size: download it and overwrite the local file.
4. Reports transfer progress and timing in the log.

The comparison uses file size only; modification times and file contents are not checked. It processes files directly in the requested directory, does not recurse into child directories, and does not remove local files that are absent from the remote listing.

## Example

Suppose the selected storage has a `Pictures` directory containing:

```text
photo-a.jpg  4096 bytes
photo-b.jpg  2048 bytes
photo-c.jpg  1024 bytes
```

Before running, the local output could look like this:

```text
out/
└── Pictures/
    ├── photo-a.jpg    # 4096 bytes: same size as remote
    ├── photo-b.jpg    # 1500 bytes: different size
    └── notes.txt      # local-only file
```

Run the `pull` subcommand for the `Pictures` directory within the selected storage:

```sh
fast-sync --ip 192.168.1.111 pull --directory Pictures
```

After a successful run:

```text
out/
└── Pictures/
    ├── photo-a.jpg    # left unchanged
    ├── photo-b.jpg    # downloaded and replaced (2048 bytes)
    ├── photo-c.jpg    # downloaded (1024 bytes)
    └── notes.txt      # left untouched
```

## Usage

The server port defaults to `1234`, the directory within the selected storage to `DCIM/Camera`,
the storage root with the largest reported total capacity, and the local output root to `out`.

```sh
fast-sync --ip 192.168.1.111 pull
fast-sync --ip 192.168.1.111 pull --storage /storage/ABCD-1234 --directory Pictures --output phone-backup
fast-sync --ip 192.168.1.111 storage-info
```

Use `pull` to download from a directory. `storage-info` displays the media sizes and total/available
space reported by the server. These are server-provided summaries; the command does not recursively
scan directories.

Build or run with Nix:

```sh
nix build .#fast-sync
nix run .#fast-sync -- --ip 192.168.1.111 pull --directory Pictures
```

For local development, install [Nix](https://nixos.org/) and [direnv](https://direnv.net/), then allow the project environment:

```sh
direnv allow
cargo run -- --ip 192.168.1.111 pull --directory Pictures
```

The Nix development shell provides Cargo, rustfmt, Clippy, and Rust standard-library sources for rust-analyzer.

The reusable Rust API is documented in [`src/wft/README.md`](src/wft/README.md).
