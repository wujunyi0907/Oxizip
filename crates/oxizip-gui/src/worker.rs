//! Background archive jobs for the iced GUI.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use iced::futures::channel::oneshot;
use iced::Task;
use oxizip_engine::{
    add_to_archive, checksum_paths, create, delete_entries, extract, list,
    recover_archive_password, rename_entry, repair_archive, test_archive, write_archive_comment,
    CancellationToken, CreateOptions, EntryMeta, ExtractOptions, FileChecksum, FnProgress,
    FormatId, JobStage, ListOptions, PasswordRecoverOptions, Progress, ProgressUpdate,
};
use oxizip_i18n::{error_message, t, Lang};
use tempfile::TempDir;

use crate::Message;

/// GUI extract preferences threaded into worker jobs.
#[derive(Debug, Clone, Copy)]
pub struct ExtractRuntimePrefs {
    pub copy_zone_identifier: bool,
    pub delete_on_cancel: bool,
    pub smart_single_file_folder: bool,
}

impl Default for ExtractRuntimePrefs {
    fn default() -> Self {
        Self {
            copy_zone_identifier: true,
            delete_on_cancel: false,
            smart_single_file_folder: false,
        }
    }
}

fn make_temp_dir(temp_folder: &str) -> Result<TempDir, String> {
    if temp_folder.trim().is_empty() {
        TempDir::new().map_err(|e| e.to_string())
    } else {
        std::fs::create_dir_all(temp_folder).map_err(|e| e.to_string())?;
        TempDir::new_in(temp_folder).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    List,
    Extract,
    Create,
    OpenEntry,
    Test,
    Add,
    Delete,
    Rename,
    Repair,
    Checksum,
    RecoverPassword,
    WriteComment,
    ExtractForClipboard,
}

#[derive(Debug, Clone, Default)]
pub struct ProgressSnapshot {
    pub fraction: f32,
    /// Per-file / current-entry progress when bytes are known; else mirrors overall.
    pub file_fraction: f32,
    pub detail: String,
}

#[derive(Clone)]
pub struct ProgressSlot {
    inner: Arc<Mutex<ProgressSnapshot>>,
}

impl ProgressSlot {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ProgressSnapshot::default())),
        }
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        self.inner.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn set_from_update(&self, update: &ProgressUpdate) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        g.fraction = fraction_of(update);
        g.file_fraction = file_fraction_of(update).unwrap_or(g.fraction);
        g.detail = update
            .current_entry
            .clone()
            .or_else(|| update.message.clone())
            .unwrap_or_default();
    }
}

fn fraction_of(update: &ProgressUpdate) -> f32 {
    if let (Some(total), done) = (update.entries_total, update.entries_done) {
        if total > 0 {
            return (done as f32 / total as f32).clamp(0.0, 1.0);
        }
    }
    if let (Some(total), done) = (update.bytes_total, update.bytes_done) {
        if total > 0 {
            return (done as f32 / total as f32).clamp(0.0, 1.0);
        }
    }
    // Unknown total but entries advancing (e.g. zip walk): asymptotic fill.
    if update.entries_done > 0 {
        return (1.0 - 1.0 / (1.0 + update.entries_done as f32 * 0.12)).clamp(0.05, 0.95);
    }
    match update.stage {
        JobStage::Detect => 0.05,
        JobStage::List => 0.2,
        JobStage::Extract | JobStage::Create => 0.5,
        JobStage::Test => 0.4,
        JobStage::Checksum => 0.3,
        JobStage::Repair => 0.45,
        JobStage::RecoverPassword => 0.35,
    }
}

fn file_fraction_of(update: &ProgressUpdate) -> Option<f32> {
    let total = update.bytes_total?;
    if total == 0 {
        return Some(1.0);
    }
    Some((update.bytes_done as f32 / total as f32).clamp(0.0, 1.0))
}

