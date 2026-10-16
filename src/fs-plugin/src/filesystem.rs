//! Safe plugin core. [`crate::wfx`] decodes host calls into [`Command`]s and runs them here.
#![forbid(unsafe_code)]

use crate::{
    remote_path::{DevicePath, RemotePath},
    root,
};
use fast_sync::wft::{DirectoryEntry, Entry as WftEntry, Error as WftError, Wft};
use std::{
    collections::HashMap,
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tokio::runtime::Runtime;
use tracing::info;

pub use crate::root::Entry;

pub const ROOT_NAME: &str = "TotalDroid";
const DEVICE_ADDRESS: &str = "192.168.1.9:1234";

/// An operation requested by the host.
#[derive(Debug)]
#[expect(dead_code, reason = "Put..Rename are not exported until implemented")]
pub enum Command {
    Init,
    List {
        path: RemotePath,
    },
    Get {
        remote: DevicePath,
        local: PathBuf,
        overwrite: bool,
    },
    Put {
        local: PathBuf,
        remote: DevicePath,
        overwrite: bool,
    },
    Delete {
        path: DevicePath,
    },
    MkDir {
        path: DevicePath,
    },
    RemoveDir {
        path: DevicePath,
    },
    Rename {
        from: DevicePath,
        to: DevicePath,
        overwrite: bool,
    },
}

#[derive(Debug)]
pub enum Reply {
    Done,
    Entries(Vec<Entry>),
}

#[derive(Debug)]
pub enum Error {
    /// The remote file or directory does not exist.
    NotFound,
    /// The local target exists and overwriting was not requested.
    Exists,
    Read(WftError),
    Write(io::Error),
    Plugin(String),
    NotSupported,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Exists => f.write_str("already exists"),
            Self::Read(error) => error.fmt(f),
            Self::Write(error) => error.fmt(f),
            Self::Plugin(error) => f.write_str(error),
            Self::NotSupported => f.write_str("operation is not supported"),
        }
    }
}

impl From<WftError> for Error {
    fn from(error: WftError) -> Self {
        match error {
            WftError::HttpStatus { status: 404, .. } => Self::NotFound,
            error => Self::Read(error),
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::AlreadyExists => Self::Exists,
            _ => Self::Write(error),
        }
    }
}

pub fn execute(command: Command) -> Result<Reply, Error> {
    let result = match command {
        Command::Init => init().map(|()| Reply::Done),
        Command::List { path } => list(&path).map(Reply::Entries),
        Command::Get {
            remote,
            local,
            overwrite,
        } => get(&remote, &local, overwrite).map(|()| Reply::Done),
        Command::Put { .. }
        | Command::Delete { .. }
        | Command::MkDir { .. }
        | Command::RemoveDir { .. }
        | Command::Rename { .. } => Err(Error::NotSupported),
    };
    if let Err(error) = &result {
        tracing::error!(%error, "command failed");
    }
    result
}

/// Shared runtime and lazily connected clients, keyed by device address.
struct PluginState {
    /// Executes WFT requests from synchronous host callbacks.
    runtime: Runtime,
    /// Successful connections reused by subsequent device operations.
    clients: Mutex<HashMap<SocketAddr, Arc<Wft>>>,
}

static STATE: Mutex<Option<Arc<PluginState>>> = Mutex::new(None);

impl PluginState {
    /// Creates the runtime without contacting any device.
    fn new() -> Result<Self, Error> {
        let runtime =
            Runtime::new().map_err(|error| plugin(format!("create Tokio runtime: {error}")))?;
        Ok(Self {
            runtime,
            clients: Mutex::new(HashMap::new()),
        })
    }

    /// Returns a cached client or connects once while holding the cache lock.
    fn client(&self, address: SocketAddr) -> Result<Arc<Wft>, Error> {
        let mut clients = self
            .clients
            .lock()
            .map_err(|_| plugin("device client cache lock is poisoned"))?;
        if let Some(client) = clients.get(&address) {
            return Ok(Arc::clone(client));
        }
        info!(%address, "connecting to WFT device");
        let client = self
            .runtime
            .block_on(Wft::new(address))
            .map_err(|error| plugin(format!("connect to WFT device {address}: {error}")))?;
        info!(%address, root = %client.storages().selected_root().root_path, "connected to WFT device");
        let client = Arc::new(client);
        clients.insert(address, Arc::clone(&client));
        Ok(client)
    }

