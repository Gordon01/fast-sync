use serde::Deserialize;

/// Remote directory listing returned by [`Wft::directory`](crate::Wft::directory).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Directory {
    /// Directories in the listing.
    #[serde(default)]
    pub directories: Vec<Entry>,
    /// Files in the listing.
    #[serde(default)]
    pub files: Vec<Entry>,
}

/// Remote file or directory metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Entry {
    /// Entry name.
    pub name: String,
    /// Entry size in bytes.
    pub size: u64,
    /// Absolute server path, usable with [`Wft::download_file`](crate::Wft::download_file).
    pub path: String,
    /// Modification time in milliseconds since the Unix epoch.
    pub modified: u64,
    /// File extension, or an empty string when none is reported.
    pub extension: String,
}
