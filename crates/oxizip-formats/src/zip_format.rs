use oxizip_core::{
    safe_dest_path, CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate,
};
use std::fs::{self, File};
use std::io::{copy, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, ZipWriter};

use crate::traits::{ArchiveReader, ArchiveWriter};
use crate::types::{
    CreateOptions, EntryMeta, ExtractOptions, FormatId, ListOptions, MethodId, ZipCryptoKind,
};

pub struct ZipReader {
    path: PathBuf,
}

impl ZipReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let file = File::open(&path)?;
        let _ = ZipArchive::new(BufReader::new(file))
            .map_err(|e| CoreError::Other(format!("invalid zip: {e}")))?;
        Ok(Self { path })
    }

    fn archive(&self) -> CoreResult<ZipArchive<BufReader<File>>> {
        let file = File::open(&self.path)?;
        ZipArchive::new(BufReader::new(file))
            .map_err(|e| CoreError::Other(format!("invalid zip: {e}")))
    }
}

impl ArchiveReader for ZipReader {
    fn format(&self) -> FormatId {
        FormatId::Zip
    }

    fn list(
        &mut self,
        opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        let mut archive = self.archive()?;
        let total = archive.len() as u64;
        let mut out = Vec::with_capacity(archive.len());
        let password = opts.password.as_deref().map(str::as_bytes);

        for i in 0..archive.len() {
            cancel.check()?;
            let file = open_zip_entry(&mut archive, i, password)?;
            let name = file
                .enclosed_name()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| file.name().replace('\\', "/"));

            out.push(EntryMeta {
                name,
                is_dir: file.is_dir(),
                uncompressed_size: Some(file.size()),
                compressed_size: Some(file.compressed_size()),
                modified: None,
                method: Some(format!("{:?}", file.compression())),
            });

            progress.report(ProgressUpdate {
                stage: JobStage::List,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(file.name().to_string()),
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
        let dest_root = opts.dest_dir.canonicalize().map_err(CoreError::Io)?;
        let mut archive = self.archive()?;
        let total = archive.len() as u64;
        let filter = opts.entries.as_ref();
        let password = opts.password.as_deref().map(str::as_bytes);

        for i in 0..archive.len() {
            cancel.check()?;
            let mut file = open_zip_entry(&mut archive, i, password)?;

            let enclosed = file.enclosed_name().ok_or_else(|| {
                CoreError::PathUnsafe(format!("rejected zip path: {}", file.name()))
            })?;
            let rel = enclosed.to_string_lossy().replace('\\', "/");

            if let Some(entries) = filter {
                if !entries
                    .iter()
                    .any(|e| e == &rel || e.trim_end_matches('/') == rel.trim_end_matches('/'))
                {
                    continue;
                }
            }

            let out_path = safe_dest_path(&dest_root, &rel)?;

            progress.report(ProgressUpdate {
                stage: JobStage::Extract,
                bytes_done: 0,
                bytes_total: Some(file.size()),
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(rel.clone()),
                message: None,
            });

            if file.is_dir() || rel.ends_with('/') {
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
            copy(&mut file, &mut outfile).map_err(map_zip_io_err)?;
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
        let mut archive = self.archive()?;
        let total = archive.len() as u64;
        let password = opts.password.as_deref().map(str::as_bytes);
        for i in 0..archive.len() {
            cancel.check()?;
            let mut file = open_zip_entry(&mut archive, i, password)?;
            let name = file.name().to_string();
            progress.report(ProgressUpdate {
                stage: JobStage::Test,
                bytes_done: 0,
                bytes_total: Some(file.size()),
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(name),
                message: None,
            });
            if file.is_dir() {
                continue;
            }
            // Decompress fully; the zip crate checks CRC on drop/EOF.
            std::io::copy(&mut file, &mut std::io::sink()).map_err(map_zip_io_err)?;
        }
        Ok(())
    }
}

fn open_zip_entry<'a>(
    archive: &'a mut ZipArchive<BufReader<File>>,
    index: usize,
    password: Option<&[u8]>,
) -> CoreResult<zip::read::ZipFile<'a, BufReader<File>>> {
    let result = match password {
        Some(pw) => archive.by_index_decrypt(index, pw),
        None => archive.by_index(index),
    };
    result.map_err(map_zip_err)
}

fn map_zip_err(e: zip::result::ZipError) -> CoreError {
    let msg = e.to_string();
    let lower = msg.to_ascii_lowercase();
    if lower.contains("password") || lower.contains("encrypt") {
        if lower.contains("required") || lower.contains("missing") {
            CoreError::PasswordRequired
        } else {
            CoreError::InvalidPassword
        }
    } else {
        CoreError::Other(format!("zip: {msg}"))
    }
}

fn map_zip_io_err(e: std::io::Error) -> CoreError {
    let msg = e.to_string().to_ascii_lowercase();
    if msg.contains("password") || msg.contains("authentication") {
        CoreError::InvalidPassword
    } else {
        CoreError::Io(e)
    }
}

pub struct ZipWriterBackend {
    writer: ZipWriter<BufWriter<File>>,
    base_method: CompressionMethod,
    format: FormatId,
    path: PathBuf,
    files_done: u64,
}

impl ZipWriterBackend {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        Self::create_with(path, FormatId::Zip, CompressionMethod::Deflated)
    }

    pub fn create_zipx(path: impl Into<PathBuf>) -> CoreResult<Self> {
        Self::create_with(path, FormatId::Zipx, CompressionMethod::Zstd)
    }

    fn create_with(
        path: impl Into<PathBuf>,
        format: FormatId,
        method: CompressionMethod,
    ) -> CoreResult<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let file = File::create(&path)?;
        Ok(Self {
            writer: ZipWriter::new(BufWriter::new(file)),
            base_method: method,
            format,
            path,
            files_done: 0,
        })
    }

