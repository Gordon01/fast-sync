use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::Deserialize;

/// Directory listing returned by the WFT API.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Directory {
    /// Directories in the listing.
    #[serde(default)]
    pub directories: Vec<Entry>,
    /// Files in the listing.
    #[serde(default)]
    pub files: Vec<Entry>,
}

impl Directory {
    /// Consumes this listing and iterates its entries using `local_directory`
    /// to determine the status of each local file.
    ///
    /// Directories are yielded before files.
    pub fn iter(self, local_directory: impl Into<PathBuf>) -> DirectoryIterator {
        let local_directory = local_directory.into();
        let mut total_bytes = 0_u64;
        let mut up_to_date_bytes = 0_u64;
        let files: Vec<_> = self
            .files
            .into_iter()
            .map(|file| {
                total_bytes += file.size;
                let local_path = local_directory.join(&file.name);
                let status = file.compare_local_file(local_path);
                if matches!(&status, FileStatus::UpToDate) {
                    up_to_date_bytes += file.size;
                }
                (file, status)
            })
            .collect();
        DirectoryIterator {
            directories: self.directories,
            files,
            local_directory,
            total_bytes,
            up_to_date_bytes,
        }
    }
}

/// Iterator over the entries in a directory listing.
pub struct DirectoryIterator {
    directories: Vec<Entry>,
    files: Vec<(Entry, FileStatus)>,
    local_directory: PathBuf,
    total_bytes: u64,
    up_to_date_bytes: u64,
}

impl DirectoryIterator {
    /// Returns the combined size of all files in the directory listing.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Returns the combined size of files that were up to date when the
    /// iterator was created.
    pub fn up_to_date_bytes(&self) -> u64 {
        self.up_to_date_bytes
    }
}

impl Iterator for DirectoryIterator {
    type Item = DirectoryEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(directory) = self.directories.pop() {
            return Some(DirectoryEntry::Directory(directory));
        }

        let (file, status) = self.files.pop()?;
        let local_path = self.local_directory.join(&file.name);
        Some(DirectoryEntry::File(file, local_path, status))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.directories.len() + self.files.len();
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for DirectoryIterator {}

/// A file or directory returned by [`DirectoryIterator`].
#[derive(Debug)]
pub enum DirectoryEntry {
    /// A child directory.
    Directory(Entry),
    /// A file contained in the listed directory.
    File(Entry, PathBuf, FileStatus),
}

/// A file or directory in a listing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Entry {
    /// Entry name.
    pub name: String,
    /// Entry size in bytes.
    pub size: u64,
    /// Absolute server path of the entry.
    pub path: String,
    /// Modification timestamp reported by the server.
    pub modified: u64,
    /// File extension, or an empty string when none is reported.
    pub extension: String,
}

impl Entry {
    /// Compares this remote entry with a local file.
    ///
    /// `DifferentTimestamp` is returned when the local file exists but its size
    /// or modification time differs from this entry.
    pub fn compare_local_file(&self, local_path: impl AsRef<Path>) -> FileStatus {
        let metadata = match std::fs::metadata(local_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return FileStatus::Missing;
            }
            Err(error) => return FileStatus::MetadataError(error),
        };
        if metadata.len() != self.size {
            return FileStatus::DifferentTimestamp;
        }

        let modified = match metadata.modified() {
            Ok(modified) => modified,
            Err(error) => return FileStatus::ModifiedTimeError(error),
        };
        if modified
            .duration_since(UNIX_EPOCH)
            .ok()
            .is_some_and(|duration| duration.as_millis() == u128::from(self.modified))
        {
            FileStatus::UpToDate
        } else {
            FileStatus::DifferentTimestamp
        }
    }
}

/// Whether a local file matches a remote directory entry.
#[derive(Debug)]
pub enum FileStatus {
    /// The local file exists and matches the remote size and modification time.
    UpToDate,
    /// No local file exists at the requested path.
    Missing,
    /// The local file exists but differs in size or modification time.
    DifferentTimestamp,
    /// Reading the local file metadata failed.
    MetadataError(std::io::Error),
    /// Reading the local file modification time failed.
    ModifiedTimeError(std::io::Error),
}
