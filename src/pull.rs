use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use fast_sync::wft::{DirectoryEntry, FileStatus, Wft};
use filetime::FileTime;
use readable::{byte::Byte, up::Uptime};
use tracing::{debug, info};

use crate::stats::TransferStats;

#[derive(Debug, Args)]
pub struct PullArgs {
    /// Storage root to sync from.
    #[arg(long, short)]
    storage: Option<String>,

    /// Relative directory within the selected storage to sync.
    #[arg(long, short, default_value = "DCIM/Camera")]
    directory: String,

    /// Local output directory.
    #[arg(long, short, default_value = "out")]
    output: PathBuf,
}

pub async fn run(wft: &mut Wft, args: &PullArgs) -> anyhow::Result<()> {
    if let Some(root_path) = args.storage.as_deref() {
        wft.storages().select(root_path)?;
    }
    let selected_root = wft.storages().root_path().to_owned();
    let total_bytes = wft.storages().total_bytes();

    info!(
        "Selected storage {} ({} total bytes)",
        selected_root, total_bytes
    );

    validate_directory(&args.directory)?;
    let entries = wft.directory(&args.directory).await?;

    process_files(args, entries.into_entries(), wft).await
}

/// Downloads files that are missing or differ in size or modification time.
async fn process_files(
    args: &PullArgs,
    entries: impl Iterator<Item = DirectoryEntry>,
    wft: &Wft,
) -> anyhow::Result<()> {
    let out_dir = args.output.join(&args.directory);
    std::fs::create_dir_all(&out_dir)?;

    let mut files = Vec::new();
    for entry in entries {
        let DirectoryEntry::File(file) = entry else {
            continue;
        };
        let out_file = out_dir.join(&file.name);
        let status = match file.compare_local_file(&out_file) {
            FileStatus::MetadataError(error) | FileStatus::ModifiedTimeError(error) => {
                return Err(error).with_context(|| format!("Failed to inspect {:?}", out_file));
            }
            status => status,
        };
        files.push((file, out_file, status));
    }

    let total_bytes: u64 = files
        .iter()
        .filter(|(_, _, status)| !matches!(status, FileStatus::UpToDate))
        .map(|(file, _, _)| file.size)
        .sum();
    let mut stats = TransferStats::new(total_bytes);

    for (file, out_file, status) in files {
        debug!(
            "Checking {}: status = {status:?}, size = {}, ETA = {}",
            out_file.display(),
            Byte::from(file.size),
            Uptime::from(file.size / stats.speed().max(1))
        );
        match status {
            FileStatus::UpToDate => continue,
            FileStatus::Missing | FileStatus::DifferentTimestamp => {}
            FileStatus::MetadataError(error) | FileStatus::ModifiedTimeError(error) => {
                return Err(error).with_context(|| format!("Failed to inspect {:?}", out_file));
            }
        }

        let modified = remote_mtime(file.modified)?;
        let download = wft.download_file(&file.path).await?;
        std::fs::write(&out_file, &download.bytes)
            .with_context(|| format!("Failed to write output file {:?}", out_file))?;
        filetime::set_file_mtime(&out_file, modified)
            .with_context(|| format!("Failed to set modification time for {:?}", out_file))?;
        stats.update(download.bytes.len() as u64);

        info!(
            "Bytes left: {}, {}/s, {:.2}% ETA: {}, done: {} in {}",
            Byte::from(stats.left()),
            Byte::from(stats.speed()),
            if stats.total_bytes == 0 {
                100.0
            } else {
                stats.progress() * 100.0
            },
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

fn validate_directory(directory: &str) -> anyhow::Result<()> {
    if directory.is_empty() {
        return Ok(());
    }
    if directory.starts_with('/') {
        anyhow::bail!("--directory must be relative to the selected storage");
    }

    if directory.split('/').any(|component| {
        component.is_empty() || matches!(component, "." | "..") || component.contains('\\')
    }) {
        anyhow::bail!(
            "--directory must not contain empty, '.' or '..' path components or backslashes"
        );
    }
    Ok(())
}

fn remote_mtime(modified_millis: u64) -> anyhow::Result<FileTime> {
    let seconds = i64::try_from(modified_millis / 1_000)
        .context("Remote modification time is outside the supported timestamp range")?;
    let nanoseconds = ((modified_millis % 1_000) * 1_000_000) as u32;
    Ok(FileTime::from_unix_time(seconds, nanoseconds))
}