    fn resolve_method(&self, opts: &CreateOptions) -> CoreResult<CompressionMethod> {
        match opts.method {
            Some(MethodId::Store) => Ok(CompressionMethod::Stored),
            Some(MethodId::Deflate) => Ok(CompressionMethod::Deflated),
            Some(MethodId::Bzip2) => Ok(CompressionMethod::Bzip2),
            Some(MethodId::Lzma) => Ok(CompressionMethod::Lzma),
            Some(MethodId::Xz) => Ok(CompressionMethod::Xz),
            Some(MethodId::Zstd) => Ok(CompressionMethod::Zstd),
            Some(MethodId::Lzma2 | MethodId::Ppmd) => Err(CoreError::Other(format!(
                "method {} is for 7z, not zip/zipx",
                opts.method.unwrap().as_str()
            ))),
            None => Ok(self.base_method),
        }
    }

    fn start_file_with_opts(&mut self, name: &str, opts: &CreateOptions) -> CoreResult<()> {
        let method = self.resolve_method(opts)?;
        let mut options = SimpleFileOptions::default()
            .compression_method(method)
            .unix_permissions(0o644);
        if let Some(level) = opts.level {
            options = options.compression_level(Some(i64::from(level.min(9))));
        }

        let result = match (opts.password.as_deref(), opts.zip_crypto) {
            (Some(_), ZipCryptoKind::ZipCrypto) => {
                return Err(CoreError::Other(
                    "legacy ZipCrypto create is not available; use aes/aes128/aes192/aes256".into(),
                ));
            }
            (Some(password), ZipCryptoKind::Aes256) => self
                .writer
                .start_file(name, options.with_aes_encryption(AesMode::Aes256, password)),
            (Some(password), ZipCryptoKind::Aes128) => self
                .writer
                .start_file(name, options.with_aes_encryption(AesMode::Aes128, password)),
            (Some(password), ZipCryptoKind::Aes192) => self
                .writer
                .start_file(name, options.with_aes_encryption(AesMode::Aes192, password)),
            (None, _) => self.writer.start_file(name, options),
        };
        result.map_err(|e| CoreError::Other(format!("zip start_file: {e}")))
    }
}

impl ArchiveWriter for ZipWriterBackend {
    fn format(&self) -> FormatId {
        self.format
    }

