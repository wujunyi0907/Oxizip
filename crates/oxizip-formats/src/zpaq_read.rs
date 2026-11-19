//! ZPAQ list/extract via embedded official zpaq.

use std::fs;
use std::path::{Path, PathBuf};

use oxizip_core::{
    safe_dest_path, validate_entry_name, CancellationToken, CoreError, CoreResult, JobStage,
    Progress, ProgressUpdate,
};
use tempfile::TempDir;

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};
use crate::zpaq_ffi::{path_arg, run_zpaq, run_zpaq_capture_stdout};

pub struct ZpaqReader {
    path: PathBuf,
}

impl ZpaqReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        Ok(Self { path: path.into() })
    }
}

impl ArchiveReader for ZpaqReader {
    fn format(&self) -> FormatId {
        FormatId::Zpaq
    }

    fn list(
        &mut self,
        _opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        progress.report(ProgressUpdate::new(JobStage::List));
        let archive = path_arg(&self.path)?;
        let stdout = run_zpaq_capture_stdout(&["l", &archive])?;
        let mut out = Vec::new();
        for (i, line) in stdout.lines().enumerate() {
            cancel.check()?;
            if let Some(entry) = parse_zpaq_list_line(line) {
                out.push(entry);
                progress.report(ProgressUpdate {
                    stage: JobStage::List,
                    bytes_done: 0,
                    bytes_total: None,
                    entries_done: (i as u64) + 1,
                    entries_total: None,
                    current_entry: out.last().map(|e| e.name.clone()),
                    message: None,
                });
            }
        }
        Ok(out)
    }

    fn extract(
        &mut self,
        opts: &ExtractOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        fs::create_dir_all(&opts.dest_dir)?;
        let dest_root = opts.dest_dir.canonicalize().map_err(CoreError::Io)?;

        let entries = self.list(&ListOptions::default(), progress, cancel)?;
        for e in &entries {
            let _ = sanitize_zpaq_name(&e.name)?;
        }

        progress.report(ProgressUpdate::new(JobStage::Extract));
        let archive = path_arg(&self.path)?;

        // Extract into a staging dir, then copy only safe relative names into dest.
        let stage = TempDir::new().map_err(CoreError::Io)?;
        let stage_path = path_arg(stage.path())?;

        let filter_owned: Vec<String> = match &opts.entries {
            Some(v) => {
                let mut out = Vec::new();
                for s in v {
                    out.push(sanitize_zpaq_name(s)?);
                }
                out
            }
            None => Vec::new(),
        };
        let mut args = vec![
            "x".to_string(),
            archive,
            "-to".to_string(),
            stage_path,
            "-force".to_string(),
        ];
        args.extend(filter_owned);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_zpaq(&arg_refs)?;

        copy_stage_to_dest(stage.path(), &dest_root, opts.overwrite, progress, cancel)?;
        Ok(())
    }
}

fn copy_stage_to_dest(
    stage: &Path,
    dest_root: &Path,
    overwrite: bool,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    fn walk(
        dir: &Path,
        stage_root: &Path,
        dest_root: &Path,
        overwrite: bool,
        progress: &dyn Progress,
        cancel: &CancellationToken,
        count: &mut u64,
    ) -> CoreResult<()> {
        for entry in fs::read_dir(dir)? {
            cancel.check()?;
            let entry = entry?;
            let path = entry.path();
            let rel = path
                .strip_prefix(stage_root)
                .map_err(|e| CoreError::Other(e.to_string()))?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.is_empty() {
                continue;
            }
            if path.is_dir() {
                walk(
                    &path, stage_root, dest_root, overwrite, progress, cancel, count,
                )?;
                continue;
            }
            let name = sanitize_zpaq_name(&rel_str)?;
            let out = safe_dest_path(dest_root, &name)?;
            if out.exists() && !overwrite {
                continue;
            }
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &out)?;
            *count += 1;
            progress.report(ProgressUpdate {
                stage: JobStage::Extract,
                bytes_done: 0,
                bytes_total: None,
                entries_done: *count,
                entries_total: None,
                current_entry: Some(name),
                message: None,
            });
        }
        Ok(())
    }

    let mut count = 0u64;
    walk(
        stage, stage, dest_root, overwrite, progress, cancel, &mut count,
    )
}

