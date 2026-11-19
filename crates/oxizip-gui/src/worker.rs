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
    cancel: &CancellationToken,
    slot: &ProgressSlot,
    lang: Lang,
) -> JobOutcome {
    let progress = FnProgress({
        let slot = slot.clone();
        move |u: ProgressUpdate| slot.set_from_update(&u)
    });
    let mut opts = ExtractOptions::to_dir(&dest);
    opts.password = password;
    match extract(&archive, &opts, &progress, cancel) {
        Ok(()) => JobOutcome::Extracted { dest, error: None },
        Err(oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Extracted {
            dest,
            error: Some(error_message(lang, &err)),
        },
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

pub fn open_entry(
    archive: PathBuf,
    entry: EntryMeta,
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
            return JobOutcome::Opened {
                name: entry.name,
                temp: None,
                error: Some(e.to_string()),
            };
        }
    };
    let mut opts = ExtractOptions::to_dir(temp.path());
    opts.password = password;
    opts.entries = Some(vec![entry.name.clone()]);
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
