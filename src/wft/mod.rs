use std::{
    fmt,
    net::SocketAddr,
    time::{Duration, Instant},
};

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
}

impl Wft {
    /// Pings the WFT server and creates a client for it.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot be reached, responds with a
    /// non-success status, or does not return `pong`.
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

        Ok(Self { address, client })
    }

    /// Fetches the platform identifier reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response is an API,
    /// HTTP, or JSON error.
    pub async fn get_platform_info(&self) -> Result<PlatformInfo> {
        self.get_json(self.url(&["api", "get-platform-info"])?)
            .await
    }

    /// Fetches whether the app has Android's manage-external-storage permission.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response is an API,
    /// HTTP, or JSON error.
    pub async fn check_manage_storage_permission(&self) -> Result<ManageStoragePermission> {
        self.get_json(self.url(&["api", "check-manage-storage-permission"])?)
            .await
    }

    /// Fetches Android SDK and release information.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response is an API,
    /// HTTP, or JSON error.
    pub async fn get_android_version(&self) -> Result<AndroidVersion> {
        self.get_json(self.url(&["api", "get-android-version"])?)
            .await
    }

    /// Fetches summaries of the available storage locations.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response is an API,
    /// HTTP, or JSON error.
    pub async fn get_storage_details(&self) -> Result<Vec<Storage>> {
        self.get_json(self.url(&["api", "get-storage-details"])?)
            .await
    }

    /// Lists a directory using its absolute storage path, such as
    /// `"/storage/emulated/0/DCIM"`.
    ///
    /// The path is appended to `/api/directory/` without a `root` prefix.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPath`] if `path` is not absolute. Also returns
    /// an error if the request fails or the response contains an API, HTTP, or
    /// JSON error.
    pub async fn directory(&self, path: impl AsRef<str>) -> Result<Directory> {
        let input = path.as_ref();
        let Some(path) = input.strip_prefix('/') else {
            return Err(Error::InvalidPath(input.to_owned()));
        };
        let path = path.strip_suffix('/').unwrap_or(path);
        let mut url_segments = vec!["api", "directory"];
        if path.is_empty() {
            url_segments.push("");
        } else {
            url_segments.extend(path.split('/'));
        }
        let url = self.url(&url_segments)?;
        let directory: Directory = self.get_json(url.clone()).await?;
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

    async fn get_json<T: DeserializeOwned>(&self, url: impl reqwest::IntoUrl) -> Result<T> {
        let response = self.client.get(url).send().await?;
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

/// One storage location returned by `/api/get-storage-details`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Storage {
    /// Path of the storage entry.
    pub path: String,
    /// Size in bytes of this storage entry.
    pub size: u64,
    /// Display name of this storage entry.
    pub name: String,
    /// Total capacity in bytes of the storage root.
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    /// Absolute path of the storage root.
    #[serde(rename = "rootPath")]
    pub root_path: String,
    /// Available capacity in bytes of the storage root.
    #[serde(rename = "availableBytes")]
    pub available_bytes: u64,
}

impl fmt::Display for Storage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.name, Byte::from(self.size))
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_url_encodes_path_segments_and_preserves_trailing_empty_segment() {
        let address = SocketAddr::from(([127, 0, 0, 1], 1234));
        let url = server_url(address, &["api", "directory", "a b", ""]).unwrap();

        assert_eq!(url.as_str(), "http://127.0.0.1:1234/api/directory/a%20b/");
    }
}