/// Normalize archive member path to a relative safe form.
fn sanitize_zpaq_name(name: &str) -> CoreResult<String> {
    let n = name.replace('\\', "/");
    let trimmed = n.trim_start_matches('/');
    // Drop Windows drive prefix if present (legacy absolute members).
    let stripped = if trimmed.len() >= 2 {
        let b = trimmed.as_bytes();
        if b[1] == b':' {
            let rest = &trimmed[2..];
            rest.trim_start_matches('/')
        } else {
            trimmed
        }
    } else {
        trimmed
    };
    // If still absolute-looking or empty, keep basename only.
    let candidate = if stripped.is_empty()
        || stripped.starts_with('/')
        || validate_entry_name(stripped).is_err()
    {
        Path::new(stripped)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| CoreError::PathUnsafe(format!("unsafe zpaq name: {name}")))?
    } else {
        stripped.to_string()
    };
    validate_entry_name(&candidate)?;
    Ok(candidate)
}

/// Parse one official `zpaq l` file line.
fn parse_zpaq_list_line(line: &str) -> Option<EntryMeta> {
    let b = line.as_bytes();
    if b.len() < 36 {
        return None;
    }
    let mark = b[0] as char;
    if !matches!(mark, '-' | '+' | '=' | '#' | '^') || b[1] != b' ' {
        return None;
    }
    // "%c %s %12.0f " with date width 19 → size field at [22..34]
    let size_str = line.get(22..34)?.trim();
    let size: u64 = size_str.parse().ok()?;
    let after = line.get(35..)?.trim_start();
    // attr is typically 5 chars then space; tolerate variable attr width.
    let name = if after.len() > 6 && after.as_bytes().get(5) == Some(&b' ') {
        after[6..].trim().to_string()
    } else {
        // Fallback: last whitespace-separated token path-ish
        after
            .rsplit_once(' ')
            .map(|(_, n)| n.trim().to_string())
            .unwrap_or_else(|| after.to_string())
    };
    if name.is_empty() || name.contains(" MB ") {
        return None;
    }
    let rel = sanitize_zpaq_name(&name).ok()?;
    let is_dir = rel.ends_with('/');
    Some(EntryMeta {
        name: rel.trim_end_matches('/').to_string(),
        is_dir,
        uncompressed_size: Some(size),
        compressed_size: None,
        modified: None,
        method: Some("zpaq".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::ZpaqWriter;
    use crate::traits::ArchiveWriter;
    use crate::types::CreateOptions;
    use oxizip_core::{CancellationToken, NullProgress};
    use tempfile::tempdir;

    #[test]
    fn zpaq_roundtrip() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("t.zpaq");
        let src = dir.path().join("hello.txt");
        fs::write(&src, b"hello zpaq").unwrap();

        let mut w = Box::new(ZpaqWriter::create(&archive).unwrap());
        let cancel = CancellationToken::new();
        w.add_path(&src, &CreateOptions::default(), &NullProgress, &cancel)
            .unwrap();
        w.finish(&NullProgress, &cancel).unwrap();

        let mut r = ZpaqReader::open(&archive).unwrap();
        let entries = r
            .list(&ListOptions::default(), &NullProgress, &cancel)
            .unwrap();
        assert!(
            entries.iter().any(|e| e.name.ends_with("hello.txt")),
            "entries={entries:?}"
        );

        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        r.extract(&ExtractOptions::to_dir(&out), &NullProgress, &cancel)
            .unwrap();
        assert_eq!(fs::read(out.join("hello.txt")).unwrap(), b"hello zpaq");
    }
}
