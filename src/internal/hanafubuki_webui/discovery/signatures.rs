//! Discovery signatures an instance type declares, and whether one matches
//! a directory the scanner visits.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::path::{Component, Path};

use crate::internal::hanafubuki_foundation::paths::{PathError, ResolvedPath};
use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;
use serde_json::{Map, Value};

use super::DeclaredDiscoveryCandidate;

/// A path below a directory, by its ordinary components; none is the
/// directory itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelativePath(Vec<String>);

impl RelativePath {
    /// `/`-separated components, each an ordinary name: `.` and empty
    /// components name the same directory, while `..`, a backslash, a root
    /// or a drive is refused.
    pub fn parse(text: &str) -> Result<Self, String> {
        let refused = || format!("`{text}` is not a relative path of ordinary names");
        if text.starts_with('/') {
            return Err(refused());
        }
        let mut components = Vec::new();
        for name in text.split('/') {
            if name.is_empty() || name == "." {
                continue;
            }
            let mut parsed = Path::new(name).components();
            let ordinary = matches!(
                (parsed.next(), parsed.next()),
                (Some(Component::Normal(_)), None)
            );
            if !ordinary || name.contains('\\') {
                return Err(refused());
            }
            components.push(name.to_string());
        }
        Ok(Self(components))
    }

    pub fn components(&self) -> &[String] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The path below `base`, which need not exist.
    pub fn below(&self, base: &ResolvedPath) -> Result<ResolvedPath, PathError> {
        self.0.iter().try_fold(base.clone(), |path, name| {
            ResolvedPath::resolve_child(&path, OsStr::new(name))
        })
    }
}

/// How a type recognizes an existing installation: the scanner reports a
/// directory where every marker exists as an instance of `type_id`.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoverySignature {
    pub type_id: InstanceTypeId,
    /// Files or directories below the matched directory, at least one.
    pub markers: Vec<RelativePath>,
    /// The instance root, below the matched directory or the directory itself.
    pub root: RelativePath,
    /// Declared components below the root; the others take their defaults.
    pub components: BTreeMap<String, RelativePath>,
    /// Install parameter values, read against the type's form at
    /// registration; parameters without one take their defaults.
    pub values: Map<String, Value>,
    /// The components an attached installation records as external, each
    /// declared `external = "allowed"`.
    pub external: BTreeSet<String>,
}

impl DiscoverySignature {
    /// The candidate this signature makes of `directory`, when every marker
    /// is there: a regular file or directory reached through directories on
    /// the scan's filesystem, never through a link. The first component of
    /// each is looked up in the entries the scanner already read.
    pub(super) fn candidate(
        &self,
        directory: &ResolvedPath,
        entries: &DirectoryEntries<'_>,
    ) -> Option<Result<DeclaredDiscoveryCandidate, PathError>> {
        if !self
            .markers
            .iter()
            .all(|marker| marker_present(directory, marker, entries))
        {
            return None;
        }
        Some(self.declared_candidate(directory))
    }

    fn declared_candidate(
        &self,
        directory: &ResolvedPath,
    ) -> Result<DeclaredDiscoveryCandidate, PathError> {
        let root = self.root.below(directory)?;
        let component_paths = self
            .components
            .iter()
            .map(|(name, path)| Ok((name.clone(), path.below(&root)?)))
            .collect::<Result<_, PathError>>()?;
        Ok(DeclaredDiscoveryCandidate {
            type_id: self.type_id.clone(),
            root,
            component_paths,
            values: self.values.clone(),
            external: self.external.clone(),
        })
    }
}

/// What the scanner read of one directory: its regular files and its
/// directories (links excluded), by UTF-8 name, and the scan's filesystem.
pub(super) struct DirectoryEntries<'a> {
    pub files: &'a HashSet<String>,
    pub directories: &'a HashSet<String>,
    pub filesystem: Option<u64>,
}

fn marker_present(
    directory: &ResolvedPath,
    marker: &RelativePath,
    entries: &DirectoryEntries<'_>,
) -> bool {
    let Some((first, rest)) = marker.components().split_first() else {
        return false;
    };
    if rest.is_empty() {
        return entries.files.contains(first) || entries.directories.contains(first);
    }
    if !entries.directories.contains(first) {
        return false;
    }
    let mut path = directory.clone();
    for (index, name) in marker.components().iter().enumerate() {
        let Ok(child) = ResolvedPath::resolve_child(&path, OsStr::new(name)) else {
            return false;
        };
        let Ok(metadata) = fs::symlink_metadata(child.system()) else {
            return false;
        };
        if is_link_like(&metadata) {
            return false;
        }
        let present = if index == rest.len() {
            metadata.is_file() || metadata.is_dir()
        } else {
            metadata.is_dir() && on_filesystem(&metadata, entries.filesystem)
        };
        if !present {
            return false;
        }
        path = child;
    }
    true
}

#[cfg(not(windows))]
fn is_link_like(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn is_link_like(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(unix)]
fn on_filesystem(metadata: &Metadata, filesystem: Option<u64>) -> bool {
    use std::os::unix::fs::MetadataExt;

    filesystem.is_none_or(|filesystem| metadata.dev() == filesystem)
}

#[cfg(not(unix))]
fn on_filesystem(_metadata: &Metadata, _filesystem: Option<u64>) -> bool {
    true
}
