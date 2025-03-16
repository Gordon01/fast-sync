use std::net::SocketAddr;

use clap::builder::Str;
use reqwest::{Method, Request};
use serde::Deserialize;
use tracing::{debug, info};

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
        let res: Directory = serde_json::from_str(&req)?;
        info!(
            url,
            path,
            "got {} dirs, {} files",
            res.directories.len(),
            res.files.len()
        );
        Ok(res)
    }

    pub async fn download_file(&self, path: impl AsRef<str>) -> Result<Vec<u8>> {
        let path = path.as_ref();
        let url = format!("http://{}/{}", self.address, path);
        let req = reqwest::get(&url).await?;
        let status = req.status();
        let bytes = req.bytes().await?;
        info!(url, path, "status: {status}, len {}", bytes.len(),);
        Ok(bytes.to_vec())
    }
}

#[derive(Debug, Deserialize)]
pub struct Directory {
    pub(crate) directories: Vec<Entry>,
    pub(crate) files: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) path: String,
    pub(crate) modified: u64,
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
