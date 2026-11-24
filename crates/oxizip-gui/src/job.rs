//! Shell-launched compact job dialog (`--job create|extract …`) with dual progress.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use oxizip_engine::{EntryMeta, FormatId};

/// Compact progress window (installer-class dialog, not the 775×480 main UI).
pub const JOB_WINDOW_SIZE: iced::Size = iced::Size::new(520.0, 240.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobVerb {
    Create,
    Extract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractMode {
    Here,
    Smart,
    ArchiveName,
    /// Prompt for destination folder (no dest yet).
    Dialog,
}

#[derive(Debug, Clone)]
pub struct CreateJob {
    pub format: FormatId,
    pub out: PathBuf,
    pub inputs: Vec<PathBuf>,
    /// Create one archive per input path (beside each).
    pub each: bool,
}

#[derive(Debug, Clone)]
pub struct ExtractJob {
    pub archives: Vec<PathBuf>,
    pub mode: ExtractMode,
    /// Delete source archive(s) after successful extract.
    pub delete_archive: bool,
    /// When set (e.g. after folder dialog), extract all archives here.
    pub dest_override: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum PendingJob {
    Create(CreateJob),
    Extract(ExtractJob),
}

/// Parse `oxizip --job create|extract …`
pub fn parse_job_args() -> Option<PendingJob> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--job" {
            i += 1;
            break;
        }
        i += 1;
    }
    if i >= args.len() {
        return None;
    }
    let verb = args[i].as_str();
    i += 1;
    match verb {
        "create" => parse_create(&args[i..]),
        "extract" => parse_extract(&args[i..]),
        _ => None,
    }
}

fn parse_create(args: &[String]) -> Option<PendingJob> {
    let mut beside = false;
    let mut each = false;
    let mut format = String::from("zip");
    let mut paths = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--beside" => {
                beside = true;
                i += 1;
            }
            "--each" => {
                each = true;
                beside = true;
                i += 1;
            }
            "-f" | "--format" => {
                i += 1;
                format = args.get(i)?.clone();
                i += 1;
            }
            s if s.starts_with('-') => {
                i += 1;
            }
            _ => {
                paths.push(PathBuf::from(&args[i]));
                i += 1;
            }
        }
    }
    if paths.is_empty() {
        return None;
    }
    let fmt = FormatId::parse_create_name(&format)?;
    let (out, inputs) = if beside || each {
        (default_create_path(&paths[0], fmt.as_str()), paths)
    } else if paths.len() >= 2 {
        let mut paths = paths;
        let out = paths.remove(0);
        (out, paths)
    } else {
        return None;
    };
    Some(PendingJob::Create(CreateJob {
        format: fmt,
        out,
        inputs,
        each,
    }))
}

fn parse_extract(args: &[String]) -> Option<PendingJob> {
    let mut mode = ExtractMode::Dialog;
    let mut mode_set = false;
    let mut delete_archive = false;
    let mut paths = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--here" => {
                mode = ExtractMode::Here;
                mode_set = true;
                i += 1;
            }
            "--smart" => {
                mode = ExtractMode::Smart;
                mode_set = true;
                i += 1;
            }
            "--archive-name-dir" => {
                mode = ExtractMode::ArchiveName;
                mode_set = true;
                i += 1;
            }
            "--dialog" => {
                mode = ExtractMode::Dialog;
                mode_set = true;
                i += 1;
            }
            "--delete-archive" => {
                delete_archive = true;
                i += 1;
            }
            s if s.starts_with('-') => {
                i += 1;
            }
            _ => {
                paths.push(PathBuf::from(&args[i]));
                i += 1;
            }
        }
    }
    if paths.is_empty() {
        return None;
    }
    let _ = mode_set;
    Some(PendingJob::Extract(ExtractJob {
        archives: paths,
        mode,
        delete_archive,
        dest_override: None,
    }))
}

/// Default create output beside the first input: `<dir>/<name>.<ext>`.
pub fn default_create_path(first_input: &Path, format_ext: &str) -> PathBuf {
    let parent = first_input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = if first_input.is_dir() {
        first_input
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive".into())
    } else {
        first_input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive".into())
    };
    parent.join(format!("{stem}.{format_ext}"))
}

fn archive_parent(archive: &Path) -> PathBuf {
    archive
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn archive_stem(archive: &Path) -> String {
    archive
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".into())
}

pub fn dest_here(archive: &Path) -> PathBuf {
    archive_parent(archive)
}

pub fn dest_archive_name(archive: &Path) -> PathBuf {
    archive_parent(archive).join(archive_stem(archive))
}

/// One shared top-level folder → Here; otherwise ArchiveName.
/// When `single_file_folder` is set and the archive holds a single file, use ArchiveName.
pub fn dest_smart(archive: &Path, entries: &[EntryMeta], single_file_folder: bool) -> PathBuf {
    let tops: BTreeSet<&str> = entries
        .iter()
        .filter_map(|e| {
            let name = e.name.trim_matches('/');
            if name.is_empty() {
                None
            } else {
                Some(name.split('/').next().unwrap_or(name))
            }
        })
        .collect();
    let file_count = entries
        .iter()
        .filter(|e| !e.is_dir && !e.name.ends_with('/'))
        .count();
    if tops.len() == 1 {
        if single_file_folder && file_count == 1 {
            dest_archive_name(archive)
        } else {
            dest_here(archive)
        }
    } else {
        dest_archive_name(archive)
    }
}

pub fn format_hms(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

pub fn eta_pair(started: Instant, fraction: f32) -> (String, String) {
    let elapsed = started.elapsed();
    let elapsed_s = format_hms(elapsed);
    let total_s = if fraction > 0.02 {
        let total = elapsed.div_f32(fraction.clamp(0.02, 1.0));
        format_hms(total)
    } else {
        "--:--:--".into()
    };
    (elapsed_s, total_s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beside_path_uses_stem() {
        let p = PathBuf::from(r"C:\tmp\photo.png");
        assert_eq!(
            default_create_path(&p, "zip"),
            PathBuf::from(r"C:\tmp\photo.zip")
        );
    }

    #[test]
    fn smart_single_root_uses_here() {
        let archive = Path::new("/tmp/pack.zip");
        let entries = vec![
            EntryMeta {
                name: "root/a.txt".into(),
                is_dir: false,
                uncompressed_size: None,
                compressed_size: None,
                modified: None,
                method: None,
            },
            EntryMeta {
                name: "root/b.txt".into(),
                is_dir: false,
                uncompressed_size: None,
                compressed_size: None,
                modified: None,
                method: None,
            },
        ];
        assert_eq!(dest_smart(archive, &entries, false), PathBuf::from("/tmp"));
    }

    #[test]
    fn smart_single_file_uses_folder_when_enabled() {
        let archive = Path::new("/tmp/pack.zip");
        let entries = vec![EntryMeta {
            name: "only.txt".into(),
            is_dir: false,
            uncompressed_size: None,
            compressed_size: None,
            modified: None,
            method: None,
        }];
        assert_eq!(
            dest_smart(archive, &entries, true),
            PathBuf::from("/tmp/pack")
        );
    }
}
