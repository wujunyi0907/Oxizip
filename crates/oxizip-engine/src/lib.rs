//! High-level archive operations shared by CLI and GUI.

use std::path::{Path, PathBuf};

use oxizip_formats::{
    is_volume_path, join_volumes, maybe_split_archive, open_reader, open_writer, sniff_path,
};
use tempfile::NamedTempFile;

pub use oxizip_core::{
    CancellationToken, CoreError, CoreResult, ErrorCode, FnProgress, JobStage, NullProgress,
    Progress, ProgressUpdate,
};
pub use oxizip_formats::{
    CreateOptions, EntryMeta, ExtractOptions, FormatId, ListOptions, MethodId, ZipCryptoKind,
};

struct ResolvedArchive {
    path: PathBuf,
    _joined: Option<NamedTempFile>,
}

fn resolve_archive(path: &Path) -> CoreResult<ResolvedArchive> {
    if is_volume_path(path) {
        let joined = join_volumes(path)?;
        let p = joined.path().to_path_buf();
        Ok(ResolvedArchive {
            path: p,
            _joined: Some(joined),
        })
    } else {
        Ok(ResolvedArchive {
            path: path.to_path_buf(),
            _joined: None,
        })
    }
}

pub fn detect(path: &Path) -> CoreResult<FormatId> {
    let probe = if is_volume_path(path) {
        oxizip_formats::first_volume_path(path)
    } else {
        path.to_path_buf()
    };
    sniff_path(&probe)
}

pub fn list(
    path: &Path,
    opts: &ListOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<Vec<EntryMeta>> {
    cancel.check()?;
    let resolved = resolve_archive(path)?;
    let format = sniff_path(&resolved.path)?;
    let mut reader = open_reader(&resolved.path, format)?;
    reader.list(opts, progress, cancel)
}

pub fn extract(
    path: &Path,
    opts: &ExtractOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    let resolved = resolve_archive(path)?;
    let format = sniff_path(&resolved.path)?;
    let mut reader = open_reader(&resolved.path, format)?;
    reader.extract(opts, progress, cancel)
}

pub fn test_archive(
    path: &Path,
    opts: &ListOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    let resolved = resolve_archive(path)?;
    let format = sniff_path(&resolved.path)?;
    let mut reader = open_reader(&resolved.path, format)?;
    reader.test(opts, progress, cancel)
}

pub fn create(
    path: &Path,
    format: FormatId,
    inputs: &[impl AsRef<Path>],
    opts: &CreateOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    if !format.supports_create() {
        return Err(CoreError::UnsupportedFormat);
    }
    let mut writer = open_writer(path, format)?;
    for input in inputs {
        writer.add_path(input.as_ref(), opts, progress, cancel)?;
    }
    writer.finish(progress, cancel)?;
    maybe_split_archive(path, opts.volume_size)?;
    Ok(())
}

/// Human-readable list of createable formats for CLI help.
pub fn create_format_names() -> Vec<&'static str> {
    FormatId::create_choices()
        .iter()
        .map(|f| f.as_str())
        .collect()
}
