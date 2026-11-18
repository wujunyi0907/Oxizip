//! Destination helpers for extract-here / smart / archive-name modes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use oxizip_engine::EntryMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractLayout {
    /// Explicit `-o` path.
    Explicit,
    /// Archive's parent directory.
    Here,
    /// `<parent>/<archive-stem>/`.
    ArchiveName,
    /// One shared top-level folder → Here; otherwise ArchiveName.
    Smart,
}

pub fn archive_stem(archive: &Path) -> String {
    archive
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".into())
}

pub fn archive_parent(archive: &Path) -> PathBuf {
    archive
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn dest_here(archive: &Path) -> PathBuf {
    archive_parent(archive)
}

pub fn dest_archive_name(archive: &Path) -> PathBuf {
    archive_parent(archive).join(archive_stem(archive))
}

/// Smart layout used by shell "Extract Here (Smart)".
pub fn dest_smart(archive: &Path, entries: &[EntryMeta]) -> PathBuf {
    let tops: BTreeSet<&str> = entries
        .iter()
        .filter_map(|e| {
            let name = e.name.trim_matches('/');
            if name.is_empty() {
                None
            } else {
                Some(name.split('/').next().unwrap_or(name))
            }
        })
        .collect();
    if tops.len() == 1 {
        dest_here(archive)
    } else {
        dest_archive_name(archive)
    }
}

/// Default create output beside the first input: `<dir>/<name>.<ext>`.
pub fn default_create_path(first_input: &Path, format_ext: &str) -> PathBuf {
    let parent = first_input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = if first_input.is_dir() {
        first_input
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive".into())
    } else {
        first_input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive".into())
    };
    parent.join(format!("{stem}.{format_ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxizip_engine::EntryMeta;

    fn entry(name: &str, is_dir: bool) -> EntryMeta {
        EntryMeta {
            name: name.into(),
            is_dir,
            uncompressed_size: None,
            compressed_size: None,
            modified: None,
            method: None,
        }
    }

    #[test]
    fn smart_single_root_uses_here() {
        let archive = Path::new("/tmp/pack.zip");
        let entries = vec![entry("root/a.txt", false), entry("root/b.txt", false)];
        assert_eq!(dest_smart(archive, &entries), PathBuf::from("/tmp"));
    }

    #[test]
    fn smart_multi_root_uses_stem() {
        let archive = Path::new("/tmp/pack.zip");
        let entries = vec![entry("a.txt", false), entry("b.txt", false)];
        assert_eq!(dest_smart(archive, &entries), PathBuf::from("/tmp/pack"));
    }
}
