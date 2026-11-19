use std::fs::File;
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};
use sevenz_turbo::encoder_options::{AesEncoderOptions, Lzma2Options};
use sevenz_turbo::ArchiveWriter as SevenzArchiveWriter;
use sevenz_turbo::{ArchiveEntry, EncoderMethod, Password};

use crate::traits::ArchiveWriter;
use crate::types::{CreateOptions, FormatId, MethodId};
use crate::util::{ensure_parent, stage_inputs};

pub struct SevenZWriter {
    path: PathBuf,
    inputs: Vec<PathBuf>,
    opts: CreateOptions,
}

impl SevenZWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Self {
            path,
            inputs: Vec::new(),
            opts: CreateOptions::default(),
        })
    }
}

impl ArchiveWriter for SevenZWriter {
    fn format(&self) -> FormatId {
        FormatId::SevenZ
    }

    fn add_path(
        &mut self,
        path: &Path,
        opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        self.opts = opts.clone();
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
            return Err(CoreError::Other("no input for 7z".into()));
        }

        let refs: Vec<&Path> = self.inputs.iter().map(Path::new).collect();
        let (_staging, compress_root) = if refs.len() == 1 && refs[0].is_dir() {
            (None, refs[0].to_path_buf())
        } else {
            let (tmp, root) = stage_inputs(&refs, cancel)?;
            (Some(tmp), root)
        };

        progress.report(ProgressUpdate {
            stage: JobStage::Create,
            bytes_done: 0,
            bytes_total: None,
            entries_done: 0,
            entries_total: None,
            current_entry: None,
            message: Some("7z".into()),
        });

        let file = File::create(&self.path)?;
        let mut writer = SevenzArchiveWriter::new(file)
            .map_err(|e| CoreError::Other(format!("7z writer: {e}")))?;

        apply_7z_methods(&mut writer, &self.opts)?;

        push_path_recursive(&compress_root, &compress_root, &mut writer, cancel)?;
        writer
            .finish()
            .map_err(|e| CoreError::Other(format!("7z finish: {e}")))?;
        drop(_staging);
        Ok(())
    }
}

fn apply_7z_methods(
    writer: &mut SevenzArchiveWriter<File>,
    opts: &CreateOptions,
) -> CoreResult<()> {
    let method = opts.method.unwrap_or(MethodId::Lzma2);
    let mut chain = Vec::new();

    if let Some(password) = opts.password.as_deref() {
        if !password.is_empty() {
            chain.push(AesEncoderOptions::new(Password::from(password)).into());
            writer.set_encrypt_header(true);
        }
    }

    let codec = match method {
        MethodId::Store => EncoderMethod::COPY.into(),
        MethodId::Lzma2 | MethodId::Lzma => {
            let mut cfg = EncoderMethod::LZMA2.into();
            if let Some(level) = opts.level {
                // Lzma2Options level 0-9 roughly
                cfg = Lzma2Options::from_level(level.min(9)).into();
            }
            cfg
        }
        MethodId::Deflate => EncoderMethod::DEFLATE.into(),
        MethodId::Bzip2 => EncoderMethod::BZIP2.into(),
        MethodId::Zstd => EncoderMethod::ZSTD.into(),
        MethodId::Ppmd => EncoderMethod::PPMD.into(),
        MethodId::Xz => {
            return Err(CoreError::Other(
                "xz method is for zip/zipx; use lzma2 for 7z".into(),
            ));
        }
    };
    chain.push(codec);
    writer.set_content_methods(chain);
    Ok(())
}

fn push_path_recursive<W: std::io::Write + std::io::Seek>(
    src: &Path,
    root: &Path,
    writer: &mut SevenzArchiveWriter<W>,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    cancel.check()?;
    if src.is_dir() {
        if src != root {
            let name = src
                .strip_prefix(root)
                .map_err(|e| CoreError::Other(e.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let entry = ArchiveEntry::from_path(src, name);
            writer
                .push_archive_entry::<&[u8]>(entry, None)
                .map_err(|e| CoreError::Other(format!("7z entry: {e}")))?;
        }
        for dent in src.read_dir()? {
            let dent = dent?;
            let ft = dent.file_type()?;
            if ft.is_dir() || ft.is_file() {
                push_path_recursive(&dent.path(), root, writer, cancel)?;
            }
        }
    } else if src.is_file() {
        let name = src
            .strip_prefix(root)
            .map_err(|e| CoreError::Other(e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        let entry = ArchiveEntry::from_path(src, name);
        let file = File::open(src)?;
        writer
            .push_archive_entry(entry, Some(file))
            .map_err(|e| CoreError::Other(format!("7z entry: {e}")))?;
    }
    Ok(())
}
