use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use fast_sync::wft::{Directory, Storage, Wft};
use readable::{byte::Byte, up::Uptime};
use tracing::{debug, info};

use crate::stats::TransferStats;

#[derive(Debug, Args)]
pub struct PullArgs {
    /// Storage root to sync from.
    #[arg(long, short)]
    storage: Option<String>,

    /// Directory within the selected storage to sync.
    #[arg(long, short, default_value = "DCIM/Camera")]
    directory: String,

    /// Local output directory.
    #[arg(long, short, default_value = "out")]
    output: PathBuf,
}

#[derive(Debug)]
enum FileStatus {
    Missing,
    UpToDate,
    Outdated,
}

pub async fn run(wft: &Wft, args: &PullArgs) -> anyhow::Result<()> {
    let storages = wft.get_storage_details().await?;
    if storages.is_empty() {
        anyhow::bail!("WFT returned no storage details");
    }
    let selected_storage = select_storage(&storages, args.storage.as_deref())?;
    let selected_root = selected_storage.root_path.as_str();

    info!(
        "Selected storage {} ({} total bytes)",
        selected_storage.root_path, selected_storage.total_bytes
    );

    let directory = args.directory.trim_matches('/');
    let remote_directory = if directory.is_empty() {
        selected_root.to_owned()
    } else if args.directory == selected_root
        || args.directory.starts_with(&format!("{selected_root}/"))
    {
        args.directory.trim_end_matches('/').to_owned()
    } else if args.directory.starts_with('/') {
        anyhow::bail!(
            "--directory must be relative to the selected storage ({selected_root}) or be inside it"
        );
    } else {
        format!("{selected_root}/{directory}")
    };
    let directory = wft.directory(&remote_directory).await?;

    process_files(args, &remote_directory, &directory, wft).await
}

fn select_storage<'a>(
    storages: &'a [Storage],
    requested_root: Option<&str>,
) -> anyhow::Result<&'a Storage> {
    if let Some(storage) = requested_root {
        return storages
            .iter()
            .find(|summary| summary.root_path == storage)
            .with_context(|| format!("Storage root {storage:?} was not reported by WFT"));
    }

    storages
        .iter()
        .max_by_key(|summary| summary.total_bytes)
        .context("WFT returned no storage details")
}

/// Downloads files that are missing or newer/changed on the remote endpoint.
async fn process_files(
    args: &PullArgs,
    remote_directory: &str,
    directory: &Directory,
    wft: &Wft,
) -> anyhow::Result<()> {
    let out_dir = {
        let mut dir = args.output.clone();
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
        let download = wft.download_file(&out_path).await?;
        std::fs::write(&out_file, &download.bytes)
            .with_context(|| format!("Failed to write output file {:?}", out_file))?;
        stats.update(download.bytes.len() as u64);

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
            Uptime::from(download.elapsed)
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
