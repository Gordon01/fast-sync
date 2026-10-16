//! Safe plugin core. [`crate::wfx`] decodes host calls into [`Command`]s and runs them here.
#![forbid(unsafe_code)]

use crate::root;
use fast_sync::wft::{DirectoryEntry, Entry as WftEntry, Error as WftError, Wft};
use std::{
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
        path: String,
    },
    Get {
        remote: String,
        local: PathBuf,
        overwrite: bool,
    },
    Put {
        local: PathBuf,
        remote: String,
        overwrite: bool,
    },
    Delete {
        path: String,
    },
    MkDir {
        path: String,
    },
    RemoveDir {
        path: String,
    },
    Rename {
        from: String,
        to: String,
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

struct PluginState {
    runtime: Runtime,
    client: Wft,
}

static STATE: Mutex<Option<Arc<PluginState>>> = Mutex::new(None);

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
        let address = device_address()?;
        info!(%address, "connecting to WFT device");
        let runtime =
            Runtime::new().map_err(|error| plugin(format!("create Tokio runtime: {error}")))?;
        let client = runtime
            .block_on(Wft::new(address))
            .map_err(|error| plugin(format!("connect to WFT device {address}: {error}")))?;
        info!(root = %client.storages().selected_root().root_path, "connected to WFT device");
        *state = Some(Arc::new(PluginState { runtime, client }));
    }
    Ok(())
}

fn list(path: &str) -> Result<Vec<Entry>, Error> {
    let _span = tracing::info_span!("find_first", path = %path).entered();
    let address = device_address()?;
    let Some(path) = root::device_relative_path(path, &address).map_err(Error::Plugin)? else {
        info!(
            device = root::device_name(&address),
            "listed WFT device at plugin root"
        );
        return Ok(root::root_entries(&address));
    };
    let state = plugin_state()?;
    let _span = tracing::info_span!("list_directory", device_path = %path).entered();
    let entries: Vec<_> = state
        .runtime
        .block_on(state.client.directory(&path))?
        .map(|entry| match entry {
            DirectoryEntry::Directory(entry) => to_entry(entry, true),
            DirectoryEntry::File(entry, _, _) => to_entry(entry, false),
        })
        .collect();
    info!(count = entries.len(), "listed WFT directory");
    Ok(entries)
}

fn get(remote: &str, local: &Path, overwrite: bool) -> Result<(), Error> {
    let _span = tracing::info_span!(
        "get_file",
        remote_path = %remote,
        local_path = %local.display(),
        overwrite
    )
    .entered();
    let contents = download(remote)?;
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

fn download(path: &str) -> Result<Vec<u8>, Error> {
    let path = root::device_relative_path(path, &device_address()?)
        .map_err(Error::Plugin)?
        .filter(|path| !path.is_empty())
        .ok_or_else(|| plugin("a device file path is required"))?;
    let state = plugin_state()?;
    let root = &state.client.storages().selected_root().root_path;
    let remote_path = format!("{}/{}", root.trim_end_matches('/'), path);
    let _span = tracing::info_span!("download", device_path = %remote_path).entered();
    let contents = state
        .runtime
        .block_on(state.client.download_file(remote_path))?
        .bytes;
    info!(size = contents.len(), "downloaded WFT file");
    Ok(contents)
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

#[cfg(test)]
mod tests {
    use super::{Command, Error, execute};

    #[test]
    fn logging_initialization_is_cached() {
        const CHILD: &str = "FS_PLUGIN_LOGGING_TEST";
        if let Ok(case) = std::env::var(CHILD) {
            let first = super::initialize_logging();
            let second = super::initialize_logging();
            assert_eq!(first, second);
            assert_eq!(first.is_ok(), case == "success");
            return;
        }

        // Isolate the global tracing subscriber and HOME from the test runner.
        let directory =
            std::env::temp_dir().join(format!("fs-plugin-logging-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let invalid_home = directory.join("not-a-directory");
        std::fs::write(&invalid_home, b"").unwrap();
        for (case, home) in [("success", &directory), ("failure", &invalid_home)] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "filesystem::tests::logging_initialization_is_cached",
                    "--nocapture",
                ])
                .env(CHILD, case)
                .env("HOME", home)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unsupported_commands_are_rejected() {
        let commands = [
            Command::Put {
                local: "a".into(),
                remote: "b".into(),
                overwrite: false,
            },
            Command::Delete { path: "a".into() },
            Command::MkDir { path: "a".into() },
            Command::RemoveDir { path: "a".into() },
            Command::Rename {
                from: "a".into(),
                to: "b".into(),
                overwrite: false,
            },
        ];
        for command in commands {
            assert!(matches!(execute(command), Err(Error::NotSupported)));
        }
    }

    #[test]
    fn get_rejects_invalid_remote_paths() {
        for remote in ["\\", "\\other-device\\file", "\\192.168.1.9\\..\\x"] {
            let command = Command::Get {
                remote: remote.into(),
                local: "unused".into(),
                overwrite: false,
            };
            assert!(
                matches!(execute(command), Err(Error::Plugin(_))),
                "{remote}"
            );
        }
    }
}