    /// Lists a device-relative directory without rewriting the host path.
    fn list_device(&self, address: SocketAddr, path: &str) -> Result<Vec<Entry>, Error> {
        let client = self.client(address)?;
        let entries: Vec<_> = self
            .runtime
            .block_on(client.directory(path))?
            .map(|entry| match entry {
                DirectoryEntry::Directory(entry) => to_entry(entry, true),
                DirectoryEntry::File(entry, _, _) => to_entry(entry, false),
            })
            .collect();
        info!(%address, count = entries.len(), "listed WFT directory");
        Ok(entries)
    }

    /// Downloads a WFX-relative file beneath the device's selected storage root.
    fn download(&self, address: SocketAddr, path: &str) -> Result<Vec<u8>, Error> {
        let client = self.client(address)?;
        let root = &client.storages().selected_root().root_path;
        let remote_path = format!("{}/{}", root.trim_end_matches('/'), path);
        let _span = tracing::info_span!("download", %address, device_path = %remote_path).entered();
        let contents = self
            .runtime
            .block_on(client.download_file(remote_path))?
            .bytes;
        info!(size = contents.len(), "downloaded WFT file");
        Ok(contents)
    }
}

fn init() -> Result<(), Error> {
    if let Err(error) = initialize_logging() {
        eprintln!("android-wfx: {error}");
        return Err(Error::Plugin(error.to_string()));
    }
    let _span = tracing::info_span!("init").entered();
    let mut state = STATE
        .lock()
        .map_err(|_| plugin("plugin state lock is poisoned"))?;
    if state.is_none() {
        *state = Some(Arc::new(PluginState::new()?));
    }
    Ok(())
}

fn list(path: &RemotePath) -> Result<Vec<Entry>, Error> {
    let _span = tracing::info_span!("find_first", path = %path).entered();
    match path {
        RemotePath::Root => {
            let address = device_address()?;
            info!(%address, "listed WFT device at plugin root");
            Ok(root::root_entries(&address))
        }
        RemotePath::Device(device) => plugin_state()?.list_device(device.address, &device.path),
    }
}

fn get(remote: &DevicePath, local: &Path, overwrite: bool) -> Result<(), Error> {
    let _span = tracing::info_span!(
        "get_file",
        address = %remote.address,
        remote_path = %remote.path,
        local_path = %local.display(),
        overwrite
    )
    .entered();
    let contents = plugin_state()?.download(remote.address, &remote.path)?;
    if overwrite {
        fs::write(local, contents)?;
    } else {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(local)?
            .write_all(&contents)?;
    }
    Ok(())
}

fn initialize_logging() -> Result<(), String> {
    static LOGGING: OnceLock<Result<(), String>> = OnceLock::new();
    LOGGING
        .get_or_init(|| configure_logging().map_err(|error| error.to_string()))
        .clone()
}

fn configure_logging() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    let directory = home.join(if cfg!(target_os = "macos") {
        "Library/Logs/fast-sync"
    } else {
        ".local/state/fast-sync"
    });
    fs::create_dir_all(&directory)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("plugin.log"))?;
    tracing_subscriber::fmt()
        .compact()
        .with_ansi(false)
        .with_writer(file)
        .try_init()
}

fn plugin(message: impl Into<String>) -> Error {
    Error::Plugin(message.into())
}

fn plugin_state() -> Result<Arc<PluginState>, Error> {
    STATE
        .lock()
        .map_err(|_| plugin("plugin state lock is poisoned"))?
        .clone()
        .ok_or_else(|| plugin("plugin is not initialized"))
}

fn device_address() -> Result<SocketAddr, Error> {
    DEVICE_ADDRESS.parse().map_err(|error| {
        plugin(format!(
            "invalid WFT device address {DEVICE_ADDRESS:?}: {error}"
        ))
    })
}

fn to_entry(entry: WftEntry, is_directory: bool) -> Entry {
    Entry {
        name: entry.name,
        is_directory,
        size: entry.size,
    }
}
