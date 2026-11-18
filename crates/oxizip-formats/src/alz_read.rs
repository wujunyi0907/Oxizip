//! ALZ archive reader via the MIT `unalz` crate.

use std::path::PathBuf;

use oxizip_core::{
    safe_dest_path, CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate,
};
use unalz::archive::AlzArchive;
use unalz::extract::extract_entry;

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

pub struct AlzReader {
    path: PathBuf,
}

impl AlzReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let path_str = path
            .to_str()
            .ok_or_else(|| CoreError::Other("alz path is not valid UTF-8".into()))?;
        let _ = AlzArchive::open(path_str)
            .map_err(|e| CoreError::Other(format!("invalid alz: {e}")))?;
        Ok(Self { path })
    }

    fn archive(&self) -> CoreResult<AlzArchive> {
        let path_str = self
            .path
            .to_str()
            .ok_or_else(|| CoreError::Other("alz path is not valid UTF-8".into()))?;
        AlzArchive::open(path_str).map_err(|e| CoreError::Other(format!("invalid alz: {e}")))
    }
}

fn map_alz_err(e: unalz::error::AlzError) -> CoreError {
    use unalz::error::AlzError;
    match e {
        AlzError::PasswordNotSet => CoreError::PasswordRequired,
        AlzError::InvalidPassword => CoreError::InvalidPassword,
        AlzError::PathTraversal(p) => CoreError::PathUnsafe(p),
        other => CoreError::Other(other.to_string()),
    }
}

impl ArchiveReader for AlzReader {
    fn format(&self) -> FormatId {
        FormatId::Alz
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
                compressed_size: Some(entry.compressed_size),
                modified: None,
                method: Some(entry.compression_method.to_string()),
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
            // Enforce oxizip-core zip-slip before delegating to unalz.
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
                .map_err(map_alz_err)?;
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
