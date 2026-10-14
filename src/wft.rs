use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use readable::byte::Byte;
use serde::Deserialize;
use tracing::{info, trace};

/// Wi‑Fi File Transfer client for the device HTTP API.
///
/// This client talks to a single host at `http://<address>/...` and exposes the small subset of
/// operations used here: listing a directory and downloading a file.
pub struct Wft {
    address: SocketAddr,
    //client: reqwest::
}

impl Wft {
    /// Creates a client bound to a specific Wi‑Fi File Transfer device address.
    pub fn new(address: SocketAddr) -> Self {
        Self { address }
    }

    /// Lists a directory on the device.
    ///
    /// `path` is appended to the server's directory endpoint, so values like `"/"` or
    /// `"/Pictures"` are typical.
    ///
    /// # Errors
    ///
    /// Returns an error for a failed request, a not-found response, or a payload that does not
    /// match [`Directory`].
    pub async fn directory(&self, path: impl AsRef<str>) -> Result<Directory> {
        let path = path.as_ref();
        let url = format!("http://{}/api/directory/root{}", self.address, path);
        let req = reqwest::get(&url).await?;
        let status = req.status();
        let req = req.text().await?;
        let content_len = req.len();

        if let Ok(e) = serde_json::from_str::<ErrorResponse>(&req) {
            return Err(Error::NotFound(e.error));
        }

        let dir: Directory = serde_json::from_str(&req)?;
        info!(
            url,
            path,
            "got {} dirs, {} files, total {} ({status})",
            dir.directories.len(),
            dir.files.len(),
            Byte::from(content_len)
        );
        Ok(dir)
    }

    /// Downloads a file from the device.
    ///
    /// The returned tuple contains the file bytes and the elapsed request time.
    ///
    /// # Errors
    ///
    /// Returns an error when the request or body read fails.
    pub async fn download_file(&self, path: impl AsRef<str>) -> Result<(Vec<u8>, Duration)> {
        let start = Instant::now();
        let path = path.as_ref();
        let url = format!("http://{}/{}", self.address, path);
        let req = reqwest::get(&url).await?;
        let status = req.status();
        let bytes = req.bytes().await?;
        trace!(
            url,
            path,
            "downloaded file: status = {status}, size = {}",
            Byte::from(bytes.len()),
        );
        Ok((bytes.to_vec(), start.elapsed()))
    }
}

/// Directory listing from the device API.
#[derive(Debug, Deserialize)]
pub struct Directory {
    #[serde(default)]
    pub(crate) directories: Vec<Entry>,
    #[serde(default)]
    pub(crate) files: Vec<Entry>,
}

/// One entry in a directory listing.
#[derive(Debug, Deserialize)]
pub struct Entry {
    pub(crate) name: String,
    pub(crate) size: u64,
    #[allow(unused)]
    pub(crate) path: String,
    #[allow(unused)] // TODO use for resolving conflicts
    pub(crate) modified: u64,
    #[allow(unused)] // Unsure
    pub(crate) extension: String,
}

/// Error payload returned by the device.
#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    error: String,
}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("network failure: {0}")]
    NetworkFailure(#[from] reqwest::Error),
    #[error("unexpected data format: {0}")]
    UnexpectedFormat(#[from] serde_json::Error),
}
