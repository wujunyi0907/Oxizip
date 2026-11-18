use std::fs;
use std::path::{Path, PathBuf};

use oxizip_core::{
    safe_dest_path, validate_entry_name, CancellationToken, CoreError, CoreResult, JobStage,
    Progress, ProgressUpdate,
};
use sevenz_turbo::Password;
use tempfile::TempDir;

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

pub struct SevenZReader {
    path: PathBuf,
}

impl SevenZReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        Ok(Self { path: path.into() })
    }

    fn password(opts_password: Option<&str>) -> Password {
        match opts_password {
            Some(p) if !p.is_empty() => Password::from(p),
            _ => Password::empty(),
        }
    }

    fn map_err(e: sevenz_turbo::Error) -> CoreError {
        let msg = e.to_string();
        let lower = msg.to_ascii_lowercase();
        if lower.contains("password") {
            if lower.contains("required") {
                CoreError::PasswordRequired
            } else {
                CoreError::InvalidPassword
            }
        } else {
            CoreError::Other(format!("7z: {msg}"))
        }
    }
}

impl ArchiveReader for SevenZReader {
    fn format(&self) -> FormatId {
        FormatId::SevenZ
    }

    fn list(
        &mut self,
        opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        let file = fs::File::open(&self.path)?;
        let password = Self::password(opts.password.as_deref());
        let reader = sevenz_turbo::ArchiveReader::new(file, password).map_err(Self::map_err)?;
        let mut out = Vec::new();
        for (i, entry) in reader.archive().files.iter().enumerate() {
            cancel.check()?;
            let name = entry.name().replace('\\', "/");
            out.push(EntryMeta {
                name: name.clone(),
                is_dir: entry.is_directory(),
                uncompressed_size: Some(entry.size()),
                compressed_size: None,
                modified: None,
                method: Some("7z".into()),
            });
            progress.report(ProgressUpdate {
                stage: JobStage::List,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
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
        let dest_root = opts.dest_dir.canonicalize().map_err(CoreError::Io)?;

        let entries = self.list(
            &ListOptions {
                password: opts.password.clone(),
            },
            progress,
            cancel,
        )?;
        for e in &entries {
            if e.is_dir {
                continue;
            }
            validate_entry_name(&e.name)?;
        }

        progress.report(ProgressUpdate::new(JobStage::Extract));
        let stage = TempDir::new().map_err(CoreError::Io)?;
        let password = Self::password(opts.password.as_deref());
        sevenz_turbo::decompress_file_with_password(&self.path, stage.path(), password)
            .map_err(Self::map_err)?;

        copy_tree(stage.path(), &dest_root, opts, progress, cancel)
    }
}

fn copy_tree(
    stage: &Path,
    dest_root: &Path,
    opts: &ExtractOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<()> {
    let mut done = 0u64;
    for entry in walkdir::WalkDir::new(stage).into_iter().filter_map(|e| e.ok()) {
        cancel.check()?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let rel = path
            .strip_prefix(stage)
            .map_err(|e| CoreError::Other(e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() {
            continue;
        }
        if let Some(filter) = &opts.entries {
            if !filter.iter().any(|n| n == &rel) {
                continue;
            }
        }
        let out = safe_dest_path(dest_root, &rel)?;
        if out.exists() && !opts.overwrite {
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, &out)?;
        done += 1;
        progress.report(ProgressUpdate {
            stage: JobStage::Extract,
            bytes_done: 0,
            bytes_total: None,
            entries_done: done,
            entries_total: None,
            current_entry: Some(rel),
            message: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::SevenZWriter;
    use crate::traits::ArchiveWriter;
    use crate::types::CreateOptions;
    use oxizip_core::{CancellationToken, NullProgress};
    use tempfile::tempdir;

    #[test]
    fn sevenz_password_roundtrip() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("enc.7z");
        let src = dir.path().join("a.txt");
        fs::write(&src, b"sevenz secret").unwrap();

        let mut w = Box::new(SevenZWriter::create(&archive).unwrap());
        let cancel = CancellationToken::new();
        let opts = CreateOptions {
            password: Some("p@ss".into()),
            ..Default::default()
        };
        w.add_path(&src, &opts, &NullProgress, &cancel).unwrap();
        w.finish(&NullProgress, &cancel).unwrap();

        let mut r = SevenZReader::open(&archive).unwrap();
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let mut xo = ExtractOptions::to_dir(&out);
        xo.password = Some("p@ss".into());
        r.extract(&xo, &NullProgress, &cancel).unwrap();
        assert_eq!(fs::read(out.join("a.txt")).unwrap(), b"sevenz secret");
    }
}
