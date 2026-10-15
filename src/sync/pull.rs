use std::{
    fs::{File, FileTimes},
    io::Write,
    path::PathBuf,
    time::{Duration, UNIX_EPOCH},
};

use anyhow::Context;
use clap::Args;
use fast_sync::wft::{DirectoryEntry, FileStatus, Wft};
use indicatif::{ProgressBar, ProgressStyle};
use readable::{byte::Byte, up::Uptime};
use tracing::info;

use crate::stats::TransferStats;

#[derive(Debug, Args)]
pub struct PullArgs {
    /// Storage root to sync from.
    #[arg(long, short)]
    pub storage: Option<String>,

    /// Relative directory within the selected storage to sync.
    #[arg(long, short, default_value = "DCIM/Camera")]
    pub directory: String,

    /// Local output directory.
    #[arg(long, short, default_value = "out")]
    pub output: PathBuf,
}

/// Downloads files that are missing or differ in size or modification time.
pub async fn pull(wft: &Wft, directory: String) -> anyhow::Result<()> {
    let entries = wft.directory(&directory).await?;
    let total_bytes = entries.total_bytes() - entries.up_to_date_bytes();
    let mut stats = TransferStats::new(total_bytes);
    let progress = ProgressBar::new(total_bytes);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta}) {msg}",
        )?
        .progress_chars("=>-"),
    );
    let mut downloaded_any = false;

    for entry in entries {
        let DirectoryEntry::File(file, out_file, status) = entry else {
            continue;
        };
        match status {
            FileStatus::UpToDate => continue,
            FileStatus::Missing | FileStatus::DifferentTimestamp => {}
            FileStatus::MetadataError(error) | FileStatus::ModifiedTimeError(error) => {
                return Err(error).with_context(|| format!("Failed to inspect {:?}", out_file));
            }
        }

        let download = wft.download_file(&file.path).await?;

        let basename = out_file.file_name();
        let modified = UNIX_EPOCH
            .checked_add(Duration::from_millis(file.modified))
            .context("Remote modification time is outside the supported timestamp range")?;

        let mut output = File::create(&out_file)?;
        output.write_all(&download.bytes)?;
        output
            .set_times(FileTimes::new().set_modified(modified))
            .with_context(|| format!("Failed to set modification time for {:?}", out_file))?;

        stats.update(download.bytes.len() as u64);

        downloaded_any = true;
        if let Some(m) = basename {
            progress.set_message(m.to_string_lossy().into_owned());
        };
        progress.set_position(stats.downloaded);
        progress.suspend(|| {
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
                basename.unwrap().to_string_lossy(),
                Uptime::from(download.elapsed)
            );
        });
    }

    if downloaded_any {
        progress.finish_with_message("Done");
    }

    Ok(())
}
