use std::fs;
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult};
use walkdir::WalkDir;

use crate::types::{ConflictPolicy, CreateOptions};

/// If `path` is free, return it. Otherwise apply `policy` (skip → `None`).
pub fn resolve_conflict_path(path: PathBuf, policy: ConflictPolicy) -> Option<PathBuf> {
    if !path.exists() {
        return Some(path);
    }
    match policy {
        ConflictPolicy::Overwrite => Some(path),
        ConflictPolicy::Skip => None,
        ConflictPolicy::AutoRename => Some(unique_extract_path(&path)),
    }
}

/// `file.txt` → `file (1).txt` (first free index).
pub fn unique_extract_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let ext = path.extension().map(|s| s.to_string_lossy().into_owned());
    for n in 1..10_000 {
        let name = match &ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}.oxizip_new"))
}

/// A file to pack: absolute source path + archive-relative name (forward slashes).
#[derive(Debug, Clone)]
pub struct PackFile {
    pub source: PathBuf,
    pub name: String,
}

/// Expand user inputs (files and/or directories) into a flat file list.
pub fn collect_pack_files(
    inputs: &[&Path],
    cancel: &CancellationToken,
) -> CoreResult<Vec<PackFile>> {
    collect_pack_files_with_opts(inputs, &CreateOptions::default(), cancel)
}

/// Like [`collect_pack_files`], honoring exclude / Thumbs.db options.
pub fn collect_pack_files_with_opts(
    inputs: &[&Path],
    opts: &CreateOptions,
    cancel: &CancellationToken,
) -> CoreResult<Vec<PackFile>> {
    let mut out = Vec::new();
    for input in inputs {
        cancel.check()?;
        let path = input.canonicalize().map_err(|e| {
            CoreError::Io(std::io::Error::new(
                e.kind(),
                format!("{}: {e}", input.display()),
            ))
        })?;
        if path.is_file() {
            if opts.should_exclude_file(&path) {
                continue;
            }
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".into());
            out.push(PackFile { source: path, name });
            continue;
        }
        if !path.is_dir() {
            return Err(CoreError::Other(format!(
                "not a file or directory: {}",
                path.display()
            )));
        }
        let root = path.clone();
        for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
            cancel.check()?;
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            if opts.should_exclude_file(p) {
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .map_err(|e| CoreError::Other(e.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(PackFile {
                source: p.to_path_buf(),
                name: rel,
            });
        }
    }
    if out.is_empty() {
        return Err(CoreError::Other("no files to compress".into()));
    }
    Ok(out)
}

pub fn ensure_parent(path: &Path) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

/// Stage inputs into a temporary directory preserving relative names. Returns (temp_dir, root_to_compress).
pub fn stage_inputs(
    inputs: &[&Path],
    cancel: &CancellationToken,
) -> CoreResult<(tempfile::TempDir, PathBuf)> {
    let tmp = tempfile::TempDir::new().map_err(CoreError::Io)?;
    let root = tmp.path().join("payload");
    fs::create_dir_all(&root)?;
    let files = collect_pack_files(inputs, cancel)?;
    for file in files {
        cancel.check()?;
        let dest = root.join(&file.name);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&file.source, &dest)?;
    }
    Ok((tmp, root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ConflictPolicy;
    use tempfile::tempdir;

    #[test]
    fn auto_rename_adds_numeric_suffix() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("photo.png");
        fs::write(&path, b"a").unwrap();
        let renamed = unique_extract_path(&path);
        assert_eq!(renamed, dir.path().join("photo (1).png"));
        fs::write(&renamed, b"b").unwrap();
        let again = resolve_conflict_path(path.clone(), ConflictPolicy::AutoRename).unwrap();
        assert_eq!(again, dir.path().join("photo (2).png"));
        assert!(resolve_conflict_path(path, ConflictPolicy::Skip).is_none());
    }
}
