use std::fmt;

use readable::byte::Byte;
use serde::Deserialize;

use crate::{Error, Result};

/// One content entry returned by `/api/get-storage-details`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ContentGroup {
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

impl fmt::Display for ContentGroup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.name, Byte::from(self.size))
    }
}

/// A storage root reported by WFT, including its total and available capacity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageRoot {
    /// Absolute path of the storage root.
    pub root_path: String,
    /// Total capacity in bytes of the storage root.
    pub total_bytes: u64,
    /// Available capacity in bytes of the storage root.
    pub available_bytes: u64,
    contents: Vec<ContentGroup>,
}

/// Storage roots reported by WFT and the currently selected root.
///
/// The initial selection is the root with the greatest reported capacity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Storages {
    pub roots: Vec<StorageRoot>,
    pub selected: usize,
}

impl Storages {
    pub(super) fn new(entries: Vec<ContentGroup>) -> Result<Self> {
        let mut roots: Vec<StorageRoot> = Vec::new();

        for content in entries {
            let root_index = roots
                .iter()
                .position(|root| root.root_path == content.root_path)
                .unwrap_or_else(|| {
                    roots.push(StorageRoot {
                        root_path: content.root_path.clone(),
                        total_bytes: content.total_bytes,
                        available_bytes: content.available_bytes,
                        contents: Vec::new(),
                    });
                    roots.len() - 1
                });
            roots[root_index].contents.push(content);
        }

        let selected = roots
            .iter()
            .enumerate()
            .max_by_key(|(_, root)| root.total_bytes)
            .map(|(index, _)| index)
            .ok_or(Error::NoStorages)?;
        Ok(Self { roots, selected })
    }

    /// Returns all distinct storage roots reported by WFT.
    pub fn roots(&self) -> &[StorageRoot] {
        &self.roots
    }

    /// Returns the currently selected storage root.
    pub fn selected_root(&self) -> &StorageRoot {
        &self.roots[self.selected]
    }
}

impl fmt::Display for Storages {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for root in &self.roots {
            writeln!(
                formatter,
                "{}: {} used of {} ({} free)",
                root.root_path,
                Byte::from(root.total_bytes.saturating_sub(root.available_bytes)),
                Byte::from(root.total_bytes),
                Byte::from(root.available_bytes)
            )?;
            for content in &root.contents {
                writeln!(
                    formatter,
                    "  {}: {}",
                    content.name,
                    Byte::from(content.size)
                )?;
            }
        }
        Ok(())
    }
}
