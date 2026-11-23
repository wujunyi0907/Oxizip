use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};
use tar::Builder;

use crate::traits::ArchiveWriter;
use crate::types::{CreateOptions, FormatId};
use crate::util::{collect_pack_files_with_opts, ensure_parent};

pub struct TarWriter {
    path: PathBuf,
    files: Vec<(PathBuf, String)>,
}

impl TarWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Self {
            path,
            files: Vec::new(),
        })
    }
}

impl ArchiveWriter for TarWriter {
    fn format(&self) -> FormatId {
        FormatId::Tar
    }

    fn add_path(
        &mut self,
        path: &Path,
        opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        let packed = collect_pack_files_with_opts(&[path], opts, cancel)?;
        for f in packed {
            progress.report(ProgressUpdate {
                stage: JobStage::Create,
                bytes_done: 0,
                bytes_total: None,
                entries_done: 0,
                entries_total: None,
                current_entry: Some(f.name.clone()),
                message: None,
            });
            self.files.push((f.source, f.name));
        }
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        _progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        let file = File::create(&self.path)?;
        let mut builder = Builder::new(BufWriter::new(file));
        for (source, name) in &self.files {
            cancel.check()?;
            builder
                .append_path_with_name(source, name)
                .map_err(|e| CoreError::Other(format!("tar append: {e}")))?;
        }
        builder
            .finish()
            .map_err(|e| CoreError::Other(format!("tar finish: {e}")))?;
        Ok(())
    }
}

pub struct TarGzWriter {
    path: PathBuf,
    files: Vec<(PathBuf, String)>,
}

impl TarGzWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Self {
            path,
            files: Vec::new(),
        })
    }
}

impl ArchiveWriter for TarGzWriter {
    fn format(&self) -> FormatId {
        FormatId::TarGz
    }

    fn add_path(
        &mut self,
        path: &Path,
        opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        let packed = collect_pack_files_with_opts(&[path], opts, cancel)?;
        for f in packed {
            progress.report(ProgressUpdate {
                stage: JobStage::Create,
                bytes_done: 0,
                bytes_total: None,
                entries_done: 0,
                entries_total: None,
                current_entry: Some(f.name.clone()),
                message: None,
            });
            self.files.push((f.source, f.name));
        }
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        _progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        let file = File::create(&self.path)?;
        let enc = GzEncoder::new(BufWriter::new(file), Compression::default());
        let mut builder = Builder::new(enc);
        for (source, name) in &self.files {
            cancel.check()?;
            builder
                .append_path_with_name(source, name)
                .map_err(|e| CoreError::Other(format!("tgz append: {e}")))?;
        }
        let enc = builder
            .into_inner()
            .map_err(|e| CoreError::Other(format!("tgz: {e}")))?;
        enc.finish()
            .map_err(|e| CoreError::Other(format!("tgz finish: {e}")))?
            .flush()
            .map_err(CoreError::Io)?;
        Ok(())
    }
}
