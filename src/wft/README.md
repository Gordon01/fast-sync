# Using the WFT Rust API

`Wft` connects to the WiFi File Transfer Android app over HTTP. The crate exposes it as
`fast_sync::wft::Wft`. Create one client for the device address and use its async methods to query
device information, inspect storage, and list or download files.

```rust
use std::net::SocketAddr;

use fast_sync::wft::Wft;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = "192.0.2.10:1234".parse()?;
    let mut wft = Wft::new(address).await?;

    let platform = wft.get_platform_info().await?;
    println!("Platform: {}", platform.platform);

    let permission = wft.check_manage_storage_permission().await?;
    println!(
        "Can manage external storage: {}",
        permission.is_external_storage_manager
    );

    let version = wft.get_android_version().await?;
    println!("Android {} (SDK {})", version.release, version.sdk_int);

    println!(
        "Selected {} ({} bytes total)",
        wft.storages().root_path(),
        wft.storages().total_bytes()
    );

    wft.set_output_path("out");
    let entries = wft.directory("DCIM").await?;
    println!(
        "{} bytes total, {} bytes already up to date",
        entries.total_bytes(),
        entries.up_to_date_bytes()
    );
    for entry in entries {
        match entry {
            fast_sync::wft::DirectoryEntry::Directory(directory) => {
                println!("Directory: {}", directory.path);
            }
            fast_sync::wft::DirectoryEntry::File(file, local_path, status) => {
                println!("File {}: {status:?}", file.path);
                println!("Local path: {}", local_path.display());
            }
        }
    }

    Ok(())
}
```

The program selects the storage root with the greatest `total_bytes`. To list a directory, pass its
path relative to the selected root; for example, `"Pictures/Whatsapp"`. Configure the local output
root with `Wft::set_output_path` (default `"out"`); `Wft::directory` returns an iterator directly.
It yields
`DirectoryEntry::Directory` values first, followed by `DirectoryEntry::File` values; each file
includes its local path and `FileStatus`. `Wft::directory` creates the corresponding local output
directory. The iterator's `total_bytes()` and `up_to_date_bytes()` report the total listed file size
and the size already up to date when the iterator was created. The WFT API route appends the
selected root and relative path after `/api/directory/`, without inserting a `root` component.

`download_file` returns a `DownloadedFile` containing the downloaded `bytes` and request `elapsed`
time.

`Wft::new` takes a `SocketAddr`, pings `/api/ping`, and fetches storage details for
`wft.storages()`. Its default selected root is the one with the greatest total capacity; call
`wft.storages().select(root_path)` to select another reported root. `roots()` returns the unique
storage roots, and `directory_sizes()` returns the API-reported sizes keyed by entry path.
Construction returns an error if the server is unreachable, does not reply with `pong`, or reports
no storage entries; specify the server port explicitly if it is not `1234`. Methods return
`wft::Result<T>` and report network errors, HTTP failures, API error payloads, invalid paths, and
malformed JSON through `wft::Error`.
