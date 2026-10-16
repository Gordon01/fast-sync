//! Safe plugin core. [`crate::wfx`] decodes host calls into [`Command`]s and runs them here.
#![forbid(unsafe_code)]

use crate::{
    remote_path::{DevicePath, RemotePath},
    root,
};
use fast_sync::wft::{DirectoryEntry, Error as WftError, Wft};
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

/// Display name of the plugin root in the host.
pub const ROOT_NAME: &str = "TotalDroid";
/// Demo device advertised by the plugin root.
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

/// The result of a completed host command.
#[derive(Debug)]
pub enum Reply {
    /// The operation completed without returning data.
    Done,
    /// Entries returned by a directory listing.
    Entries(Vec<Entry>),
}

/// A filesystem operation or plugin initialization failure.
#[derive(Debug)]
pub enum Error {
    /// The remote file or directory does not exist.
    NotFound,
    /// The local target exists and overwriting was not requested.
    Exists,
    /// A remote WFT request failed.
    Read(WftError),
    /// Writing the local destination failed.
    Write(io::Error),
    /// Plugin setup, synchronization, or device connection failed.
    Plugin(String),
    /// The requested operation is not implemented.
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

/// Acquires the shared plugin, dispatches a host command, and logs failures.
pub fn execute(command: Command) -> Result<Reply, Error> {
    let result = Plugin::shared().and_then(|plugin| match command {
        Command::Init => Ok(Reply::Done),
        Command::List { path } => plugin.list(&path).map(Reply::Entries),
        Command::Get {
            remote,
            local,
            overwrite,
        } => plugin.get(&remote, &local, overwrite).map(|()| Reply::Done),
        Command::Put { .. }
        | Command::Delete { .. }
        | Command::MkDir { .. }
        | Command::RemoveDir { .. }
        | Command::Rename { .. } => Err(Error::NotSupported),
    });
    if let Err(error) = &result {
        tracing::error!(%error, "command failed");
    }
    result
}

/// Shared runtime and lazily connected clients, keyed by device address.
struct Plugin {
    /// Executes WFT requests from synchronous host callbacks.
    runtime: Runtime,
    /// Successful connections reused by subsequent device operations.
    clients: Mutex<HashMap<SocketAddr, Arc<Wft>>>,
}

impl Plugin {
    /// Lists the advertised device or a directory on a selected device.
    fn list(&self, path: &RemotePath) -> Result<Vec<Entry>, Error> {
        let _span = tracing::info_span!("find_first", path = %path).entered();
        match path {
            RemotePath::Root => {
                let address = DEVICE_ADDRESS.parse().map_err(|error| {
                    Error::Plugin(format!(
                        "invalid WFT device address {DEVICE_ADDRESS:?}: {error}"
                    ))
                })?;
                info!(%address, "listed WFT device at plugin root");
                Ok(root::root_entries(&address))
            }

            RemotePath::Device(device) => self.list_device(device.address, &device.path),
        }
    }

    /// Downloads a device file to a local path, replacing it only when requested.
    fn get(&self, remote: &DevicePath, local: &Path, overwrite: bool) -> Result<(), Error> {
        let _span = tracing::info_span!(
            "get_file",
            address = %remote.address,
            remote_path = %remote.path,
            local_path = %local.display(),
            overwrite
        )
        .entered();
        let client = self.client(remote.address)?;
        let root = &client.storages().selected_root().root_path;
        let remote_path = format!("{}/{}", root.trim_end_matches('/'), remote.path);
        let contents = {
            let _span = tracing::info_span!(
                "download",
                address = %remote.address,
                device_path = %remote_path
            )
            .entered();
            let contents = self
                .runtime
                .block_on(client.download_file(remote_path))?
                .bytes;
            info!(size = contents.len(), "downloaded WFT file");
            contents
        };
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

    /// Initializes the shared plugin once, caching either the instance or its failure.
    fn shared() -> Result<&'static Self, Error> {
        static PLUGIN: OnceLock<Result<Plugin, String>> = OnceLock::new();
        PLUGIN
            .get_or_init(|| Self::new().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(|error| Error::Plugin(error.clone()))
    }

    /// Initializes logging and creates the runtime without contacting any device.
    fn new() -> Result<Self, Error> {
        if let Err(error) = Self::initialize_logging() {
            eprintln!("android-wfx: {error}");
            return Err(Error::Plugin(error));
        }
        let _span = tracing::info_span!("init").entered();
        let runtime = Runtime::new()
            .map_err(|error| Error::Plugin(format!("create Tokio runtime: {error}")))?;
        Ok(Self {
            runtime,
            clients: Mutex::new(HashMap::new()),
        })
    }

    /// Configures file logging once, caching success or the initialization error.
    fn initialize_logging() -> Result<(), String> {
        static LOGGING: OnceLock<Result<(), String>> = OnceLock::new();
        LOGGING
            .get_or_init(|| {
                let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
                let directory = home.join(if cfg!(target_os = "macos") {
                    "Library/Logs/fast-sync"
                } else {
                    ".local/state/fast-sync"
                });
                fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(directory.join("plugin.log"))
                    .map_err(|error| error.to_string())?;
                tracing_subscriber::fmt()
                    .compact()
                    .with_ansi(false)
                    .with_writer(file)
                    .try_init()
                    .map_err(|error| error.to_string())
            })
            .clone()
    }

    /// Returns a cached client or connects once while holding the cache lock.
    fn client(&self, address: SocketAddr) -> Result<Arc<Wft>, Error> {
        let mut clients = self
            .clients
            .lock()
            .map_err(|_| Error::Plugin("device client cache lock is poisoned".into()))?;
        if let Some(client) = clients.get(&address) {
            return Ok(Arc::clone(client));
        }
        info!(%address, "connecting to WFT device");
        let client = self
            .runtime
            .block_on(Wft::new(address))
            .map_err(|error| Error::Plugin(format!("connect to WFT device {address}: {error}")))?;
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
            .map(|entry| {
                let (entry, is_directory) = match entry {
                    DirectoryEntry::Directory(entry) => (entry, true),
                    DirectoryEntry::File(entry, _, _) => (entry, false),
                };
                Entry {
                    name: entry.name,
                    is_directory,
                    size: entry.size,
                }
            })
            .collect();
        info!(%address, count = entries.len(), "listed WFT directory");
        Ok(entries)
    }
}
