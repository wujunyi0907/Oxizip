//! ZIP archive editing via rewrite (add / delete / rename).

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{copy, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};
use walkdir::WalkDir;
use zip::read::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::{AesMode, CompressionMethod};

use crate::types::{CreateOptions, MethodId, ZipCryptoKind};

fn open_archive(path: &Path) -> CoreResult<ZipArchive<BufReader<File>>> {
    let file = File::open(path)?;
    ZipArchive::new(BufReader::new(file))
        .map_err(|e| CoreError::Other(format!("invalid zip: {e}")))
}

fn norm_name(name: &str) -> String {
    name.replace('\\', "/").trim_matches('/').to_string()
}

fn names_match(entry: &str, wanted: &str) -> bool {
    norm_name(entry) == norm_name(wanted)
}

fn staged_path(dest: &Path) -> PathBuf {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let name = dest
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive.zip".into());
    parent.join(format!(".{name}.oxizip-edit"))
}

fn replace_staged(staged: &Path, dest: &Path) -> CoreResult<()> {
    if dest.exists() {
        fs::remove_file(dest)?;
    }
    fs::rename(staged, dest)?;
    Ok(())
}

fn copy_entry_to_writer(
    archive: &mut ZipArchive<BufReader<File>>,
    index: usize,
    out_name: &str,
    password: Option<&[u8]>,
    writer: &mut ZipWriter<BufWriter<File>>,
) -> CoreResult<()> {
    let mut file = match password {
        Some(pw) => archive.by_index_decrypt(index, pw),
        None => archive.by_index(index),
    }
    .map_err(|e| CoreError::Other(format!("zip entry: {e}")))?;

    let method = file.compression();
    let mut options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(file.unix_mode().unwrap_or(0o644));
    if let Some(mtime) = file.last_modified() {
        options = options.last_modified_time(mtime);
    }

    if file.is_dir() || out_name.ends_with('/') {
        let dir_name = if out_name.ends_with('/') {
            out_name.to_string()
        } else {
            format!("{out_name}/")
        };
        writer
            .add_directory(&dir_name, options)
            .map_err(|e| CoreError::Other(format!("zip add_directory: {e}")))?;
        return Ok(());
    }

    writer
        .start_file(out_name, options)
        .map_err(|e| CoreError::Other(format!("zip start_file: {e}")))?;
    copy(&mut file, writer)?;
    Ok(())
}

fn resolve_method(opts: &CreateOptions) -> CoreResult<CompressionMethod> {
    match opts.method {
        Some(MethodId::Store) => Ok(CompressionMethod::Stored),
        Some(MethodId::Deflate) | None => Ok(CompressionMethod::Deflated),
        Some(MethodId::Bzip2) => Ok(CompressionMethod::Bzip2),
        Some(MethodId::Lzma) => Ok(CompressionMethod::Lzma),
        Some(MethodId::Xz) => Ok(CompressionMethod::Xz),
        Some(MethodId::Zstd) => Ok(CompressionMethod::Zstd),
        Some(other) => Err(CoreError::Other(format!(
            "method {} is for 7z, not zip",
            other.as_str()
        ))),
    }
}

fn add_file_bytes(
    writer: &mut ZipWriter<BufWriter<File>>,
    path: &Path,
    name: &str,
    opts: &CreateOptions,
    method: CompressionMethod,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    progress.report(ProgressUpdate {
        stage: JobStage::Create,
        bytes_done: 0,
        bytes_total: None,
        entries_done: 0,
        entries_total: None,
        current_entry: Some(name.to_string()),
        message: Some("add".into()),
    });
    let mut options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o644);
    if let Some(level) = opts.level {
        options = options.compression_level(Some(i64::from(level.min(9))));
    }
    match (opts.password.as_deref(), opts.zip_crypto) {
        (Some(_), ZipCryptoKind::ZipCrypto) => {
            return Err(CoreError::Other(
                "legacy ZipCrypto create is not available; use aes*".into(),
            ));
        }
        (Some(password), ZipCryptoKind::Aes256) => writer
            .start_file(name, options.with_aes_encryption(AesMode::Aes256, password))
            .map_err(|e| CoreError::Other(format!("zip start_file: {e}")))?,
        (Some(password), ZipCryptoKind::Aes128) => writer
            .start_file(name, options.with_aes_encryption(AesMode::Aes128, password))
            .map_err(|e| CoreError::Other(format!("zip start_file: {e}")))?,
        (Some(password), ZipCryptoKind::Aes192) => writer
            .start_file(name, options.with_aes_encryption(AesMode::Aes192, password))
            .map_err(|e| CoreError::Other(format!("zip start_file: {e}")))?,
        (None, _) => writer
            .start_file(name, options)
            .map_err(|e| CoreError::Other(format!("zip start_file: {e}")))?,
    }
    let mut input = File::open(path)?;
    copy(&mut input, writer)?;
    Ok(())
}

/// Delete entries (exact name match) from a ZIP archive.
pub fn zip_delete_entries(
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
    let remove: HashSet<String> = entries.iter().map(|s| norm_name(s)).collect();
    let mut archive = open_archive(path)?;
    let total = archive.len() as u64;
    let pw = password.map(str::as_bytes);

    let staged = staged_path(path);
    let mut kept = 0u64;
    {
        let mut writer = ZipWriter::new(BufWriter::new(File::create(&staged)?));
        for i in 0..archive.len() {
            cancel.check()?;
            let name = {
                let file = archive
                    .by_index(i)
                    .map_err(|e| CoreError::Other(format!("zip: {e}")))?;
                file.name().replace('\\', "/")
            };
            progress.report(ProgressUpdate {
                stage: JobStage::Create,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(name.clone()),
                message: Some("delete".into()),
            });
            if remove.contains(&norm_name(&name)) {
                continue;
            }
            copy_entry_to_writer(&mut archive, i, &name, pw, &mut writer)?;
            kept += 1;
        }
        writer
            .finish()
            .map_err(|e| CoreError::Other(format!("zip finish: {e}")))?;
    }
    if kept == total {
        let _ = fs::remove_file(&staged);
        return Err(CoreError::EntryNotFound(entries.join(", ")));
    }
    replace_staged(&staged, path)
}

