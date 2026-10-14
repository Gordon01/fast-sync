A client for [WiFi File Transfer](https://play.google.com/store/apps/details?id=com.techprd.filetransfer). It downloads files from the app's HTTP server into a local directory (default: "out/").

```powershell
$env:RUST_LOG="fast_sync=trace"; cargo r -q -- --ip 192.168.1.111 -d /Pictures
```

Build with Nix:

```sh
nix build .#fast-sync
```

Run directly with Nix:

```sh
nix run .#fast-sync -- --ip 192.168.1.111 -d /Pictures
```

For a development environment, install and enable [direnv](https://direnv.net/),
then run `direnv allow` in the project directory. The flake provides Rust
tooling and Rust standard-library sources for rust-analyzer.