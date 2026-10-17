# WFT Rust client

`wft` implements the WiFi File Transfer Android app's HTTP protocol. It returns remote metadata
and file bytes; callers handle local files and synchronization.

```no_run
use std::net::SocketAddr;

use wft::Wft;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = "192.0.2.10:1234".parse()?;
    let wft = Wft::new(address).await?;

    println!("Selected storage: {}", wft.storages().selected_root().root_path);
    let directory = wft.directory("DCIM/Camera").await?;
    for file in directory.files {
        println!("{}: {} bytes", file.path, file.size);
    }
    Ok(())
}
```

`Wft::new` selects the storage root with the greatest reported capacity. Use
`Wft::select_storage` to choose another root. Directory paths are relative to the selected root;
`download_file` accepts a returned file's server path and returns bytes for the caller to save.

## TODO

See the [API documentation](https://docs.rs/wft/latest/wft/) for method details and errors,
or the [protocol reference](https://github.com/Gordon01/fast-sync/blob/main/src/wft/PROTOCOL.md)
for HTTP endpoints.
