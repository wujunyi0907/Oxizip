//! Self-extracting EXE = `oxizip-sfx-stub` + ZIP payload (ZIP is located from EOF).

use std::fs::File;
use std::io::{copy, BufWriter, Write};
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};
use tempfile::NamedTempFile;

use crate::traits::ArchiveWriter;
use crate::types::{CreateOptions, FormatId};
use crate::util::ensure_parent;
use crate::zip_format::ZipWriterBackend;

pub struct ExeSfxWriter {
    path: PathBuf,
    zip_tmp: NamedTempFile,
    zip: ZipWriterBackend,
}

impl ExeSfxWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        let zip_tmp = NamedTempFile::new().map_err(CoreError::Io)?;
        let zip = ZipWriterBackend::create(zip_tmp.path())?;
        Ok(Self { path, zip_tmp, zip })
    }
}

impl ArchiveWriter for ExeSfxWriter {
    fn format(&self) -> FormatId {
        FormatId::ExeSfx
    }

    fn add_path(
        &mut self,
        path: &Path,
        opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        self.zip.add_path(path, opts, progress, cancel)
    }

    fn finish(
        self: Box<Self>,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        // Finish the inner zip by reconstructing ownership.
        let ExeSfxWriter {
            path,
            zip_tmp,
            zip,
        } = *self;
        Box::new(zip).finish(progress, cancel)?;

        let stub = find_sfx_stub().ok_or_else(|| {
            CoreError::Other(
                "oxizip-sfx-stub not found; build workspace and keep stub beside oxizip"
                    .into(),
            )
        })?;

        progress.report(ProgressUpdate {
            stage: JobStage::Create,
            bytes_done: 0,
            bytes_total: None,
            entries_done: 0,
            entries_total: None,
            current_entry: None,
            message: Some("sfx".into()),
        });

        let mut out = BufWriter::new(File::create(&path)?);
        let mut stub_file = File::open(&stub)?;
        copy(&mut stub_file, &mut out)?;
        let mut zip_file = File::open(zip_tmp.path())?;
        copy(&mut zip_file, &mut out)?;
        out.flush()?;
        // Keep temp until drop
        drop(zip_tmp);
        Ok(())
    }
}

fn find_sfx_stub() -> Option<PathBuf> {
    let names = ["oxizip-sfx-stub.exe", "oxizip-sfx-stub"];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in names {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    // Dev fallback: target/{debug,release}/oxizip-sfx-stub*
    if let Ok(cwd) = std::env::current_dir() {
        for profile in ["debug", "release"] {
            for name in names {
                let p = cwd.join("target").join(profile).join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}