    fn add_path(
        &mut self,
        path: &Path,
        opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        let path = path.canonicalize().map_err(CoreError::Io)?;
        if path.is_file() {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".into());
            self.add_file_with_name(&path, &name, opts, progress, cancel)?;
            return Ok(());
        }

        let root = path.clone();
        for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
            cancel.check()?;
            let p = entry.path();
            if p.is_dir() {
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .map_err(|e| CoreError::Other(e.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            self.add_file_with_name(p, &rel, opts, progress, cancel)?;
        }
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        _progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        self.writer
            .finish()
            .map_err(|e| CoreError::Other(format!("zip finish: {e}")))?;
        let _ = &self.path;
        Ok(())
    }
}

impl ZipWriterBackend {
    fn add_file_with_name(
        &mut self,
        path: &Path,
        name: &str,
        opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        self.files_done = self.files_done.saturating_add(1);
        let file_len = fs::metadata(path).map(|m| m.len()).ok();
        progress.report(ProgressUpdate {
            stage: JobStage::Create,
            bytes_done: 0,
            bytes_total: file_len,
            entries_done: self.files_done,
            entries_total: None,
            current_entry: Some(name.to_string()),
            message: None,
        });
        self.start_file_with_opts(name, opts)?;
        let mut input = File::open(path)?;
        copy(&mut input, &mut self.writer)?;
        if let Some(total) = file_len {
            progress.report(ProgressUpdate {
                stage: JobStage::Create,
                bytes_done: total,
                bytes_total: Some(total),
                entries_done: self.files_done,
                entries_total: None,
                current_entry: Some(name.to_string()),
                message: None,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxizip_core::{validate_entry_name, NullProgress};
    use tempfile::tempdir;

    #[test]
    fn zip_roundtrip_and_slip_rejected() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        let src = dir.path().join("hello.txt");
        fs::write(&src, b"hello oxizip").unwrap();

        let mut writer = Box::new(ZipWriterBackend::create(&zip_path).unwrap());
        let cancel = CancellationToken::new();
        writer
            .add_path(&src, &CreateOptions::default(), &NullProgress, &cancel)
            .unwrap();
        writer.finish(&NullProgress, &cancel).unwrap();

        let mut reader = ZipReader::open(&zip_path).unwrap();
        let entries = reader
            .list(&ListOptions::default(), &NullProgress, &cancel)
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hello.txt");

        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        reader
            .extract(&ExtractOptions::to_dir(&out), &NullProgress, &cancel)
            .unwrap();
        assert_eq!(fs::read(out.join("hello.txt")).unwrap(), b"hello oxizip");

        assert!(validate_entry_name("../evil.txt").is_err());
    }

    #[test]
    fn zip_aes_password_roundtrip() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("enc.zip");
        let src = dir.path().join("secret.txt");
        fs::write(&src, b"top secret").unwrap();

        let opts = CreateOptions {
            password: Some("s3cret".into()),
            zip_crypto: ZipCryptoKind::Aes256,
            method: Some(MethodId::Deflate),
            ..Default::default()
        };
        let mut writer = Box::new(ZipWriterBackend::create(&zip_path).unwrap());
        let cancel = CancellationToken::new();
        writer
            .add_path(&src, &opts, &NullProgress, &cancel)
            .unwrap();
        writer.finish(&NullProgress, &cancel).unwrap();

        let mut reader = ZipReader::open(&zip_path).unwrap();
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let mut extract_opts = ExtractOptions::to_dir(&out);
        extract_opts.password = Some("s3cret".into());
        reader
            .extract(&extract_opts, &NullProgress, &cancel)
            .unwrap();
        assert_eq!(fs::read(out.join("secret.txt")).unwrap(), b"top secret");

        let mut bad = ExtractOptions::to_dir(dir.path().join("bad"));
        fs::create_dir_all(bad.dest_dir.clone()).unwrap();
        bad.password = Some("wrong".into());
        assert!(reader.extract(&bad, &NullProgress, &cancel).is_err());
    }

    #[test]
    fn zipx_zstd_method() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.zipx");
        let src = dir.path().join("a.txt");
        fs::write(&src, b"zzzzzzzzzzzz").unwrap();
        let mut w = Box::new(ZipWriterBackend::create_zipx(&path).unwrap());
        let cancel = CancellationToken::new();
        let opts = CreateOptions {
            method: Some(MethodId::Zstd),
            ..Default::default()
        };
        w.add_path(&src, &opts, &NullProgress, &cancel).unwrap();
        w.finish(&NullProgress, &cancel).unwrap();
        let mut r = ZipReader::open(&path).unwrap();
        let entries = r
            .list(&ListOptions::default(), &NullProgress, &cancel)
            .unwrap();
        assert_eq!(entries.len(), 1);
    }
}
