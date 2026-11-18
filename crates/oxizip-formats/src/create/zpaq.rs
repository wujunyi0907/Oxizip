//! ZPAQ create via embedded official zpaq (journaling format, relative member names).

use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};

use crate::traits::ArchiveWriter;
use crate::types::{CreateOptions, FormatId};
use crate::util::{ensure_parent, stage_inputs};
use crate::zpaq_ffi::{path_arg, run_zpaq_in_dir};

pub struct ZpaqWriter {
    path: PathBuf,
    inputs: Vec<PathBuf>,
}

impl ZpaqWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Self {
            path,
            inputs: Vec::new(),
        })
    }
}

impl ArchiveWriter for ZpaqWriter {
    fn format(&self) -> FormatId {
        FormatId::Zpaq
    }

    fn add_path(
        &mut self,
        path: &Path,
        _opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        progress.report(ProgressUpdate {
            stage: JobStage::Create,
            bytes_done: 0,
            bytes_total: None,
            entries_done: 0,
            entries_total: None,
            current_entry: Some(path.display().to_string()),
            message: None,
        });
        self.inputs.push(path.to_path_buf());
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        if self.inputs.is_empty() {
            return Err(CoreError::Other("no input for zpaq".into()));
        }

        progress.report(ProgressUpdate {
            stage: JobStage::Create,
            bytes_done: 0,
            bytes_total: None,
            entries_done: 0,
            entries_total: None,
            current_entry: None,
            message: Some("zpaq".into()),
        });

        // Stage with relative names so archives don't embed absolute paths.
        let refs: Vec<&Path> = self.inputs.iter().map(Path::new).collect();
        let (_tmp, root) = stage_inputs(&refs, cancel)?;
        let archive = path_arg(&self.path)?;
        // Prefer absolute archive path so cwd change doesn't break output location.
        let archive_abs = if Path::new(&archive).is_absolute() {
            archive
        } else {
            let cwd = std::env::current_dir().map_err(CoreError::Io)?;
            path_arg(&cwd.join(&archive))?
        };

        let mut names = Vec::new();
        for entry in walkdir::WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .map_err(|e| CoreError::Other(e.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            names.push(rel);
        }
        if names.is_empty() {
            return Err(CoreError::Other("no files to compress for zpaq".into()));
        }

        let mut args = vec!["a".to_string(), archive_abs];
        args.extend(names);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_zpaq_in_dir(&root, &arg_refs)
    }
}
