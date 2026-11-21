//! High-level archive operations shared by CLI and GUI.

use std::path::{Path, PathBuf};

use oxizip_formats::{
    is_volume_path, join_volumes, maybe_split_archive, open_reader, open_writer, recover_password,
    repair_sevenz, repair_zip, sevenz_add_paths, sevenz_delete_entries, sevenz_rename_entry,
    sniff_path, zip_add_paths, zip_delete_entries, zip_read_comment, zip_rename_entry,
    zip_write_comment,
};
use tempfile::NamedTempFile;

pub use oxizip_core::{
    checksum_file, checksum_paths, propagate_zone_identifier, CancellationToken, CoreError,
    CoreResult, ErrorCode, FileChecksum, FnProgress, JobStage, NullProgress, Progress,
    ProgressUpdate,
};
pub use oxizip_formats::{
    ConflictPolicy, CreateOptions, EntryMeta, ExtractOptions, FormatId, ListOptions, MethodId,
    RecoverPasswordOptions as PasswordRecoverOptions, ZipCryptoKind,
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
    reader.extract(opts, progress, cancel)?;
    // Mark-of-the-Web: copy the archive's Zone.Identifier onto extracted files
    // (Windows NTFS ADS). No-op on other platforms / when ADS is absent.
    let zone_src = if resolved.path != path {
        path
    } else {
        &resolved.path
    };
    propagate_zone_identifier(zone_src, &opts.dest_dir)?;
    Ok(())
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
    let total = inputs.len() as u64;
    for (i, input) in inputs.iter().enumerate() {
        cancel.check()?;
        let name = input.as_ref().display().to_string();
        progress.report(ProgressUpdate {
            stage: JobStage::Create,
            bytes_done: 0,
            bytes_total: None,
            entries_done: i as u64,
            entries_total: Some(total.max(1)),
            current_entry: Some(name),
            message: None,
        });
        writer.add_path(input.as_ref(), opts, progress, cancel)?;
    }
    progress.report(ProgressUpdate {
        stage: JobStage::Create,
        bytes_done: 0,
        bytes_total: None,
        entries_done: total,
        entries_total: Some(total.max(1)),
        current_entry: None,
        message: None,
    });
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

fn ensure_editable(path: &Path) -> CoreResult<FormatId> {
    if is_volume_path(path) {
        return Err(CoreError::Other(
            "cannot edit multi-volume archives in-place".into(),
        ));
    }
    let format = sniff_path(path)?;
    match format {
        FormatId::Zip | FormatId::Zipx | FormatId::SevenZ => Ok(format),
        FormatId::ExeSfx => Err(CoreError::Other(
            "cannot edit self-extracting (.exe) archives".into(),
        )),
        _ => Err(CoreError::UnsupportedFormat),
    }
}

/// Append files/folders to an existing ZIP/ZIPX/7z archive.
pub fn add_to_archive(
    path: &Path,
    inputs: &[impl AsRef<Path>],
    opts: &CreateOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    let format = ensure_editable(path)?;
    let inputs: Vec<PathBuf> = inputs.iter().map(|p| p.as_ref().to_path_buf()).collect();
    match format {
        FormatId::Zip | FormatId::Zipx => zip_add_paths(path, &inputs, opts, progress, cancel),
        FormatId::SevenZ => sevenz_add_paths(path, &inputs, opts, progress, cancel),
        _ => Err(CoreError::UnsupportedFormat),
    }
}

/// Delete entries from an existing ZIP/ZIPX/7z archive.
pub fn delete_entries(
    path: &Path,
    entries: &[String],
    password: Option<&str>,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    let format = ensure_editable(path)?;
    match format {
        FormatId::Zip | FormatId::Zipx => {
            zip_delete_entries(path, entries, password, progress, cancel)
        }
        FormatId::SevenZ => sevenz_delete_entries(path, entries, password, progress, cancel),
        _ => Err(CoreError::UnsupportedFormat),
    }
}

/// Rename one entry inside an existing ZIP/ZIPX/7z archive.
pub fn rename_entry(
    path: &Path,
    from: &str,
    to: &str,
    password: Option<&str>,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    let format = ensure_editable(path)?;
    match format {
        FormatId::Zip | FormatId::Zipx => {
            zip_rename_entry(path, from, to, password, progress, cancel)
        }
        FormatId::SevenZ => sevenz_rename_entry(path, from, to, password, progress, cancel),
        _ => Err(CoreError::UnsupportedFormat),
    }
}

/// Repair a damaged ZIP or 7z archive into a new output path.
pub fn repair_archive(
    path: &Path,
    dest: Option<&Path>,
    password: Option<&str>,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<PathBuf> {
    cancel.check()?;
    let resolved = resolve_archive(path)?;
    let format = sniff_path(&resolved.path)?;
    match format {
        FormatId::Zip | FormatId::Zipx | FormatId::ExeSfx => {
            repair_zip(&resolved.path, dest, progress, cancel)
        }
        FormatId::SevenZ => repair_sevenz(&resolved.path, dest, password, progress, cancel),
        _ => Err(CoreError::Other("repair supports ZIP/ZIPX/7z only".into())),
    }
}

/// Recover a forgotten ZIP/7z password via wordlist and optional short brute force.
pub fn recover_archive_password(
    path: &Path,
    opts: &PasswordRecoverOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<String> {
    cancel.check()?;
    let resolved = resolve_archive(path)?;
    recover_password(&resolved.path, opts, progress, cancel)
}

/// Read ZIP/ZIPX archive comment (empty string when absent). Other formats → unsupported.
pub fn read_archive_comment(path: &Path) -> CoreResult<String> {
    let resolved = resolve_archive(path)?;
    match sniff_path(&resolved.path)? {
        FormatId::Zip | FormatId::Zipx | FormatId::ExeSfx => zip_read_comment(&resolved.path),
        _ => Err(CoreError::UnsupportedFormat),
    }
}

/// Write ZIP/ZIPX archive comment. Other formats → unsupported.
pub fn write_archive_comment(path: &Path, comment: &str) -> CoreResult<()> {
    let resolved = resolve_archive(path)?;
    match sniff_path(&resolved.path)? {
        FormatId::Zip | FormatId::Zipx | FormatId::ExeSfx => {
            zip_write_comment(&resolved.path, comment)
        }
        _ => Err(CoreError::UnsupportedFormat),
    }
}
