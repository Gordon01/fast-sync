use std::fmt;

use readable::byte::Byte;
use serde::Deserialize;

use super::{Error, Result};

/// One storage location returned by `/api/get-storage-details`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Storage {
    /// Path of the storage entry.
    pub path: String,
    /// Size in bytes of this storage entry.
    pub size: u64,
    /// Display name of this storage entry.
    pub name: String,
    /// Total capacity in bytes of the storage root.
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    /// Absolute path of the storage root.
    #[serde(rename = "rootPath")]
    pub root_path: String,
    /// Available capacity in bytes of the storage root.
    #[serde(rename = "availableBytes")]
    pub available_bytes: u64,
}

impl fmt::Display for Storage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.name, Byte::from(self.size))
    }
}

/// Storage entries reported by WFT, with one root selected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Storages {
    entries: Vec<Storage>,
    selected: usize,
}

impl Storages {
    pub(super) fn new(entries: Vec<Storage>) -> Result<Self> {
        let selected = entries
            .iter()
            .enumerate()
            .max_by_key(|(_, storage)| storage.total_bytes)
            .map(|(index, _)| index)
            .ok_or(Error::NoStorages)?;
        Ok(Self { entries, selected })
    }

    pub fn select(&mut self, root_path: impl AsRef<str>) -> Result<()> {
        let root_path = root_path.as_ref();
        let selected = self
            .entries
            .iter()
            .position(|storage| storage.root_path == root_path)
            .ok_or_else(|| Error::StorageNotFound(root_path.to_owned()))?;
        self.selected = selected;
        Ok(())
    }

    /// Returns the selected storage root path.
    pub fn root_path(&self) -> &str {
        &self.entries[self.selected].root_path
    }

    /// Returns the selected storage's total capacity in bytes.
    pub fn total_bytes(&self) -> u64 {
        self.entries[self.selected].total_bytes
    }

    /// Returns the selected storage's available capacity in bytes.
    pub fn available_bytes(&self) -> u64 {
        self.entries[self.selected].available_bytes
    }

    /// Returns all entries reported for available storage locations.
    pub fn entries(&self) -> &[Storage] {
        &self.entries
    }
}

impl fmt::Display for Storages {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut roots: Vec<(&str, Vec<&Storage>)> = Vec::new();
        for storage in &self.entries {
            if let Some((_, entries)) = roots
                .iter_mut()
                .find(|(root_path, _)| *root_path == storage.root_path)
            {
                entries.push(storage);
            } else {
                roots.push((&storage.root_path, vec![storage]));
            }
        }

        for (root_path, entries) in roots {
            let total_bytes = entries[0].total_bytes;
            let available_bytes = entries[0].available_bytes;
            writeln!(
                formatter,
                "{root_path}: {} used of {} ({} free)",
                Byte::from(total_bytes.saturating_sub(available_bytes)),
                Byte::from(total_bytes),
                Byte::from(available_bytes)
            )?;
            for storage in entries {
                writeln!(formatter, "  {storage}")?;
            }
        }
        Ok(())
    }
}
