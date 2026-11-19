//! Microsoft WIM reader via wimlib-imagex CLI (no in-tree WIM bitstream).
//!
//! - Windows: `OXIZIP_WIMLIB` / beside exe / `third_party/wimlib/` / PATH
//! - Other OS: system `wimlib-imagex` / `wimdir`+`wimextract` on PATH

use std::path::{Path, PathBuf};
use std::process::Command;

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

/// Locate a wimlib-imagex (or shorthand) executable.
pub fn find_wimlib(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }

    if let Ok(env) = std::env::var("OXIZIP_WIMLIB") {
        let p = PathBuf::from(env);
        if p.is_file() {
            return Some(p);
        }
    }

    #[cfg(windows)]
    let names = ["wimlib-imagex.exe", "wimlib-imagex", "wimdir.exe", "wimdir"];
    #[cfg(not(windows))]
    let names = ["wimlib-imagex", "wimdir"];

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
            let candidate = root.join("third_party").join("wimlib").join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
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

enum WimTool {
    /// Unified `wimlib-imagex` binary (subcommand form).
    Imagex(PathBuf),
    /// Separate `wimdir` / `wimextract` shims on PATH.
    Split {
        dir: Option<PathBuf>,
        extract: PathBuf,
    },
}

impl WimTool {
    fn discover(explicit: Option<&Path>) -> CoreResult<Self> {
        let found = find_wimlib(explicit).ok_or(CoreError::WimlibMissing)?;
        let stem = found
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if stem.contains("imagex") {
            return Ok(Self::Imagex(found));
        }
        // Found wimdir (or similar); locate wimextract beside it / on PATH.
        let extract = {
            let sibling = found
                .parent()
                .map(|d| {
                    #[cfg(windows)]
                    {
                        d.join("wimextract.exe")
                    }
                    #[cfg(not(windows))]
                    {
                        d.join("wimextract")
                    }
                })
                .filter(|p| p.is_file());
            sibling
                .or_else(|| which("wimextract"))
                .or_else(|| which("wimextract.exe"))
                .ok_or(CoreError::WimlibMissing)?
        };
        Ok(Self::Split {
            dir: Some(found),
            extract,
        })
    }

    fn run_dir(&self, wim: &Path, image: Option<&str>) -> CoreResult<std::process::Output> {
        let mut cmd = match self {
            Self::Imagex(p) => {
                let mut c = Command::new(p);
                c.arg("dir");
                c
            }
            Self::Split { dir: Some(p), .. } => Command::new(p),
            Self::Split { dir: None, .. } => {
                return Err(CoreError::WimlibMissing);
            }
        };
        cmd.arg(wim);
        if let Some(image) = image.filter(|s| !s.is_empty()) {
            cmd.arg(image);
        }
        cmd.output()
            .map_err(|e| CoreError::WimlibFailed(e.to_string()))
    }

    fn run_extract(
        &self,
        wim: &Path,
        image: Option<&str>,
        dest: &Path,
        paths: &[String],
    ) -> CoreResult<std::process::Output> {
        let mut cmd = match self {
            Self::Imagex(p) => {
                let mut c = Command::new(p);
                c.arg("extract");
                c
            }
            Self::Split { extract, .. } => Command::new(extract),
        };
        cmd.arg(wim);
        if let Some(image) = image.filter(|s| !s.is_empty()) {
            cmd.arg(image);
        }
        cmd.arg(format!("--dest-dir={}", dest.display()))
            .arg("--preserve-dir-structure")
            .arg("--no-acls");
        for path in paths {
            cmd.arg(path);
        }
        cmd.output()
            .map_err(|e| CoreError::WimlibFailed(e.to_string()))
    }
}

pub struct WimReader {
    path: PathBuf,
    tool: WimTool,
    /// 1-based image index when known; `None` lets wimlib pick the sole image.
    image: Option<String>,
}

impl WimReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let tool = WimTool::discover(None)?;
        // Validate by listing image 1 (or sole image).
        let output = tool.run_dir(&path, Some("1"))?;
        if !output.status.success() {
            let output2 = tool.run_dir(&path, None)?;
            if !output2.status.success() {
                return Err(CoreError::WimlibFailed(
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                ));
            }
            return Ok(Self {
                path,
                tool,
                image: None,
            });
        }
        Ok(Self {
            path,
            tool,
            image: Some("1".into()),
        })
    }
}

impl ArchiveReader for WimReader {
    fn format(&self) -> FormatId {
        FormatId::Wim
    }

    fn list(
        &mut self,
        _opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        progress.report(ProgressUpdate::new(JobStage::List));
        let output = self.tool.run_dir(&self.path, self.image.as_deref())?;
        if !output.status.success() {
            return Err(CoreError::WimlibFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();
        for (i, line) in text.lines().enumerate() {
            cancel.check()?;
            let name = line.trim().trim_start_matches('\\').replace('\\', "/");
            if name.is_empty() {
                continue;
            }
            let is_dir = name.ends_with('/');
            entries.push(EntryMeta {
                name: name.clone(),
                is_dir,
                uncompressed_size: None,
                compressed_size: None,
                modified: None,
                method: Some("wim".into()),
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
        let paths: Vec<String> = opts
            .entries
            .as_ref()
            .map(|v| {
                v.iter()
                    .map(|e| {
                        if e.starts_with('/') || e.starts_with('\\') {
                            e.clone()
                        } else {
                            format!("/{e}")
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let output =
            self.tool
                .run_extract(&self.path, self.image.as_deref(), &opts.dest_dir, &paths)?;
        if !output.status.success() {
            return Err(CoreError::WimlibFailed(
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
        // Listing via wimdir exercises header + directory metadata.
        let _ = self.list(opts, progress, cancel)?;
        Ok(())
    }
}