/// Rename a single ZIP entry.
pub fn zip_rename_entry(
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

    let mut archive = open_archive(path)?;
    let total = archive.len() as u64;
    let pw = password.map(str::as_bytes);
    let mut found = false;
    let staged = staged_path(path);
    {
        let mut writer = ZipWriter::new(BufWriter::new(File::create(&staged)?));
        for i in 0..archive.len() {
            cancel.check()?;
            let name = {
                let file = archive
                    .by_index(i)
                    .map_err(|e| CoreError::Other(format!("zip: {e}")))?;
                file.name().replace('\\', "/")
            };
            progress.report(ProgressUpdate {
                stage: JobStage::Create,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(name.clone()),
                message: Some("rename".into()),
            });
            let out_name = if names_match(&name, &from_n) {
                found = true;
                if name.ends_with('/') && !to_n.ends_with('/') {
                    format!("{to_n}/")
                } else {
                    to_n.clone()
                }
            } else {
                name
            };
            copy_entry_to_writer(&mut archive, i, &out_name, pw, &mut writer)?;
        }
        writer
            .finish()
            .map_err(|e| CoreError::Other(format!("zip finish: {e}")))?;
    }
    if !found {
        let _ = fs::remove_file(&staged);
        return Err(CoreError::EntryNotFound(from.to_string()));
    }
    replace_staged(&staged, path)
}

/// Append files/folders to an existing ZIP (rewrite + new members).
pub fn zip_add_paths(
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
    if !path.is_file() {
        return Err(CoreError::Other(format!(
            "archive not found: {}",
            path.display()
        )));
    }

    let mut archive = open_archive(path)?;
    let total = archive.len() as u64;
    let pw = opts.password.as_deref().map(str::as_bytes);
    let method = resolve_method(opts)?;
    let staged = staged_path(path);
    {
        let mut writer = ZipWriter::new(BufWriter::new(File::create(&staged)?));
        for i in 0..archive.len() {
            cancel.check()?;
            let name = {
                let file = archive
                    .by_index(i)
                    .map_err(|e| CoreError::Other(format!("zip: {e}")))?;
                file.name().replace('\\', "/")
            };
            progress.report(ProgressUpdate {
                stage: JobStage::Create,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(name.clone()),
                message: Some("add-copy".into()),
            });
            copy_entry_to_writer(&mut archive, i, &name, pw, &mut writer)?;
        }

        for input in inputs {
            cancel.check()?;
            let input = input.canonicalize().map_err(CoreError::Io)?;
            if input.is_file() {
                let name = input
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".into());
                add_file_bytes(&mut writer, &input, &name, opts, method, progress, cancel)?;
            } else {
                for entry in WalkDir::new(&input).into_iter().filter_map(|e| e.ok()) {
                    cancel.check()?;
                    let p = entry.path();
                    if p.is_dir() {
                        continue;
                    }
                    let rel = p
                        .strip_prefix(&input)
                        .map_err(|e| CoreError::Other(e.to_string()))?
                        .to_string_lossy()
                        .replace('\\', "/");
                    add_file_bytes(&mut writer, p, &rel, opts, method, progress, cancel)?;
                }
            }
        }

        writer
            .finish()
            .map_err(|e| CoreError::Other(format!("zip finish: {e}")))?;
    }
    replace_staged(&staged, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{ArchiveReader, ArchiveWriter};
    use crate::types::{CreateOptions, ListOptions};
    use crate::zip_format::{ZipReader, ZipWriterBackend};
    use oxizip_core::NullProgress;
    use tempfile::tempdir;

    #[test]
    fn zip_add_delete_rename_roundtrip() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        fs::write(&a, b"aaa").unwrap();
        fs::write(&b, b"bbb").unwrap();
        let cancel = CancellationToken::new();
        let mut w = Box::new(ZipWriterBackend::create(&zip_path).unwrap());
        w.add_path(&a, &CreateOptions::default(), &NullProgress, &cancel)
            .unwrap();
        w.finish(&NullProgress, &cancel).unwrap();

        zip_add_paths(
            &zip_path,
            &[b.clone()],
            &CreateOptions::default(),
            &NullProgress,
            &cancel,
        )
        .unwrap();

        let mut reader = ZipReader::open(&zip_path).unwrap();
        let names: Vec<_> = reader
            .list(&ListOptions::default(), &NullProgress, &cancel)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.iter().any(|n| n == "a.txt"));
        assert!(names.iter().any(|n| n == "b.txt"));

        zip_rename_entry(
            &zip_path,
            "a.txt",
            "alpha.txt",
            None,
            &NullProgress,
            &cancel,
        )
        .unwrap();
        zip_delete_entries(
            &zip_path,
            &["b.txt".into()],
            None,
            &NullProgress,
            &cancel,
        )
        .unwrap();

        let mut reader = ZipReader::open(&zip_path).unwrap();
        let names: Vec<_> = reader
            .list(&ListOptions::default(), &NullProgress, &cancel)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.iter().any(|n| n == "alpha.txt"));
        assert!(!names.iter().any(|n| n == "a.txt"));
        assert!(!names.iter().any(|n| n == "b.txt"));
    }
}
