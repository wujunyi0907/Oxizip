//! Background archive jobs for the iced GUI.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use iced::futures::channel::oneshot;
use iced::Task;
use oxizip_engine::{
    create, extract, list, CancellationToken, CreateOptions, EntryMeta, ExtractOptions, FormatId,
    FnProgress, JobStage, ListOptions, ProgressUpdate,
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
        self.inner
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
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
        Err(err) if matches!(err, oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
        Err(err) => JobOutcome::Listed {
            path,
            entries: Vec::new(),
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
        Ok(()) => JobOutcome::Extracted {
            dest,
            error: None,
        },
        Err(err) if matches!(err, oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
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
        Err(err) if matches!(err, oxizip_engine::CoreError::Canceled) => JobOutcome::Canceled,
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
