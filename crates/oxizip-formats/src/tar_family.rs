use std::fs::{self, File};
use std::io::{copy, BufReader, Read};
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use lzma_rs::xz_decompress;
use oxizip_core::{
    safe_dest_path, CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate,
};
use tar::Archive;
use zstd::stream::read::Decoder as ZstdDecoder;

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

pub struct TarReader {
    path: PathBuf,
    format: FormatId,
}

impl TarReader {
    pub fn open(path: impl Into<PathBuf>, format: FormatId) -> Self {
        Self {
            path: path.into(),
            format,
        }
    }

    fn open_raw(&self) -> CoreResult<Box<dyn Read + Send>> {
        let file = File::open(&self.path)?;
        let reader: Box<dyn Read + Send> = match self.format {
            FormatId::Tar => Box::new(BufReader::new(file)),
            FormatId::TarGz | FormatId::Gz => Box::new(GzDecoder::new(BufReader::new(file))),
            FormatId::TarBz2 | FormatId::Bz2 => Box::new(BzDecoder::new(BufReader::new(file))),
            FormatId::TarZst | FormatId::Zst => {
                let dec = ZstdDecoder::new(BufReader::new(file))
                    .map_err(|e| CoreError::Other(format!("zstd: {e}")))?;
                Box::new(dec)
            }
            FormatId::TarXz | FormatId::Xz => {
                let mut compressed = BufReader::new(file);
                let mut decompressed = Vec::new();
                xz_decompress(&mut compressed, &mut decompressed)
                    .map_err(|e| CoreError::Other(format!("xz: {e}")))?;
                Box::new(std::io::Cursor::new(decompressed))
            }
            other => {
                return Err(CoreError::Other(format!(
                    "tar_family cannot open {other:?}"
                )))
            }
        };
        Ok(reader)
    }
}

impl ArchiveReader for TarReader {
    fn format(&self) -> FormatId {
        self.format
    }

    fn list(
        &mut self,
        _opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        match self.format {
            FormatId::Gz | FormatId::Xz | FormatId::Zst | FormatId::Bz2 => {
                let name = self
                    .path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "data".into());
                progress.report(ProgressUpdate {
                    stage: JobStage::List,
                    bytes_done: 0,
                    bytes_total: None,
                    entries_done: 1,
                    entries_total: Some(1),
                    current_entry: Some(name.clone()),
                    message: None,
                });
                return Ok(vec![EntryMeta {
                    name,
                    is_dir: false,
                    uncompressed_size: None,
                    compressed_size: None,
                    modified: None,
                    method: Some(self.format.as_str().into()),
                }]);
            }
            _ => {}
        }

        let reader = self.open_raw()?;
        let mut archive = Archive::new(reader);
        let mut out = Vec::new();
        for (idx, entry) in archive
            .entries()
            .map_err(|e| CoreError::Other(format!("tar: {e}")))?
            .enumerate()
        {
            cancel.check()?;
            let entry = entry.map_err(|e| CoreError::Other(format!("tar entry: {e}")))?;
            let path = entry
                .path()
                .map_err(|e| CoreError::Other(format!("tar path: {e}")))?;
            let name = path.to_string_lossy().replace('\\', "/");
            out.push(EntryMeta {
                name: name.clone(),
                is_dir: entry.header().entry_type().is_dir(),
                uncompressed_size: entry.header().size().ok(),
                compressed_size: None,
                modified: entry
                    .header()
                    .mtime()
                    .ok()
                    .map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_secs(t)),
                method: Some("tar".into()),
            });
            progress.report(ProgressUpdate {
                stage: JobStage::List,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (idx as u64) + 1,
                entries_total: None,
                current_entry: Some(name),
                message: None,
            });
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

        match self.format {
            FormatId::Gz | FormatId::Xz | FormatId::Zst | FormatId::Bz2 => {
                return extract_single(self, opts, progress, cancel);
            }
            _ => {}
        }

        let reader = self.open_raw()?;
        let mut archive = Archive::new(reader);
        for (idx, entry) in archive
            .entries()
            .map_err(|e| CoreError::Other(format!("tar: {e}")))?
            .enumerate()
        {
            cancel.check()?;
            let mut entry = entry.map_err(|e| CoreError::Other(format!("tar entry: {e}")))?;
            let path = entry
                .path()
                .map_err(|e| CoreError::Other(format!("tar path: {e}")))?;
            let rel = path.to_string_lossy().replace('\\', "/");

            if let Some(entries) = opts.entries.as_ref() {
                if !entries.iter().any(|e| e == &rel) {
                    continue;
                }
            }

            let out_path = safe_dest_path(&opts.dest_dir, &rel)?;
            progress.report(ProgressUpdate {
                stage: JobStage::Extract,
                bytes_done: 0,
                bytes_total: entry.header().size().ok(),
                entries_done: (idx as u64) + 1,
                entries_total: None,
                current_entry: Some(rel.clone()),
                message: None,
            });

            if entry.header().entry_type().is_dir() {
                fs::create_dir_all(&out_path)?;
                continue;
            }
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let Some(out_path) =
                crate::util::resolve_conflict_path(out_path, opts.conflict)
            else {
                continue;
            };
            let mut outfile = File::create(&out_path)?;
            copy(&mut entry, &mut outfile)?;
        }
        Ok(())
    }
}

fn extract_single(
    reader: &TarReader,
    opts: &ExtractOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    let name = reader
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "data".into());
    // Strip remaining compression suffix from stem if present (e.g. file.tar from file.tar.gz handled by extension sniff as TarGz).
    let out_path = safe_dest_path(&opts.dest_dir, &name)?;
    let Some(out_path) = crate::util::resolve_conflict_path(out_path, opts.conflict) else {
        return Ok(());
    };
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    progress.report(ProgressUpdate {
        stage: JobStage::Extract,
        bytes_done: 0,
        bytes_total: None,
        entries_done: 1,
        entries_total: Some(1),
        current_entry: Some(name),
        message: None,
    });
    let mut input = reader.open_raw()?;
    let mut outfile = File::create(&out_path)?;
    copy(&mut input, &mut outfile)?;
    Ok(())
}

/// Helper used by tests / engine when building from path.
pub fn is_tar_family(format: FormatId) -> bool {
    matches!(
        format,
        FormatId::Tar
            | FormatId::TarGz
            | FormatId::TarXz
            | FormatId::TarZst
            | FormatId::TarBz2
            | FormatId::Gz
            | FormatId::Xz
            | FormatId::Zst
            | FormatId::Bz2
    )
}

pub fn open_tar_family(path: &Path, format: FormatId) -> TarReader {
    TarReader::open(path, format)
}
