use std::{
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, Instant},
};

mod directory;
mod storage;
pub use directory::{Directory, DirectoryEntry, DirectoryIterator, Entry, FileStatus};
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
    output_path: PathBuf,
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
        let response = client.get(url.clone()).send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(http_status_error(&url, status, &body));
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
            output_path: PathBuf::from("out"),
        })
    }

    /// Sets the local output root.
    pub fn set_output_path(&mut self, path: impl Into<PathBuf>) -> std::io::Result<()> {
        self.output_path = path.into();
        std::fs::create_dir_all(&self.output_path)
    }

    /// Returns storage details fetched when this client was constructed.
    pub fn storages(&self) -> &Storages {
        &self.storages
    }

    pub fn select_storage(&mut self, root_path: impl AsRef<str>) -> Result<()> {
        let root_path = root_path.as_ref();
        let selected = self
            .storages
            .roots
            .iter()
            .position(|root| root.root_path == root_path)
            .ok_or_else(|| Error::StorageNotFound(root_path.to_owned()))?;
        self.storages.selected = selected;
        Ok(())
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

    /// Lists a directory relative to the storage root, such as
    /// `"Pictures/Whatsapp"`, and compares its files with the corresponding
    /// local output paths.
    pub async fn directory(&self, path: impl AsRef<str>) -> Result<DirectoryIterator> {
        let relative_path = path.as_ref();
        let root = &self.storages.selected_root().root_path;
        let mut segments = vec!["api", "directory"];
        segments.extend(
            root.split('/')
                .chain(relative_path.split('/'))
                .filter(|segment| !segment.is_empty()),
        );
        if segments.len() == 2 {
            segments.push("");
        }
        let url = self.url(&segments)?;
        let directory: Directory = Self::get_json(&self.client, url.clone()).await?;
        info!(
            url = %url,
            path = %relative_path,
            "got {} dirs, {} files",
            directory.directories.len(),
            directory.files.len(),
        );
        let local_directory = self.output_path.join(relative_path);
        Ok(directory.iter(local_directory))
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
            return Err(http_status_error(&url, status, &bytes));
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

    async fn get_json<T: DeserializeOwned>(client: &reqwest::Client, url: Url) -> Result<T> {
        let response = client.get(url.clone()).send().await?;
        let status = response.status();
        let body = response.bytes().await?;

        if let Ok(error) = serde_json::from_slice::<ApiErrorResponse>(&body) {
            return Err(Error::Api(error.error));
        }
        if !status.is_success() {
            return Err(http_status_error(&url, status, &body));
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

fn http_status_error(url: &Url, status: StatusCode, body: &[u8]) -> Error {
    Error::HttpStatus {
        url: url.to_string(),
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
    #[error("HTTP request to {url} failed with status {status}: {body}")]
    HttpStatus {
        url: String,
        status: u16,
        body: String,
    },
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
    /// The local output directory could not be created.
    #[error("failed to create output directory {path:?}: {source}")]
    OutputDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
