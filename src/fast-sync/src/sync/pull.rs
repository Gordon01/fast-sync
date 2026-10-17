use std::{
    fs::{File, FileTimes},
    io::Write,
    path::PathBuf,
    time::{Duration, UNIX_EPOCH},
};

use anyhow::Context;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use wft::Wft;

use super::local::{FileStatus, LocalFiles};

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
pub async fn pull(wft: &Wft, directory: String, output: PathBuf) -> anyhow::Result<()> {
    let local_directory = output.join(&directory);
    std::fs::create_dir_all(&local_directory)
        .with_context(|| format!("Failed to create {:?}", local_directory))?;
    let directory = wft.directory(&directory).await?;
    let entries = LocalFiles::new(directory.files, &local_directory);
    let total_bytes = entries.total_bytes - entries.up_to_date_bytes;
    let progress = ProgressBar::new(total_bytes);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta})
             {msg}",
        )?
        .progress_chars("=>-"),
    );
    let mut downloaded_any = false;

    for (file, out_file, status) in entries.files.into_iter().rev() {
        match status {
            FileStatus::UpToDate => continue,
            FileStatus::Missing | FileStatus::DifferentTimestamp => {}
            FileStatus::MetadataError(error) | FileStatus::ModifiedTimeError(error) => {
                return Err(error).with_context(|| format!("Failed to inspect {:?}", out_file));
            }
        }

        progress.set_message(file.name.clone());
        let download = wft.download_file(&file.path).await?;

        let modified = UNIX_EPOCH
            .checked_add(Duration::from_millis(file.modified))
            .context("Remote modification time is outside the supported timestamp range")?;

        let mut output = File::create(&out_file)?;
        output.write_all(&download.bytes)?;
        output
            .set_times(FileTimes::new().set_modified(modified))
            .with_context(|| format!("Failed to set modification time for {:?}", out_file))?;

        downloaded_any = true;
        progress.inc(download.bytes.len() as u64);
    }

    if downloaded_any {
        progress.finish_with_message("Done");
    }

    Ok(())
}