#[derive(Debug, Clone)]
pub enum JobOutcome {
    Listed {
        path: PathBuf,
        entries: Vec<EntryMeta>,
        error: Option<String>,
    },
    Extracted {
        dest: PathBuf,
        error: Option<String>,
    },
    /// Shell/GUI extract found existing files; wait for Overwrite / AutoRename.
    NeedsConflictChoice {
        conflicts: Vec<String>,
    },
    Created {
        path: PathBuf,
        format: FormatId,
        error: Option<String>,
    },
    Opened {
        name: String,
        temp: Option<Arc<TempDir>>,
        error: Option<String>,
    },
    Tested {
        path: PathBuf,
        error: Option<String>,
    },
    Edited {
        path: PathBuf,
        error: Option<String>,
    },
    Renamed {
        path: PathBuf,
        error: Option<String>,
    },
    Repaired {
        path: PathBuf,
        error: Option<String>,
    },
    Checksummed {
        report: String,
        error: Option<String>,
    },
    PasswordRecovered {
        password: String,
        error: Option<String>,
    },
    CommentWritten {
        path: PathBuf,
        comment: String,
        error: Option<String>,
    },
    ClipboardFiles {
        paths: Vec<PathBuf>,
        temp: Option<Arc<TempDir>>,
        error: Option<String>,
    },
    Canceled,
}

pub fn spawn_job<F>(
    _kind: JobKind,
    cancel: CancellationToken,
    slot: ProgressSlot,
    work: F,
) -> Task<Message>
where
    F: FnOnce(&CancellationToken, &ProgressSlot) -> JobOutcome + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let outcome = if cancel.is_canceled() {
            JobOutcome::Canceled
        } else {
            work(&cancel, &slot)
        };
        let _ = tx.send(outcome);
    });

    Task::perform(
        async move {
            match rx.await {
                Ok(outcome) => outcome,
                Err(_) => JobOutcome::Canceled,
            }
        },
        Message::JobFinished,
    )
}

pub fn list_archive(
    path: PathBuf,
    password: Option<String>,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    let opts = ListOptions { password };
    match list(&path, &opts, &progress, cancel) {
        Ok(entries) => JobOutcome::Listed {
            path,
            entries,
            error: None,
        },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Listed {
            path,
            entries: Vec::new(),
            error: Some(error_message(lang, &err)),
        },
    }
}

pub fn add_files(
    archive: PathBuf,
    inputs: Vec<PathBuf>,
    opts: CreateOptions,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    match add_to_archive(&archive, &inputs, &opts, &progress, cancel) {
        Ok(()) => JobOutcome::Edited {
            path: archive,
            error: None,
        },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Edited {
            path: archive,
            error: Some(error_message(lang, &err)),
        },
    }
}

pub fn delete_files(
    archive: PathBuf,
    entries: Vec<String>,
    password: Option<String>,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    match delete_entries(&archive, &entries, password.as_deref(), &progress, cancel) {
        Ok(()) => JobOutcome::Edited {
            path: archive,
            error: None,
        },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Edited {
            path: archive,
            error: Some(error_message(lang, &err)),
        },
    }
}

pub fn test_archive_job(
    path: PathBuf,
    password: Option<String>,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    let opts = ListOptions { password };
    match test_archive(&path, &opts, &progress, cancel) {
        Ok(()) => JobOutcome::Tested { path, error: None },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Tested {
            path,
            error: Some(error_message(lang, &err)),
        },
    }
}

pub fn extract_archive(
    archive: PathBuf,
    dest: PathBuf,
    password: Option<String>,
    conflict: Option<oxizip_engine::ConflictPolicy>,
    prefs: ExtractRuntimePrefs,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    use oxizip_engine::ConflictPolicy;
    use std::fs;

    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });

    if conflict.is_none() {
        let list_opts = ListOptions {
            password: password.clone(),
        };
        match list(&archive, &list_opts, &progress, cancel) {
            Ok(entries) => {
                let _ = fs::create_dir_all(&dest);
                let conflicts = collect_conflicts(&dest, &entries);
                if !conflicts.is_empty() {
                    return JobOutcome::NeedsConflictChoice { conflicts };
                }
            }
            Err(oxizip_engine::CoreError::Canceled) => return JobOutcome::Canceled,
            Err(err) => {
                return JobOutcome::Extracted {
                    dest,
                    error: Some(error_message(lang, &err)),
                };
            }
        }
    }

    let policy = conflict.unwrap_or(ConflictPolicy::Overwrite);
    let mut opts = ExtractOptions::to_dir(&dest);
    opts.password = password;
    opts.conflict = policy;
    opts.copy_zone_identifier = prefs.copy_zone_identifier;
    match extract(&archive, &opts, &progress, cancel) {
        Ok(()) => JobOutcome::Extracted { dest, error: None },
        Err(oxizip_engine::CoreError::Canceled) => {
            if prefs.delete_on_cancel {
                let _ = fs::remove_dir_all(&dest);
            }
            JobOutcome::Canceled
        }
        Err(err) => JobOutcome::Extracted {
            dest,
            error: Some(error_message(lang, &err)),
        },
    }
}

