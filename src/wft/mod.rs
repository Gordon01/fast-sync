use std::{
    net::SocketAddr,
    path::Path,
    time::{Duration, Instant, UNIX_EPOCH},
};

mod storage;
pub use storage::{ContentGroup, StorageRoot, Storages};

use readable::byte::Byte;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, de::DeserializeOwned};
use tracing::{debug, info, trace};

/// Client for the WiFi File Transfer Android app HTTP API.
///
/// Construct a client with [`Wft::new`], then use it to query device information
/// and transfer files.
#[derive(Clone, Debug)]
pub struct Wft {
    address: SocketAddr,
    client: reqwest::Client,
    storages: Storages,
}

impl Wft {
    /// Pings the WFT server, fetches storage details, and creates a client for it.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot be reached, responds with a
    /// non-success status, does not return `pong`, or returns no storage details.
    pub async fn new(address: SocketAddr) -> Result<Self> {
        debug!(%address, "Pinging WFT server");
        let client = reqwest::Client::builder().build()?;
        let url = server_url(address, &["api", "ping"])?;
        let response = client.get(url).send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(http_status_error(status, &body));
        }
        let body = String::from_utf8_lossy(&body);
        if body.trim() != "pong" {
            return Err(Error::UnexpectedPingResponse(body.into_owned()));
        }

        let storages = Self::get_json(
            &client,
            server_url(address, &["api", "get-storage-details"])?,
        )
        .await?;

        Ok(Self {
            address,
            client,
            storages: Storages::new(storages)?,
        })
    }

    /// Returns storage details fetched when this client was constructed.
    pub fn storages(&mut self) -> &mut Storages {
        &mut self.storages
    }

    /// Fetches the platform identifier reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response is an API,
    /// HTTP, or JSON error.
    pub async fn get_platform_info(&self) -> Result<PlatformInfo> {
        Self::get_json(&self.client, self.url(&["api", "get-platform-info"])?).await
    }

    /// Fetches whether the app has Android's manage-external-storage permission.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response is an API,
    /// HTTP, or JSON error.
    pub async fn check_manage_storage_permission(&self) -> Result<ManageStoragePermission> {
        Self::get_json(
            &self.client,
            self.url(&["api", "check-manage-storage-permission"])?,
        )
        .await
    }

    /// Fetches Android SDK and release information.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response is an API,
    /// HTTP, or JSON error.
    pub async fn get_android_version(&self) -> Result<AndroidVersion> {
        Self::get_json(&self.client, self.url(&["api", "get-android-version"])?).await
    }

    /// Lists a directory relative to the selected storage root, such as
    /// `"Pictures/Whatsapp"`.
    ///
    /// The selected root is prepended to `path`, then appended to
    /// `/api/directory/` without a `root` prefix. Returns directories followed
    /// by files.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPath`] if `path` is not a valid relative path.
    /// Also returns an error if the request fails or the response contains an
    /// API, HTTP, or JSON error.
    pub async fn directory(&self, path: impl AsRef<str>) -> Result<Directory> {
        let path = directory_path(self.storages.root_path(), path.as_ref())?;
        let path = path.strip_prefix('/').expect("storage root is absolute");
        let mut url_segments = vec!["api", "directory"];
        if path.is_empty() {
            url_segments.push("");
        } else {
            url_segments.extend(path.split('/'));
        }
        let url = self.url(&url_segments)?;
        let directory: Directory = Self::get_json(&self.client, url.clone()).await?;
        info!(
            url = %url,
            path,
            "got {} dirs, {} files",
            directory.directories.len(),
            directory.files.len(),
        );
        Ok(directory)
    }

    /// Downloads a file from the server path.
    ///
    /// Returns the file bytes and elapsed request time in a [`DownloadedFile`].
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response contains an API
    /// or HTTP error.
    pub async fn download_file(&self, path: impl AsRef<str>) -> Result<DownloadedFile> {
        let start = Instant::now();
        let path = path.as_ref();
        let path = path.strip_prefix('/').unwrap_or(path);
        let url = self.url(&path.split('/').collect::<Vec<_>>())?;
        let response = self.client.get(url.clone()).send().await?;
        let status = response.status();
        let json_response = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
        let bytes = response.bytes().await?;
        if json_response && let Ok(error) = serde_json::from_slice::<ApiErrorResponse>(&bytes) {
            return Err(Error::Api(error.error));
        }
        if !status.is_success() {
            return Err(http_status_error(status, &bytes));
        }

        trace!(
            url = %url,
            path,
            "downloaded file: status = {status}, size = {}",
            Byte::from(bytes.len()),
        );
        Ok(DownloadedFile {
            bytes: bytes.to_vec(),
            elapsed: start.elapsed(),
        })
    }

    async fn get_json<T: DeserializeOwned>(
        client: &reqwest::Client,
        url: impl reqwest::IntoUrl,
    ) -> Result<T> {
        let response = client.get(url).send().await?;
        let status = response.status();
        let body = response.bytes().await?;

        if let Ok(error) = serde_json::from_slice::<ApiErrorResponse>(&body) {
            return Err(Error::Api(error.error));
        }
        if !status.is_success() {
            return Err(http_status_error(status, &body));
        }

        Ok(serde_json::from_slice(&body)?)
    }

    fn url(&self, path_segments: &[&str]) -> Result<Url> {
        server_url(self.address, path_segments)
    }
}

fn server_url(address: SocketAddr, path_segments: &[&str]) -> Result<Url> {
    let mut url = Url::parse(&format!("http://{address}/"))
        .map_err(|error| Error::InvalidUrl(error.to_string()))?;
    url.path_segments_mut()
        .expect("HTTP URLs support path segments")
        .pop_if_empty()
        .extend(path_segments.iter().copied());
    Ok(url)
}

