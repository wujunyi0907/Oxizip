//! Microsoft Cabinet (CAB) reader via the `cab` crate.

use std::fs::{self, File};
use std::io::copy;
use std::path::PathBuf;

use oxizip_core::{
    safe_dest_path, CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate,
};

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

pub struct CabReader {
    path: PathBuf,
}

impl CabReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let file = File::open(&path)?;
        let _ =
            cab::Cabinet::new(file).map_err(|e| CoreError::Other(format!("invalid cab: {e}")))?;
        Ok(Self { path })
    }

    fn cabinet(&self) -> CoreResult<cab::Cabinet<File>> {
        let file = File::open(&self.path)?;
        cab::Cabinet::new(file).map_err(|e| CoreError::Other(format!("invalid cab: {e}")))
    }
}

impl ArchiveReader for CabReader {
    fn format(&self) -> FormatId {
        FormatId::Cab
    }

    fn list(
        &mut self,
        _opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        let cabinet = self.cabinet()?;
        let mut out = Vec::new();
        let mut done = 0u64;
        for folder in cabinet.folder_entries() {
            let method = format!("{:?}", folder.compression_type());
            for file in folder.file_entries() {
                cancel.check()?;
                done += 1;
                let name = file.name().replace('\\', "/");
                progress.report(ProgressUpdate {
                    stage: JobStage::List,
                    bytes_done: 0,
                    bytes_total: None,
                    entries_done: done,
                    entries_total: None,
                    current_entry: Some(name.clone()),
                    message: None,
                });
                out.push(EntryMeta {
                    name,
                    is_dir: false,
                    uncompressed_size: Some(u64::from(file.uncompressed_size())),
                    compressed_size: None,
                    modified: None,
                    method: Some(method.clone()),
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
        let mut cabinet = self.cabinet()?;

        let names: Vec<String> = {
            let mut v = Vec::new();
            for folder in cabinet.folder_entries() {
                for file in folder.file_entries() {
                    v.push(file.name().replace('\\', "/"));
                }
            }
            v
        };
        let total = names.len() as u64;
        let filter = opts.entries.as_ref();

        for (i, name) in names.iter().enumerate() {
            cancel.check()?;
            if let Some(entries) = filter {
                if !entries
                    .iter()
                    .any(|e| e == name || e.trim_end_matches('/') == name.trim_end_matches('/'))
                {
                    continue;
                }
            }

            progress.report(ProgressUpdate {
                stage: JobStage::Extract,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(name.clone()),
                message: None,
            });

            let out_path = safe_dest_path(&dest_root, name)?;
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            if out_path.exists() && !opts.overwrite {
                continue;
            }

            let win_name = name.replace('/', "\\");
            let mut reader = match cabinet.read_file(name) {
                Ok(r) => r,
                Err(_) => cabinet
                    .read_file(&win_name)
                    .map_err(|e| CoreError::Other(format!("cab read {name}: {e}")))?,
            };
            let mut outfile = File::create(&out_path)?;
            copy(&mut reader, &mut outfile)?;
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
        let entries = self.list(opts, progress, cancel)?;
        let mut cabinet = self.cabinet()?;
        let total = entries.len() as u64;
        for (i, entry) in entries.iter().enumerate() {
            cancel.check()?;
            progress.report(ProgressUpdate {
                stage: JobStage::Test,
                bytes_done: 0,
                bytes_total: entry.uncompressed_size,
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(entry.name.clone()),
                message: None,
            });
            let win_name = entry.name.replace('/', "\\");
            let mut reader = match cabinet.read_file(&entry.name) {
                Ok(r) => r,
                Err(_) => cabinet
                    .read_file(&win_name)
                    .map_err(|e| CoreError::Other(format!("cab test {}: {e}", entry.name)))?,
            };
            std::io::copy(&mut reader, &mut std::io::sink())?;
        }
        Ok(())
    }
}