fn collect_conflicts(dest: &std::path::Path, entries: &[EntryMeta]) -> Vec<String> {
    use oxizip_core::safe_dest_path;
    let mut out = Vec::new();
    let Ok(root) = dest.canonicalize().or_else(|_| {
        let _ = std::fs::create_dir_all(dest);
        dest.canonicalize()
    }) else {
        return out;
    };
    for e in entries {
        if e.is_dir || e.name.ends_with('/') {
            continue;
        }
        let Ok(path) = safe_dest_path(&root, &e.name) else {
            continue;
        };
        if path.is_file() {
            out.push(e.name.clone());
            if out.len() >= 8 {
                break;
            }
        }
    }
    out
}

fn resolve_shell_dest(
    mode: crate::job::ExtractMode,
    dest_override: Option<&PathBuf>,
    archive: &std::path::Path,
    entries: &[EntryMeta],
    prefs: ExtractRuntimePrefs,
) -> PathBuf {
    use crate::job::{dest_archive_name, dest_here, dest_smart, ExtractMode};
    if let Some(dest) = dest_override {
        return dest.clone();
    }
    match mode {
        ExtractMode::Here | ExtractMode::Dialog => dest_here(archive),
        ExtractMode::ArchiveName => dest_archive_name(archive),
        ExtractMode::Smart => dest_smart(archive, entries, prefs.smart_single_file_folder),
    }
}

/// Shell `--job extract`: resolve Here/Smart/ArchiveName/Dialog dest, then extract each archive.
/// `conflict = None` means probe first and return [`JobOutcome::NeedsConflictChoice`] if needed.
pub fn extract_shell_job(
    archives: Vec<PathBuf>,
    mode: crate::job::ExtractMode,
    dest_override: Option<PathBuf>,
    password: Option<String>,
    conflict: Option<oxizip_engine::ConflictPolicy>,
    prefs: ExtractRuntimePrefs,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    use crate::job::{dest_here, ExtractMode};
    use oxizip_engine::ConflictPolicy;
    use std::fs;

    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });

    // Probe phase (ask user).
    if conflict.is_none() {
        let mut all_conflicts = Vec::new();
        for archive in &archives {
            if cancel.is_canceled() {
                return JobOutcome::Canceled;
            }
            let list_opts = ListOptions {
                password: password.clone(),
            };
            let entries = match list(archive, &list_opts, &progress, cancel) {
                Ok(e) => e,
                Err(oxizip_engine::CoreError::Canceled) => return JobOutcome::Canceled,
                Err(err) => {
                    return JobOutcome::Extracted {
                        dest: dest_here(archive),
                        error: Some(error_message(lang, &err)),
                    };
                }
            };
            let dest = resolve_shell_dest(mode, dest_override.as_ref(), archive, &entries, prefs);
            let _ = fs::create_dir_all(&dest);
            for name in collect_conflicts(&dest, &entries) {
                if !all_conflicts.contains(&name) {
                    all_conflicts.push(name);
                }
                if all_conflicts.len() >= 8 {
                    break;
                }
            }
            if all_conflicts.len() >= 8 {
                break;
            }
        }
        if !all_conflicts.is_empty() {
            return JobOutcome::NeedsConflictChoice {
                conflicts: all_conflicts,
            };
        }
    }

    let policy = conflict.unwrap_or(ConflictPolicy::Overwrite);
    let mut last_dest = PathBuf::new();
    let total = archives.len() as u64;
    for (i, archive) in archives.into_iter().enumerate() {
        if cancel.is_canceled() {
            return JobOutcome::Canceled;
        }
        slot.set_from_update(&ProgressUpdate {
            stage: JobStage::Extract,
            bytes_done: 0,
            bytes_total: None,
            entries_done: i as u64,
            entries_total: Some(total.max(1)),
            current_entry: Some(archive.display().to_string()),
            message: None,
        });
        let list_opts = ListOptions {
            password: password.clone(),
        };
        let entries = match list(&archive, &list_opts, &progress, cancel) {
            Ok(e) => e,
            Err(oxizip_engine::CoreError::Canceled) => return JobOutcome::Canceled,
            Err(err) => {
                return JobOutcome::Extracted {
                    dest: dest_here(&archive),
                    error: Some(error_message(lang, &err)),
                };
            }
        };
        let dest = resolve_shell_dest(mode, dest_override.as_ref(), &archive, &entries, prefs);
        if let Err(err) = fs::create_dir_all(&dest) {
            return JobOutcome::Extracted {
                dest,
                error: Some(err.to_string()),
            };
        }
        let mut opts = ExtractOptions::to_dir(&dest);
        opts.password = password.clone();
        opts.conflict = policy;
        opts.copy_zone_identifier = prefs.copy_zone_identifier;
        match extract(&archive, &opts, &progress, cancel) {
            Ok(()) => last_dest = dest,
            Err(oxizip_engine::CoreError::Canceled) => {
                if prefs.delete_on_cancel
                    && !matches!(mode, ExtractMode::Here | ExtractMode::Dialog)
                    && dest_override.is_none()
                {
                    let _ = fs::remove_dir_all(&dest);
                }
                return JobOutcome::Canceled;
            }
            Err(err) => {
                return JobOutcome::Extracted {
                    dest,
                    error: Some(error_message(lang, &err)),
                };
            }
        }
    }
    JobOutcome::Extracted {
        dest: last_dest,
        error: None,
    }
}

