# WFT Rust client

`fast-sync` provides an async client for the WiFi File Transfer Android app. This example connects
to a device, shows the selected storage root, and lists a directory while comparing its files with
the local output directory:

```rust
use std::net::SocketAddr;

use fast_sync::wft::Wft;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = "192.0.2.10:1234".parse()?;
    let mut wft = Wft::new(address).await?;

    println!("Selected storage: {}", wft.storages().selected_root().root_path);
    wft.set_output_path("backup")?;
    for entry in wft.directory("DCIM/Camera").await? {
        println!("{entry:?}");
    }
    Ok(())
}
```

`Wft::new` selects the storage root with the greatest reported capacity. Use
`Wft::select_storage` to choose another root. Directory paths are relative to the selected root;
listing reports local paths and statuses but does not download files. `download_file` returns bytes
for the caller to save. See the [generated API documentation](https://docs.rs/fast-sync/latest/fast_sync/wft/index.html)
for method details, storage and directory types, and errors.
