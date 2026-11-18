//! 7z editing via extract → mutate staging tree → recreate (metadata-lossy).

use std::fs;
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, NullProgress, Progress};
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::create::SevenZWriter;
use crate::sevenz_read::SevenZReader;
use crate::traits::{ArchiveReader, ArchiveWriter};
use crate::types::{CreateOptions, ExtractOptions, ListOptions};

fn norm_name(name: &str) -> String {
    name.replace('\\', "/").trim_matches('/').to_string()
}

fn staging_rel(path: &Path, root: &Path) -> CoreResult<String> {
    Ok(path
        .strip_prefix(root)
        .map_err(|e| CoreError::Other(e.to_string()))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn extract_all_to(
    archive: &Path,
    dest: &Path,
    password: Option<&str>,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    let mut reader = SevenZReader::open(archive)?;
    let mut opts = ExtractOptions::to_dir(dest);
    opts.password = password.map(str::to_string);
    reader.extract(&opts, progress, cancel)
}

fn recreate_from_dir(
    archive: &Path,
    root: &Path,
    opts: &CreateOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    // Write to a sibling temp then replace, so a failed recreate keeps the original.
    let staged = archive.with_extension("7z.oxizip-edit");
    let mut writer = Box::new(SevenZWriter::create(&staged)?);
    writer.add_path(root, opts, progress, cancel)?;
    writer.finish(progress, cancel)?;
    if archive.exists() {
        fs::remove_file(archive)?;
    }
    fs::rename(&staged, archive)?;
    Ok(())
}

/// Delete entries from a 7z archive (exact relative paths).
pub fn sevenz_delete_entries(
    path: &Path,
    entries: &[String],
    password: Option<&str>,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    if entries.is_empty() {
        return Ok(());
    }
    let remove: std::collections::HashSet<String> =
        entries.iter().map(|s| norm_name(s)).collect();

    let mut reader = SevenZReader::open(path)?;
    let listed = reader.list(
        &ListOptions {
            password: password.map(str::to_string),
        },
        &NullProgress,
        cancel,
    )?;
    let any = listed.iter().any(|e| remove.contains(&norm_name(&e.name)));
    if !any {
        return Err(CoreError::EntryNotFound(entries.join(", ")));
    }

    let tmp = TempDir::new().map_err(CoreError::Io)?;
    extract_all_to(path, tmp.path(), password, progress, cancel)?;

    for entry in WalkDir::new(tmp.path()).into_iter().filter_map(|e| e.ok()) {
        cancel.check()?;
        let p = entry.path();
        if p == tmp.path() {
            continue;
        }
        let rel = staging_rel(p, tmp.path())?;
        if remove.contains(&norm_name(&rel)) {
            if p.is_dir() {
                let _ = fs::remove_dir_all(p);
            } else {
                let _ = fs::remove_file(p);
            }
        }
    }

    let mut opts = CreateOptions::default();
    opts.password = password.map(str::to_string);
    recreate_from_dir(path, tmp.path(), &opts, progress, cancel)
}

/// Add files/folders into an existing 7z archive.
pub fn sevenz_add_paths(
    path: &Path,
    inputs: &[PathBuf],
    opts: &CreateOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    if inputs.is_empty() {
        return Err(CoreError::Other("add requires at least one input".into()));
    }
    let tmp = TempDir::new().map_err(CoreError::Io)?;
    extract_all_to(
        path,
        tmp.path(),
        opts.password.as_deref(),
        progress,
        cancel,
    )?;

    for input in inputs {
        cancel.check()?;
        let input = input.canonicalize().map_err(CoreError::Io)?;
        let name = input
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "item".into());
        let dest = tmp.path().join(&name);
        if input.is_dir() {
            copy_dir_recursive(&input, &dest, cancel)?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&input, &dest)?;
        }
    }

    recreate_from_dir(path, tmp.path(), opts, progress, cancel)
}

/// Rename one 7z entry path inside the archive.
pub fn sevenz_rename_entry(
    path: &Path,
    from: &str,
    to: &str,
    password: Option<&str>,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    let from_n = norm_name(from);
    let to_n = norm_name(to);
    if from_n.is_empty() || to_n.is_empty() {
        return Err(CoreError::Other("rename requires non-empty names".into()));
    }
    if from_n == to_n {
        return Ok(());
    }

    let tmp = TempDir::new().map_err(CoreError::Io)?;
    extract_all_to(path, tmp.path(), password, progress, cancel)?;
    let src = tmp.path().join(&from_n);
    if !src.exists() {
        return Err(CoreError::EntryNotFound(from.to_string()));
    }
    let dest = tmp.path().join(&to_n);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&src, &dest)?;

    let mut opts = CreateOptions::default();
    opts.password = password.map(str::to_string);
    recreate_from_dir(path, tmp.path(), &opts, progress, cancel)
}

fn copy_dir_recursive(src: &Path, dest: &Path, cancel: &CancellationToken) -> CoreResult<()> {
    fs::create_dir_all(dest)?;
    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        cancel.check()?;
        let p = entry.path();
        let rel = p
            .strip_prefix(src)
            .map_err(|e| CoreError::Other(e.to_string()))?;
        let out = dest.join(rel);
        if p.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(p, &out)?;
        }
    }
    Ok(())
}