pub fn create_archive(
    out: PathBuf,
    format: FormatId,
    inputs: Vec<PathBuf>,
    opts: CreateOptions,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    match create(&out, format, &inputs, &opts, &progress, cancel) {
        Ok(()) => JobOutcome::Created {
            path: out,
            format,
            error: None,
        },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Created {
            path: out,
            format,
            error: Some(error_message(lang, &err)),
        },
    }
}

/// Create one archive beside each input path.
pub fn create_archive_each(
    inputs: Vec<PathBuf>,
    format: FormatId,
    ext: String,
    opts: CreateOptions,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    use crate::job::default_create_path;
    let mut last = PathBuf::new();
    for input in inputs {
        if cancel.is_canceled() {
            return JobOutcome::Canceled;
        }
        let out = default_create_path(&input, &ext);
        match create_archive(
            out.clone(),
            format,
            vec![input],
            opts.clone(),
            cancel,
            slot,
            lang,
        ) {
            JobOutcome::Created {
                path, error: None, ..
            } => last = path,
            JobOutcome::Canceled => return JobOutcome::Canceled,
            JobOutcome::Created {
                path,
                error: Some(e),
                ..
            } => {
                return JobOutcome::Created {
                    path,
                    format,
                    error: Some(e),
                };
            }
            other => return other,
        }
    }
    JobOutcome::Created {
        path: last,
        format,
        error: None,
    }
}

pub fn open_entry(
    archive: PathBuf,
    entry: EntryMeta,
    password: Option<String>,
    temp_folder: String,
    copy_zone: bool,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    let temp = match make_temp_dir(&temp_folder) {
        Ok(t) => t,
        Err(e) => {
            return JobOutcome::Opened {
                name: entry.name,
                temp: None,
                error: Some(e),
            };
        }
    };
    let mut opts = ExtractOptions::to_dir(temp.path());
    opts.password = password;
    opts.entries = Some(vec![entry.name.clone()]);
    opts.copy_zone_identifier = copy_zone;
    if let Err(err) = extract(&archive, &opts, &progress, cancel) {
        if matches!(err, oxizip_engine::CoreError::Canceled) {
            return JobOutcome::Canceled;
        }
        return JobOutcome::Opened {
            name: entry.name,
            temp: None,
            error: Some(error_message(lang, &err)),
        };
    }
    let file_path = temp.path().join(&entry.name);
    if let Err(e) = open::that(&file_path) {
        return JobOutcome::Opened {
            name: entry.name,
            temp: None,
            error: Some(format!("{}: {e}", t(lang, "gui.err.system_open"))),
        };
    }
    JobOutcome::Opened {
        name: entry.name,
        temp: Some(Arc::new(temp)),
        error: None,
    }
}

pub fn rename_entry_job(
    archive: PathBuf,
    from: String,
    to: String,
    password: Option<String>,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    match rename_entry(&archive, &from, &to, password.as_deref(), &progress, cancel) {
        Ok(()) => JobOutcome::Renamed {
            path: archive,
            error: None,
        },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Renamed {
            path: archive,
            error: Some(error_message(lang, &err)),
        },
    }
}

pub fn repair_job(
    archive: PathBuf,
    dest: Option<PathBuf>,
    password: Option<String>,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    match repair_archive(
        &archive,
        dest.as_deref(),
        password.as_deref(),
        &progress,
        cancel,
    ) {
        Ok(path) => JobOutcome::Repaired { path, error: None },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Repaired {
            path: dest.unwrap_or(archive),
            error: Some(error_message(lang, &err)),
        },
    }
}

