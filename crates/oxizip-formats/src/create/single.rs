use std::fs::File;
use std::io::{copy, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use lzma_rs::xz_compress;
use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};

use crate::traits::ArchiveWriter;
use crate::types::{CreateOptions, FormatId};
use crate::util::ensure_parent;

pub struct GzWriter {
    path: PathBuf,
    source: Option<PathBuf>,
}

impl GzWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Self { path, source: None })
    }
}

impl ArchiveWriter for GzWriter {
    fn format(&self) -> FormatId {
        FormatId::Gz
    }

    fn add_path(
        &mut self,
        path: &Path,
        _opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        if self.source.is_some() {
            return Err(CoreError::Other(
                "gz format accepts exactly one input file".into(),
            ));
        }
        let path = path.canonicalize().map_err(CoreError::Io)?;
        if !path.is_file() {
            return Err(CoreError::Other("gz format requires a single file".into()));
        }
        progress.report(ProgressUpdate {
            stage: JobStage::Create,
            bytes_done: 0,
            bytes_total: None,
            entries_done: 1,
            entries_total: Some(1),
            current_entry: Some(path.display().to_string()),
            message: None,
        });
        self.source = Some(path);
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        _progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        let source = self
            .source
            .ok_or_else(|| CoreError::Other("no input for gz".into()))?;
        let mut input = BufReader::new(File::open(source)?);
        let mut enc = GzEncoder::new(
            BufWriter::new(File::create(&self.path)?),
            Compression::default(),
        );
        copy(&mut input, &mut enc)?;
        enc.finish()?;
        Ok(())
    }
}

pub struct XzWriter {
    path: PathBuf,
    source: Option<PathBuf>,
}

impl XzWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Self { path, source: None })
    }
}

impl ArchiveWriter for XzWriter {
    fn format(&self) -> FormatId {
        FormatId::Xz
    }

    fn add_path(
        &mut self,
        path: &Path,
        _opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        if self.source.is_some() {
            return Err(CoreError::Other(
                "xz format accepts exactly one input file".into(),
            ));
        }
        let path = path.canonicalize().map_err(CoreError::Io)?;
        if !path.is_file() {
            return Err(CoreError::Other("xz format requires a single file".into()));
        }
        progress.report(ProgressUpdate {
            stage: JobStage::Create,
            bytes_done: 0,
            bytes_total: None,
            entries_done: 1,
            entries_total: Some(1),
            current_entry: Some(path.display().to_string()),
            message: None,
        });
        self.source = Some(path);
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        _progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        let source = self
            .source
            .ok_or_else(|| CoreError::Other("no input for xz".into()))?;
        let mut input = BufReader::new(File::open(source)?);
        let mut output = BufWriter::new(File::create(&self.path)?);
        xz_compress(&mut input, &mut output)
            .map_err(|e| CoreError::Other(format!("xz compress: {e}")))?;
        Ok(())
    }
}
