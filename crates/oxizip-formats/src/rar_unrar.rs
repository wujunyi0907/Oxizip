//! RAR support via official UnRAR CLI only (no RAR bitstream code in-tree).
//!
//! - Windows: official UnRAR binary (env / beside exe / third_party / PATH)
//! - Other OS: system-installed `unrar` on PATH (or `OXIZIP_UNRAR`)

use std::path::{Path, PathBuf};
use std::process::Command;

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

/// Locate an official UnRAR executable (64-bit targets only).
pub fn find_unrar(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }

    if let Ok(env) = std::env::var("OXIZIP_UNRAR") {
        let p = PathBuf::from(env);
        if p.is_file() {
            return Some(p);
        }
    }

    #[cfg(windows)]
    {
        let names = ["UnRAR.exe", "unrar.exe"];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for name in names {
                    let candidate = dir.join(name);
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        for root in candidate_repo_roots() {
            for name in names {
                let candidate = root.join("third_party").join("unrar").join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
                // Legacy layout if user drops CLI next to docs.
                let candidate = root.join("third_party").join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        for name in names {
            if let Some(p) = which(name) {
                return Some(p);
            }
        }
        None
    }

    #[cfg(not(windows))]
    {
        // Non-Windows: rely on system package (`apt/dnf install unrar`, etc.).
        for name in ["unrar", "UnRAR"] {
            if let Some(p) = which(name) {
                return Some(p);
            }
        }
        None
    }
}

#[cfg(windows)]
fn candidate_repo_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.clone());
        let mut cur = cwd;
        for _ in 0..4 {
            if let Some(parent) = cur.parent() {
                cur = parent.to_path_buf();
                roots.push(cur.clone());
            } else {
                break;
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe;
        for _ in 0..6 {
            if let Some(parent) = cur.parent() {
                cur = parent.to_path_buf();
                roots.push(cur.clone());
            } else {
                break;
            }
        }
    }
    roots
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            if !name.ends_with(".exe") {
                let with_exe = dir.join(format!("{name}.exe"));
                if with_exe.is_file() {
                    return Some(with_exe);
                }
            }
        }
    }
    None
}

pub struct RarUnrarReader {
    path: PathBuf,
    unrar: PathBuf,
}

impl RarUnrarReader {
    pub fn open(path: impl Into<PathBuf>, unrar_path: Option<&Path>) -> CoreResult<Self> {
        let unrar = find_unrar(unrar_path).ok_or(CoreError::UnrarMissing)?;
        Ok(Self {
            path: path.into(),
            unrar,
        })
    }
}

impl ArchiveReader for RarUnrarReader {
    fn format(&self) -> FormatId {
        FormatId::Rar
    }

    fn list(
        &mut self,
        opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        progress.report(ProgressUpdate::new(JobStage::List));
        let mut cmd = Command::new(&self.unrar);
        cmd.arg("lb")
            .arg(password_arg(opts.password.as_deref()))
            .arg(&self.path);
        let output = cmd
            .output()
            .map_err(|e| CoreError::UnrarFailed(e.to_string()))?;
        if !output.status.success() {
            return Err(CoreError::UnrarFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();
        for (i, line) in text.lines().enumerate() {
            cancel.check()?;
            let name = line.trim();
            if name.is_empty() {
                continue;
            }
            entries.push(EntryMeta {
                name: name.replace('\\', "/"),
                is_dir: name.ends_with('/') || name.ends_with('\\'),
                uncompressed_size: None,
                compressed_size: None,
                modified: None,
                method: Some("rar".into()),
            });
            progress.report(ProgressUpdate {
                stage: JobStage::List,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
                entries_total: None,
                current_entry: Some(name.to_string()),
                message: None,
            });
        }
        Ok(entries)
    }

    fn extract(
        &mut self,
        opts: &ExtractOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        std::fs::create_dir_all(&opts.dest_dir)?;
        progress.report(ProgressUpdate::new(JobStage::Extract));
        let mut dest = opts.dest_dir.to_string_lossy().into_owned();
        #[cfg(windows)]
        {
            if !dest.ends_with('\\') && !dest.ends_with('/') {
                dest.push('\\');
            }
        }
        #[cfg(not(windows))]
        {
            if !dest.ends_with('/') {
                dest.push('/');
            }
        }
        let mut cmd = Command::new(&self.unrar);
        cmd.arg("x")
            .arg("-o+")
            .arg("-y")
            .arg(password_arg(opts.password.as_deref()))
            .arg(&self.path)
            .arg(&dest);
        let output = cmd
            .output()
            .map_err(|e| CoreError::UnrarFailed(e.to_string()))?;
        if !output.status.success() {
            return Err(CoreError::UnrarFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
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
        progress.report(ProgressUpdate::new(JobStage::Test));
        let output = Command::new(&self.unrar)
            .arg("t")
            .arg(password_arg(opts.password.as_deref()))
            .arg(&self.path)
            .output()
            .map_err(|e| CoreError::UnrarFailed(e.to_string()))?;
        if !output.status.success() {
            return Err(CoreError::UnrarFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        Ok(())
    }
}

fn password_arg(password: Option<&str>) -> String {
    match password {
        Some(p) if !p.is_empty() => format!("-p{p}"),
        _ => "-p-".into(),
    }
}
