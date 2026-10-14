mod stats;
mod wft;

use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use anyhow::Context;
use clap::Parser;
use readable::{byte::Byte, up::Uptime};
use stats::TransferStats;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;
use wft::{Directory, Wft};

/// Syncs files from a Wi‑Fi File Transfer server to the local filesystem.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    /// Server IP address.
    #[arg(long, short)]
    ip: IpAddr,

    /// Server port.
    #[arg(long, short, default_value_t = 1234)]
    port: u16,

    /// Remote directory to sync.
    #[arg(long, short, default_value = "/DCIM/Camera")]
    directory: String,

    /// Local output directory.
    #[arg(long, short, default_value = "out")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    debug!("CL arguments: {cli:#?}");
    let wft = Wft::new(SocketAddr::new(cli.ip, cli.port));
    let directory = wft.directory(&cli.directory).await?;

    process_files(&cli, &directory, &wft).await
}

#[derive(Debug)]
enum FileStatus {
    Missing,
    UpToDate,
    Outdated,
}

/// Downloads files that are missing or newer/changed on the remote endpoint.
async fn process_files(cli: &Cli, directory: &Directory, wft: &Wft) -> anyhow::Result<()> {
    let out_dir = {
        let mut dir = cli.output.clone();
        dir.extend(cli.directory.split('/'));
        dir
    };
    std::fs::create_dir_all(&out_dir)?;

    let total_bytes: u64 = directory.files.iter().map(|e| e.size).sum();
    let mut stats = TransferStats::new(total_bytes);

    for file in &directory.files {
        let out_file = out_dir.join(&file.name);
        let status = file_status(&out_file, file.size, file.modified);

        debug!(
            "Checking {}: status = {status:?}, size = {}, ETA = {}",
            out_file.display(),
            Byte::from(file.size),
            Uptime::from(file.size / stats.speed().max(1))
        );
        if let FileStatus::UpToDate = status {
            continue;
        }

        let out_path = format!("{}/{}", cli.directory.trim_end_matches('/'), file.name);
        let (bytes, download_time) = wft.download_file(&out_path).await?;
        std::fs::write(&out_file, &bytes)
            .with_context(|| format!("Failed to write output file {:?}", out_file))?;
        stats.update(bytes.len() as u64);

        info!(
            "Bytes left: {}, {}/s, {:.2}% ETA: {}, done: {} in {}",
            Byte::from(stats.left()),
            Byte::from(stats.speed()),
            stats.progress() * 100.0,
            if let Some(eta) = stats.eta() {
                Uptime::from(eta).to_string()
            } else {
                "∞".to_string()
            },
            out_file.display(),
            Uptime::from(download_time)
        );
    }

    Ok(())
}

/// Compares the local file with the remote listing to decide whether it should be synced.
fn file_status(out_file: &PathBuf, remote_size: u64, _remote_mtime: u64) -> FileStatus {
    match std::fs::metadata(out_file) {
        Ok(metadata) => {
            let local_size = metadata.len();
            if local_size == remote_size {
                FileStatus::UpToDate
            } else {
                FileStatus::Outdated
            }
        }
        Err(_) => FileStatus::Missing,
    }
}
