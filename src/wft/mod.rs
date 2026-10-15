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
pub struct Wft {
    address: SocketAddr,
    client: reqwest::Client,
}

impl Wft {
    /// Pings the WFT server and creates a client for it.
    pub async fn new(address: SocketAddr) -> Result<Self> {
        debug!(%address, "Pinging WFT server");
        let client = reqwest::Client::new();
        let url = format!("http://{address}/api/ping");
        let response = client.get(&url).send().await?;
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

    /// Returns the platform identifier reported by the server.
    pub async fn get_platform_info(&self) -> Result<PlatformInfo> {
        self.get_json(format!("http://{}/api/get-platform-info", self.address))
            .await
    }

    /// Returns whether the app has Android's manage-external-storage permission.
    pub async fn check_manage_storage_permission(&self) -> Result<ManageStoragePermission> {
        self.get_json(format!(
            "http://{}/api/check-manage-storage-permission",
            self.address
        ))
        .await
    }

    /// Returns Android SDK and release information.
    pub async fn get_android_version(&self) -> Result<AndroidVersion> {
        self.get_json(format!("http://{}/api/get-android-version", self.address))
            .await
    }

    /// Returns summaries of the available storage locations.
    pub async fn get_storage_details(&self) -> Result<Vec<StorageSummary>> {
        self.get_json(format!("http://{}/api/get-storage-details", self.address))
            .await
    }

    /// Lists a directory using its absolute storage path, such as
    /// `"/storage/emulated/0/DCIM"`.
    ///
    /// The path is appended to `/api/directory/` without a `root` prefix.
    pub async fn directory(&self, path: impl AsRef<str>) -> Result<Directory> {
        let path = path.as_ref();
        let Some(path) = path.strip_prefix('/') else {
            return Err(Error::InvalidPath(path.to_owned()));
        };
        let path = path.strip_suffix('/').unwrap_or(path);
        let path_segments: Vec<&str> = if path.is_empty() {
            Vec::new()
        } else {
            path.split('/').collect()
        };
        if path_segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
        {
            return Err(Error::InvalidPath(path.to_owned()));
        }

        let mut url = Url::parse(&format!("http://{}/api/directory", self.address))
            .map_err(|error| Error::InvalidUrl(error.to_string()))?;
        {
            let mut segments = url
                .path_segments_mut()
                .expect("HTTP URLs support path segments");
            if path_segments.is_empty() {
                segments.push("");
            } else {
                segments.extend(path_segments);
            }
        }
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
    /// Returns the file bytes and elapsed request time.
    pub async fn download_file(&self, path: impl AsRef<str>) -> Result<(Vec<u8>, Duration)> {
        let start = Instant::now();
        let path = path.as_ref();
        let path = path.strip_prefix('/').unwrap_or(path);
        let path_segments: Vec<&str> = path.split('/').collect();
        if path_segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
        {
            return Err(Error::InvalidPath(path.to_owned()));
        }
        let mut url = Url::parse(&format!("http://{}/", self.address))
            .map_err(|error| Error::InvalidUrl(error.to_string()))?;
        url.path_segments_mut()
            .expect("HTTP URLs support path segments")
            .pop_if_empty()
            .extend(path_segments);
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
        Ok((bytes.to_vec(), start.elapsed()))
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
}

fn http_status_error(status: StatusCode, body: &[u8]) -> Error {
    Error::HttpStatus {
        status: status.as_u16(),
        body: String::from_utf8_lossy(body).into_owned(),
    }
}

/// Platform identifier returned by `/api/get-platform-info`.
#[derive(Debug, Deserialize)]
pub struct PlatformInfo {
    pub platform: String,
}

/// Permission state returned by `/api/check-manage-storage-permission`.
#[derive(Debug, Deserialize)]
pub struct ManageStoragePermission {
    #[serde(rename = "isExternalStorageManager")]
    pub is_external_storage_manager: bool,
}

/// Android version returned by `/api/get-android-version`.
#[derive(Debug, Deserialize)]
pub struct AndroidVersion {
    #[serde(rename = "SDK_INT")]
    pub sdk_int: u32,
    #[serde(rename = "RELEASE")]
    pub release: String,
}

/// One storage location returned by `/api/get-storage-details`.
#[derive(Debug, Deserialize)]
pub struct StorageSummary {
    pub path: String,
    pub size: u64,
    pub name: String,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    #[serde(rename = "rootPath")]
    pub root_path: String,
    #[serde(rename = "availableBytes")]
    pub available_bytes: u64,
}

impl StorageSummary {
    /// Prints the storage roots and media sizes reported by the server.
    pub fn print_storage_info(storages: &[Self]) {
        if storages.is_empty() {
            println!("No storage information returned.");
            return;
        }

        let mut roots: Vec<(&str, Vec<&Self>)> = Vec::new();
        for storage in storages {
            if let Some((_, entries)) = roots
                .iter_mut()
                .find(|(root_path, _)| *root_path == storage.root_path)
            {
                entries.push(storage);
            } else {
                roots.push((&storage.root_path, vec![storage]));
            }
        }

        for (root_path, entries) in roots {
            let total_bytes = entries[0].total_bytes;
            let available_bytes = entries[0].available_bytes;
            println!(
                "{root_path}: {} used of {} ({} free)",
                Byte::from(total_bytes.saturating_sub(available_bytes)),
                Byte::from(total_bytes),
                Byte::from(available_bytes)
            );
            for storage in entries {
                println!("  {storage}");
            }
        }
    }
}

impl fmt::Display for StorageSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.name, Byte::from(self.size))
    }
}

/// Directory listing returned by the WFT API.
#[derive(Debug, Deserialize)]
pub struct Directory {
    #[serde(default)]
    pub directories: Vec<Entry>,
    #[serde(default)]
    pub files: Vec<Entry>,
}

/// A file or directory in a listing.
#[derive(Debug, Deserialize)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub path: String,
    pub modified: u64,
    pub extension: String,
}

#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: String,
}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("WFT API error: {0}")]
    Api(String),
    #[error("HTTP request failed with status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("invalid server path: {0}")]
    InvalidPath(String),
    #[error("invalid request URL: {0}")]
    InvalidUrl(String),
    #[error("network failure: {0}")]
    NetworkFailure(#[from] reqwest::Error),
    #[error("unexpected data format: {0}")]
    UnexpectedFormat(#[from] serde_json::Error),
    #[error("unexpected ping response: {0}")]
    UnexpectedPingResponse(String),
}
