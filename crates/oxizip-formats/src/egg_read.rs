//! EGG archive reader via the MIT `unegg` crate.

use std::fs::File;
use std::path::PathBuf;

use oxizip_core::{
    safe_dest_path, CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate,
};
use unegg::archive::EggArchive;
use unegg::extract::extract_entry;

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

pub struct EggReader {
    path: PathBuf,
}

impl EggReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let file = File::open(&path)?;
        let _ = EggArchive::open(file).map_err(|e| CoreError::Other(format!("invalid egg: {e}")))?;
        Ok(Self { path })
    }

    fn archive(&self) -> CoreResult<EggArchive<File>> {
        let file = File::open(&self.path)?;
        EggArchive::open(file).map_err(|e| CoreError::Other(format!("invalid egg: {e}")))
    }
}

fn map_egg_err(e: unegg::error::EggError) -> CoreError {
    use unegg::error::EggError;
    match e {
        EggError::PasswordNotSet => CoreError::PasswordRequired,
        EggError::InvalidPassword | EggError::AuthenticationFailed => CoreError::InvalidPassword,
        EggError::PathTraversal(p) => CoreError::PathUnsafe(p),
        EggError::FileNotFound(name) => CoreError::EntryNotFound(name),
        other => CoreError::Other(other.to_string()),
    }
}

impl ArchiveReader for EggReader {
    fn format(&self) -> FormatId {
        FormatId::Egg
    }

    fn list(
        &mut self,
        _opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        let archive = self.archive()?;
        let total = archive.entries.len() as u64;
        let mut out = Vec::with_capacity(archive.entries.len());
        for (i, entry) in archive.entries.iter().enumerate() {
            cancel.check()?;
            let name = entry.file_name.replace('\\', "/");
            let method = entry
                .blocks
                .first()
                .map(|b| b.compression_method.to_string());
            progress.report(ProgressUpdate {
                stage: JobStage::List,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(name.clone()),
                message: None,
            });
            out.push(EntryMeta {
                name,
                is_dir: entry.is_directory(),
                uncompressed_size: Some(entry.uncompressed_size),
                compressed_size: Some(
                    entry
                        .blocks
                        .iter()
                        .map(|b| u64::from(b.compressed_size))
                        .sum(),
                ),
                modified: None,
                method,
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
        std::fs::create_dir_all(&opts.dest_dir)?;
        let dest_root = opts.dest_dir.canonicalize().map_err(CoreError::Io)?;
        let mut archive = self.archive()?;
        let entries = archive.entries.clone();
        let total = entries.len() as u64;
        let filter = opts.entries.as_ref();
        let password = opts.password.as_deref();

        for (i, entry) in entries.iter().enumerate() {
            cancel.check()?;
            let name = entry.file_name.replace('\\', "/");
            if let Some(wanted) = filter {
                if !wanted.iter().any(|e| {
                    e == &name || e.trim_end_matches('/') == name.trim_end_matches('/')
                }) {
                    continue;
                }
            }
            let _ = safe_dest_path(&dest_root, &name)?;
            progress.report(ProgressUpdate {
                stage: JobStage::Extract,
                bytes_done: 0,
                bytes_total: Some(entry.uncompressed_size),
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(name),
                message: None,
            });
            extract_entry(&mut archive, entry, &opts.dest_dir, password, false)
                .map_err(map_egg_err)?;
        }
        Ok(())
    }

    fn test(
        &mut self,
        opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        let tmp = tempfile::tempdir().map_err(CoreError::Io)?;
        let extract_opts = ExtractOptions {
            dest_dir: tmp.path().to_path_buf(),
            password: opts.password.clone(),
            entries: None,
            overwrite: true,
        };
        self.extract(&extract_opts, progress, cancel)
    }
}
