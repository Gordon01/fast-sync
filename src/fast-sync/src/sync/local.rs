//! Compares remote files with local copies and totals the planned transfer.

use std::{
    io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use wft::Entry;

/// Remote files paired with local paths, comparison results, and byte totals.
pub(super) struct LocalFiles {
    /// Files in server order, with their local paths and comparison results.
    pub files: Vec<(Entry, PathBuf, FileStatus)>,
    /// Combined size of all remote files.
    pub total_bytes: u64,
    /// Combined size of files whose local copies already match.
    pub up_to_date_bytes: u64,
}

impl LocalFiles {
    /// Inspects local copies under `directory` and calculates transfer totals.
    pub fn new(files: Vec<Entry>, directory: &Path) -> Self {
        let mut total_bytes = 0;
        let mut up_to_date_bytes = 0;
        let files = files
            .into_iter()
            .map(|file| {
                total_bytes += file.size;
                let path = directory.join(&file.name);
                let status = compare_local_file(&file, &path);
                if matches!(status, FileStatus::UpToDate) {
                    up_to_date_bytes += file.size;
                }
                (file, path, status)
            })
            .collect();
        Self {
            files,
            total_bytes,
            up_to_date_bytes,
        }
    }
}

/// Whether a local copy matches its remote file's size and modification time.
#[derive(Debug)]
pub(super) enum FileStatus {
    /// The local copy matches the remote size and timestamp.
    UpToDate,
    /// No local copy exists at the requested path.
    Missing,
    /// The local copy differs in size or modification time.
    DifferentTimestamp,
    /// Reading local file metadata failed.
    MetadataError(io::Error),
    /// Reading the local modification time failed.
    ModifiedTimeError(io::Error),
}

/// Compares file size and millisecond modification time without reading contents.
fn compare_local_file(entry: &Entry, path: &Path) -> FileStatus {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return FileStatus::Missing,
        Err(error) => return FileStatus::MetadataError(error),
    };
    if metadata.len() != entry.size {
        return FileStatus::DifferentTimestamp;
    }
    let modified = match metadata.modified() {
        Ok(modified) => modified,
        Err(error) => return FileStatus::ModifiedTimeError(error),
    };
    if modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .is_some_and(|duration| duration.as_millis() == u128::from(entry.modified))
    {
        FileStatus::UpToDate
    } else {
        FileStatus::DifferentTimestamp
    }
}
