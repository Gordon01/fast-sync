# TotalDroid WFX demo

This is a minimal, read-only Double Commander WFX plugin backed by the
[WiFi File Transfer (WFT) API](../fast-sync/src/wft/README.md). It connects to
one configured device, lists the selected storage root, and downloads files.

## Demo configuration

The device address is currently hardcoded in `src/filesystem.rs`:

```rust
const DEVICE_ADDRESS: &str = "192.168.1.9:1234";
```

Change this value and rebuild to use another device. Before connecting, make
sure the WFT app is running on the Android device, its HTTP server is enabled,
and the computer can reach the configured address and port.

When the plugin initializes, it connects to the configured device and uses the
storage root with the greatest reported capacity. The plugin root is displayed
as `TotalDroid` in Double Commander and contains one device entry named by the
configured device IP, such as `192.168.1.9`. Open that entry to browse the
device storage; relative paths beneath it are translated to paths within the
selected storage root. Copying files from the device is supported; uploads,
deletion, device discovery, and a configuration UI are not implemented.

## Build and install

Build the plugin from the repository root:

```sh
cargo build --release -p fs_plugin
```

On macOS the library is `target/release/libfs_plugin.dylib`. Add it in Double
Commander's filesystem-plugin settings, then open the `TotalDroid` root. A
connection or request failure is written to `~/Library/Application Support/fast-sync/plugin.log`.

The Nix package can also be built from the repository root:

```sh
nix build .#fs-plugin
```

## References

- [Double Commander plugin development](https://github.com/doublecmd/doublecmd/wiki/Plugins-development)
- [Total Commander WFX SDK](https://ghisler.github.io/WFX-SDK/contents.htm)
