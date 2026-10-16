use crate::root;
use fast_sync::wft::{DirectoryEntry, Entry as WftEntry, Error, Wft};
use std::{
    fmt,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::runtime::Runtime;
use tracing::info;

const DEVICE_ADDRESS: &str = "192.168.1.9:1234";

pub use crate::root::Entry;

#[derive(Debug)]
pub enum FileError {
    Plugin(String),
    Wft(Error),
}

impl fmt::Display for FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plugin(error) => formatter.write_str(error),
            Self::Wft(error) => error.fmt(formatter),
        }
    }
}

struct PluginState {
    runtime: Runtime,
    client: Wft,
}

static STATE: Mutex<Option<Arc<PluginState>>> = Mutex::new(None);

pub fn initialize() -> Result<(), String> {
    let mut state = STATE
        .lock()
        .map_err(|_| "plugin state lock is poisoned".to_owned())?;
    if state.is_none() {
        let address: SocketAddr = DEVICE_ADDRESS
            .parse()
            .map_err(|error| format!("invalid WFT device address {DEVICE_ADDRESS:?}: {error}"))?;
        info!(%address, "connecting to WFT device");
        let runtime = Runtime::new().map_err(|error| format!("create Tokio runtime: {error}"))?;
        let client = runtime
            .block_on(Wft::new(address))
            .map_err(|error| format!("connect to WFT device {address}: {error}"))?;
        info!(
            root = %client.storages().selected_root().root_path,
            "connected to WFT device"
        );
        *state = Some(Arc::new(PluginState { runtime, client }));
    }
    Ok(())
}

pub fn list_directory(path: &str) -> Result<Vec<Entry>, String> {
    let Some(path) = device_relative_path(path)? else {
        let device = device_name()?;
        info!(device, "listed WFT device at plugin root");
        return Ok(root::root_entries(&device_address()?));
    };
    let state = plugin_state()?;
    let _span = tracing::info_span!("list_directory", device_path = %path).entered();
    let directory = state
        .runtime
        .block_on(state.client.directory(&path))
        .map_err(|error| error.to_string())?;

    let entries: Vec<_> = directory
        .map(|entry| match entry {
            DirectoryEntry::Directory(entry) => to_entry(entry, true),
            DirectoryEntry::File(entry, _, _) => to_entry(entry, false),
        })
        .collect();
    info!(count = entries.len(), "listed WFT directory");
    Ok(entries)
}

pub fn get_file(path: &str) -> Result<Vec<u8>, FileError> {
    let path = device_relative_path(path).map_err(FileError::Plugin)?;
    let Some(path) = path else {
        return Err(FileError::Plugin(
            "a device file path is required".to_owned(),
        ));
    };
    if path.is_empty() {
        return Err(FileError::Plugin("a file path is required".to_owned()));
    }
    let state = plugin_state().map_err(FileError::Plugin)?;
    let root = &state.client.storages().selected_root().root_path;
    let remote_path = format!("{}/{}", root.trim_end_matches('/'), path);
    let _span = tracing::info_span!("download", device_path = %remote_path).entered();
    let contents = state
        .runtime
        .block_on(state.client.download_file(remote_path))
        .map(|download| download.bytes)
        .map_err(FileError::Wft)?;
    info!(size = contents.len(), "downloaded WFT file");
    Ok(contents)
}

fn plugin_state() -> Result<Arc<PluginState>, String> {
    STATE
        .lock()
        .map_err(|_| "plugin state lock is poisoned".to_owned())?
        .clone()
        .ok_or_else(|| "plugin is not initialized".to_owned())
}

fn device_address() -> Result<SocketAddr, String> {
    DEVICE_ADDRESS
        .parse()
        .map_err(|error| format!("invalid WFT device address {DEVICE_ADDRESS:?}: {error}"))
}

fn device_name() -> Result<String, String> {
    Ok(root::device_name(&device_address()?))
}

fn device_relative_path(path: &str) -> Result<Option<String>, String> {
    root::device_relative_path(path, &device_address()?)
}

fn to_entry(entry: WftEntry, is_directory: bool) -> Entry {
    Entry {
        name: entry.name,
        is_directory,
        size: entry.size,
    }
}