pub fn recover_password_job(
    archive: PathBuf,
    opts: PasswordRecoverOptions,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    match recover_archive_password(&archive, &opts, &progress, cancel) {
        Ok(password) => JobOutcome::PasswordRecovered {
            password,
            error: None,
        },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::PasswordRecovered {
            password: String::new(),
            error: Some(error_message(lang, &err)),
        },
    }
}

pub fn write_comment_job(
    archive: PathBuf,
    comment: String,
    cancel: &CancellationToken,
    _slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    if cancel.is_canceled() {
        return JobOutcome::Canceled;
    }
    match write_archive_comment(&archive, &comment) {
        Ok(()) => JobOutcome::CommentWritten {
            path: archive,
            comment,
            error: None,
        },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::CommentWritten {
            path: archive,
            comment,
            error: Some(error_message(lang, &err)),
        },
    }
}

pub fn extract_for_clipboard_job(
    archive: PathBuf,
    names: Vec<String>,
    password: Option<String>,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    let temp = match TempDir::new() {
        Ok(t) => t,
        Err(e) => {
            return JobOutcome::ClipboardFiles {
                paths: Vec::new(),
                temp: None,
                error: Some(e.to_string()),
            };
        }
    };
    let mut opts = ExtractOptions::to_dir(temp.path());
    opts.password = password;
    opts.entries = Some(names.clone());
    if let Err(err) = extract(&archive, &opts, &progress, cancel) {
        if matches!(err, oxizip_engine::CoreError::Canceled) {
            return JobOutcome::Canceled;
        }
        return JobOutcome::ClipboardFiles {
            paths: Vec::new(),
            temp: None,
            error: Some(error_message(lang, &err)),
        };
    }
    let paths: Vec<PathBuf> = names
        .iter()
        .map(|name| temp.path().join(name))
        .filter(|path| path.is_file())
        .collect();
    if paths.is_empty() {
        return JobOutcome::ClipboardFiles {
            paths,
            temp: None,
            error: Some(t(lang, "gui.err.clipboard_empty").into()),
        };
    }
    JobOutcome::ClipboardFiles {
        paths,
        temp: Some(Arc::new(temp)),
        error: None,
    }
}

pub fn checksum_job(
    archive: PathBuf,
    names: Vec<String>,
    password: Option<String>,
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    if names.is_empty() {
        return checksum_outcome(&[archive], &progress, cancel, lang);
    }
    let temp = match TempDir::new() {
        Ok(t) => t,
        Err(e) => {
            return JobOutcome::Checksummed {
                report: String::new(),
                error: Some(e.to_string()),
            };
        }
    };
    let mut opts = ExtractOptions::to_dir(temp.path());
    opts.password = password;
    opts.entries = Some(names.clone());
    if let Err(err) = extract(&archive, &opts, &progress, cancel) {
        if matches!(err, oxizip_engine::CoreError::Canceled) {
            return JobOutcome::Canceled;
        }
        return JobOutcome::Checksummed {
            report: String::new(),
            error: Some(error_message(lang, &err)),
        };
    }
    let paths: Vec<PathBuf> = names
        .iter()
        .map(|name| temp.path().join(name))
        .filter(|path| path.is_file())
        .collect();
    if paths.is_empty() {
        return JobOutcome::Checksummed {
            report: String::new(),
            error: Some(t(lang, "gui.err.checksum_empty").into()),
        };
    }
    checksum_outcome(&paths, &progress, cancel, lang)
}

fn checksum_outcome(
    paths: &[PathBuf],
    progress: &dyn Progress,
    cancel: &CancellationToken,
    lang: Lang,
) -> JobOutcome {
    match checksum_paths(paths, progress, cancel) {
        Ok(items) => JobOutcome::Checksummed {
            report: format_checksums(lang, &items),
            error: None,
        },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Checksummed {
            report: String::new(),
            error: Some(error_message(lang, &err)),
        },
    }
}

fn format_checksums(lang: Lang, items: &[FileChecksum]) -> String {
    let mut out = String::new();
    for item in items {
        out.push_str(&item.path);
        out.push('\n');
        out.push_str(&format!(
            "  {} {}\n  CRC32 {}\n  SHA256 {}\n",
            t(lang, "gui.col.size"),
            item.size,
            item.crc32_hex(),
            item.sha256_hex(),
        ));
    }
    out
}
