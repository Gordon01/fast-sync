use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use readable::byte::Byte;
use serde::Deserialize;
use tracing::{info, trace};

/// Wi-Fi File Transfer protocol from
/// https://play.google.com/store/apps/details?id=com.techprd.filetransfer
pub struct Wft {
    address: SocketAddr,
    //client: reqwest::
}

impl Wft {
    pub fn new(address: SocketAddr) -> Self {
        Self { address }
    }

    pub async fn directory(&self, path: impl AsRef<str>) -> Result<Directory> {
        let path = path.as_ref();
        let url = format!("http://{}/api/directory/root{}", self.address, path);
        let req = reqwest::get(&url).await?;
        let status = req.status();
        let req = req.text().await?;
        info!("status: {status}, len {}", req.len());

        let dir: Directory = serde_json::from_str(&req)?;

        info!(
            url,
            path,
            "got {} dirs, {} files",
            dir.directories.len(),
            dir.files.len()
        );
        Ok(dir)
    }

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

#[derive(Debug, Deserialize)]
pub struct Directory {
    #[serde(default)]
    pub(crate) directories: Vec<Entry>,
    #[serde(default)]
    pub(crate) files: Vec<Entry>,
}

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

type Result<T> = core::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("network failure: {0}")]
    NetworkFailure(#[from] reqwest::Error),
    #[error("unexpected data format: {0}")]
    UnexpectedFormat(#[from] serde_json::Error),
}
