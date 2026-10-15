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
    let wft = Wft::new(address).await?;

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

    let directory_path = format!("{}/DCIM", wft.storages().root_path().trim_end_matches('/'));
    let directory = wft.directory(&directory_path).await?;
    for file in directory.files {
        let download = wft.download_file(&file.path).await?;
        println!(
            "Downloaded {} bytes from {} in {:?}",
            download.bytes.len(),
            file.path,
            download.elapsed
        );
    }

    Ok(())
}
```

The program selects the storage root with the greatest `total_bytes`. For a directory listing,
pass an absolute Android storage path. The WFT API route is formed by appending that path after
`/api/directory/`, without inserting a `root` component.

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