fn http_status_error(status: StatusCode, body: &[u8]) -> Error {
    Error::HttpStatus {
        status: status.as_u16(),
        body: String::from_utf8_lossy(body).into_owned(),
    }
}

/// Platform identifier returned by `/api/get-platform-info`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PlatformInfo {
    /// Platform identifier, such as `"android"`.
    pub platform: String,
}

/// Permission state returned by `/api/check-manage-storage-permission`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ManageStoragePermission {
    /// Whether Android's manage-external-storage permission is granted.
    #[serde(rename = "isExternalStorageManager")]
    pub is_external_storage_manager: bool,
}

/// Android version returned by `/api/get-android-version`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AndroidVersion {
    /// Android SDK version number.
    #[serde(rename = "SDK_INT")]
    pub sdk_int: u32,
    /// Android release identifier.
    #[serde(rename = "RELEASE")]
    pub release: String,
}

/// Directory listing returned by the WFT API.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Directory {
    /// Directories in the listing.
    #[serde(default)]
    pub directories: Vec<Entry>,
    /// Files in the listing.
    #[serde(default)]
    pub files: Vec<Entry>,
}

impl Directory {
    pub fn into_entries(self) -> impl Iterator<Item = DirectoryEntry> {
        self.directories
            .into_iter()
            .map(DirectoryEntry::Directory)
            .chain(self.files.into_iter().map(DirectoryEntry::File))
    }
}

/// A file or directory returned by [`Wft::directory`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryEntry {
    /// A child directory.
    Directory(Entry),
    /// A file contained in the listed directory.
    File(Entry),
}

/// A file or directory in a listing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Entry {
    /// Entry name.
    pub name: String,
    /// Entry size in bytes.
    pub size: u64,
    /// Absolute server path of the entry.
    pub path: String,
    /// Modification timestamp reported by the server.
    pub modified: u64,
    /// File extension, or an empty string when none is reported.
    pub extension: String,
}

fn directory_path(root: &str, path: &str) -> Result<String> {
    if path.starts_with('/') {
        return Err(Error::InvalidPath(path.to_owned()));
    }
    if !path.is_empty()
        && path.split('/').any(|component| {
            component.is_empty() || matches!(component, "." | "..") || component.contains('\\')
        })
    {
        return Err(Error::InvalidPath(path.to_owned()));
    }
    let root = root.trim_end_matches('/');
    let root = if root.is_empty() { "/" } else { root };
    Ok(if path.is_empty() {
        root.to_owned()
    } else if root == "/" {
        format!("{root}{path}")
    } else {
        format!("{root}/{path}")
    })
}

impl Entry {
    /// Compares this remote entry with a local file.
    ///
    /// `DifferentTimestamp` is returned when the local file exists but its size
    /// or modification time differs from this entry.
    pub fn compare_local_file(&self, local_path: impl AsRef<Path>) -> FileStatus {
        let metadata = match std::fs::metadata(local_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return FileStatus::Missing;
            }
            Err(error) => return FileStatus::MetadataError(error),
        };
        if metadata.len() != self.size {
            return FileStatus::DifferentTimestamp;
        }

        let modified = match metadata.modified() {
            Ok(modified) => modified,
            Err(error) => return FileStatus::ModifiedTimeError(error),
        };
        if modified
            .duration_since(UNIX_EPOCH)
            .ok()
            .is_some_and(|duration| duration.as_millis() == u128::from(self.modified))
        {
            FileStatus::UpToDate
        } else {
            FileStatus::DifferentTimestamp
        }
    }
}

/// Whether a local file matches a remote directory entry.
#[derive(Debug)]
pub enum FileStatus {
    /// The local file exists and matches the remote size and modification time.
    UpToDate,
    /// No local file exists at the requested path.
    Missing,
    /// The local file exists but differs in size or modification time.
    DifferentTimestamp,
    /// Reading the local file metadata failed.
    MetadataError(std::io::Error),
    /// Reading the local file modification time failed.
    ModifiedTimeError(std::io::Error),
}

/// Bytes downloaded from WFT and the elapsed request time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadedFile {
    /// Downloaded file contents.
    pub bytes: Vec<u8>,
    /// Time spent performing the download request.
    pub elapsed: Duration,
}

#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: String,
}

/// Result type used by the WFT client.
pub type Result<T> = core::result::Result<T, Error>;

/// Errors returned by the WFT client.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// The WFT API returned an error payload.
    #[error("WFT API error: {0}")]
    Api(String),
    /// The server returned a non-success HTTP status.
    #[error("HTTP request failed with status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    /// A server path is not valid for the requested operation.
    #[error("invalid server path: {0}")]
    InvalidPath(String),
    /// A request URL could not be constructed.
    #[error("invalid request URL: {0}")]
    InvalidUrl(String),
    /// The HTTP client could not be built or a request failed.
    #[error("network failure: {0}")]
    NetworkFailure(#[from] reqwest::Error),
    /// A JSON response could not be decoded.
    #[error("unexpected data format: {0}")]
    UnexpectedFormat(#[from] serde_json::Error),
    /// The ping endpoint did not return `pong`.
    #[error("unexpected ping response: {0}")]
    UnexpectedPingResponse(String),
    /// The storage endpoint returned no storage entries.
    #[error("WFT returned no storage details")]
    NoStorages,
    /// The requested storage root was not reported by WFT.
    #[error("storage root {0:?} was not reported by WFT")]
    StorageNotFound(String),
}
