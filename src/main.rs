mod stats;

use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use anyhow::Context;
use clap::{Parser, Subcommand};
use fast_sync::wft::{Directory, StorageSummary, Wft};
use readable::{byte::Byte, up::Uptime};
use stats::TransferStats;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

/// Syncs files from a Wi‑Fi File Transfer server to the local filesystem.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    /// Server IP address.
    #[arg(long, short)]
    ip: IpAddr,

    /// Server port.
    #[arg(long, short, default_value_t = 1234, global = true)]
    port: u16,

    /// Storage root to sync from.
    #[arg(long, default_value = "/storage/emulated/0")]
    storage: String,

    /// Directory within the selected storage to sync.
    #[arg(long, short, default_value = "DCIM/Camera")]
    directory: String,

    /// Local output directory.
    #[arg(long, short, default_value = "out")]
    output: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show server-reported storage and media sizes.
    StorageInfo,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    debug!("CL arguments: {cli:#?}");
    let wft = Wft::new(SocketAddr::new(cli.ip, cli.port)).await?;
    if matches!(cli.command, Some(Command::StorageInfo)) {
        let storages = wft.get_storage_details().await?;
        StorageSummary::print_storage_info(&storages);
        return Ok(());
    }

    let storages = wft.get_storage_details().await?;
    if storages.is_empty() {
        anyhow::bail!("WFT returned no storage details");
    }
    let selected_root = cli.storage.trim_end_matches('/');
    let selected_storage = storages
        .iter()
        .find(|storage| storage.root_path.trim_end_matches('/') == selected_root);
    let Some(selected_storage) = selected_storage else {
        let mut available = Vec::new();
        for storage in &storages {
            if !available.contains(&storage.root_path.as_str()) {
                available.push(storage.root_path.as_str());
            }
        }
        anyhow::bail!(
            "Storage root {:?} was not reported by WFT. Available storage roots: {}",
            cli.storage,
            available.join(", ")
        );
    };
    info!(
        "Selected storage {} ({} total bytes)",
        selected_storage.root_path, selected_storage.total_bytes
    );
    let directory = cli.directory.trim_matches('/');
    let remote_directory = if directory.is_empty() {
        selected_root.to_owned()
    } else if cli.directory == selected_root
        || cli.directory.starts_with(&format!("{selected_root}/"))
    {
        cli.directory.trim_end_matches('/').to_owned()
    } else if cli.directory.starts_with('/') {
        anyhow::bail!(
            "--directory must be relative to the selected storage ({selected_root}) or be inside it"
        );
    } else {
        format!("{selected_root}/{directory}")
    };
    let directory = wft.directory(&remote_directory).await?;

    process_files(&cli, &remote_directory, &directory, &wft).await
}

#[derive(Debug)]
enum FileStatus {
    Missing,
    UpToDate,
    Outdated,
}

/// Downloads files that are missing or newer/changed on the remote endpoint.
async fn process_files(
    cli: &Cli,
    remote_directory: &str,
    directory: &Directory,
    wft: &Wft,
) -> anyhow::Result<()> {
    let out_dir = {
        let mut dir = cli.output.clone();
        dir.extend(remote_directory.split('/').filter(|part| !part.is_empty()));
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

        let out_path = format!("{}/{}", remote_directory.trim_end_matches('/'), file.name);
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
