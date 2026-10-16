# TotalDroid WFX demo

This is a minimal, read-only Double Commander WFX plugin backed by the
[WiFi File Transfer (WFT) API](../fast-sync/src/wft/README.md). It connects to
devices by IP address, lists their selected storage roots, and downloads files.

## Demo configuration

The device address is currently hardcoded in `src/filesystem.rs`:

```rust
const DEVICE_ADDRESS: &str = "192.168.1.9:1234";
```

Change this value and rebuild to use another device. Before connecting, make
sure the WFT app is running on the Android device, its HTTP server is enabled,
and the computer can reach the configured address and port.

Every command acquires the shared plugin, lazily initializing logging and the
runtime without contacting any device. Explicit host initialization uses the
same path; subsequent commands reuse the plugin.
Plugin initialization failures are cached until the plugin is reloaded.
The plugin connects on the first device operation and caches successful
connections by IP address and port. Each device uses its own storage root with
the greatest reported capacity. Failed device connections are reported and can be
retried.

The plugin root is displayed as `TotalDroid` in Double Commander and contains
one device entry named by the configured address. Port 1234 is omitted from the
entry name, such as `192.168.1.9`; other ports are included. Open that entry to
browse the device storage, or open a path to another device directly. The
configured device does not need to be online to access another device.
Copying files from devices is supported; uploads, deletion, device discovery,
and a configuration UI are not implemented.

## Remote paths

Remote names are parsed immediately after decoding at the WFX boundary.
This plugin uses the forward slashes supplied by Double Commander on Unix,
rather than the backslashes described in the
[WFX SDK](https://ghisler.github.io/WFX-SDK/fsfindfirst.htm).
`TotalDroid` is not part of the remote name.

Examples (remote names, not local filesystem paths):

```text
/                              Plugin root
/192.168.1.9                   Device root, using port 1234
/192.168.1.9/DCIM/Camera/       Directory (trailing slash preserved)
/192.168.1.9:8080/DCIM/a.jpg    File on a nondefault port
/::1/DCIM                      IPv6 device using port 1234
/[::1]:8080/DCIM/a.jpg          IPv6 device with an explicit port
```

Only numeric IP addresses are accepted, not hostnames. Downloads require a
nonempty file path beneath a device, with no trailing slash. Parsing requires
a leading slash and a valid device address; the remaining path is not
normalized or checked component by component. Case and Unicode names are preserved.

Internally, listing accepts `RemotePath::Root` or `RemotePath::Device(DevicePath)`.
Other commands accept only `DevicePath`, which contains an address and a
storage-relative path. File-specific validation happens at the WFX boundary.
The relative path is passed directly to WFT without separator conversion.
It never includes the device address or the plugin root name.

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
