//! Oxizip iced GUI — list / extract / create; open via system default app.

mod worker;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use iced::widget::{
    button, column, container, pick_list, progress_bar, row, scrollable, text, text_input, Column,
    Space,
};
use iced::{Element, Length, Subscription, Task};
use oxizip_engine::{CreateOptions, EntryMeta, FormatId, MethodId};
use oxizip_i18n::{t, Lang};
use tempfile::TempDir;
use worker::{spawn_job, JobKind, JobOutcome, ProgressSlot};

fn main() -> iced::Result {
    iced::application(OxizipApp::title, OxizipApp::update, OxizipApp::view)
        .subscription(OxizipApp::subscription)
        .window_size((980.0, 640.0))
        .run_with(OxizipApp::new)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormatLabel(FormatId);

impl std::fmt::Display for FormatLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodLabel {
    Auto,
    Method(MethodId),
}

impl MethodLabel {
    fn all() -> &'static [MethodLabel] {
        &[
            MethodLabel::Auto,
            MethodLabel::Method(MethodId::Store),
            MethodLabel::Method(MethodId::Deflate),
            MethodLabel::Method(MethodId::Bzip2),
            MethodLabel::Method(MethodId::Lzma),
            MethodLabel::Method(MethodId::Xz),
            MethodLabel::Method(MethodId::Zstd),
            MethodLabel::Method(MethodId::Lzma2),
            MethodLabel::Method(MethodId::Ppmd),
        ]
    }

    fn to_option(self) -> Option<MethodId> {
        match self {
            MethodLabel::Auto => None,
            MethodLabel::Method(m) => Some(m),
        }
    }
}

impl std::fmt::Display for MethodLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MethodLabel::Auto => f.write_str("auto"),
            MethodLabel::Method(m) => f.write_str(m.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LangLabel {
    Zh,
    En,
}

impl LangLabel {
    fn from_lang(lang: Lang) -> Self {
        match lang {
            Lang::ZhCn => Self::Zh,
            Lang::En => Self::En,
        }
    }

    fn to_lang(self) -> Lang {
        match self {
            Self::Zh => Lang::ZhCn,
            Self::En => Lang::En,
        }
    }
}

impl std::fmt::Display for LangLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zh => f.write_str("中文"),
            Self::En => f.write_str("EN"),
        }
    }
}

struct Busy {
    cancel: oxizip_engine::CancellationToken,
    progress: ProgressSlot,
}

struct OxizipApp {
    lang: Lang,
    archive: Option<PathBuf>,
    entries: Vec<EntryMeta>,
    status: String,
    last_error: Option<String>,
    password: String,
    create_format: FormatId,
    create_method: MethodLabel,
    pending_create_inputs: Option<Vec<PathBuf>>,
    busy: Option<Busy>,
    progress_fraction: f32,
    progress_detail: String,
    open_temp: Option<Arc<TempDir>>,
}

impl Default for OxizipApp {
    fn default() -> Self {
        Self {
            lang: Lang::ZhCn,
            archive: None,
            entries: Vec::new(),
            status: String::new(),
            last_error: None,
            password: String::new(),
            create_format: FormatId::Zip,
            create_method: MethodLabel::Auto,
            pending_create_inputs: None,
            busy: None,
            progress_fraction: 0.0,
            progress_detail: String::new(),
            open_temp: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    OpenClicked,
    ExtractClicked,
    CreateClicked,
    CancelClicked,
    FormatSelected(FormatLabel),
    MethodSelected(MethodLabel),
    LangSelected(LangLabel),
    PasswordChanged(String),
    OpenEntry(usize),
    ArchiveChosen(Option<PathBuf>),
    ExtractDirChosen(Option<PathBuf>),
    CreateInputsChosen(Option<Vec<PathBuf>>),
    CreateOutputChosen(Option<PathBuf>),
    Tick,
    JobFinished(JobOutcome),
}

impl OxizipApp {
    fn new() -> (Self, Task<Message>) {
        (
            Self {
                status: t(Lang::ZhCn, "gui.placeholder").to_string(),
                ..Self::default()
            },
            Task::none(),
        )
    }

    fn title(&self) -> String {
        t(self.lang, "app.name").to_string()
    }

    fn is_busy(&self) -> bool {
        self.busy.is_some()
    }

    fn password_opt(&self) -> Option<String> {
        let p = self.password.trim();
        if p.is_empty() {
            None
        } else {
            Some(p.to_string())
        }
    }

    fn create_options(&self) -> CreateOptions {
        CreateOptions {
            password: self.password_opt(),
            method: self.create_method.to_option(),
            ..CreateOptions::default()
        }
    }

    fn begin_busy(&mut self, kind: JobKind) -> (oxizip_engine::CancellationToken, ProgressSlot) {
        let cancel = oxizip_engine::CancellationToken::new();
        let progress = ProgressSlot::new();
        self.busy = Some(Busy {
            cancel: cancel.clone(),
            progress: progress.clone(),
        });
        self.progress_fraction = 0.0;
        self.progress_detail.clear();
        self.last_error = None;
        self.status = match kind {
            JobKind::List => t(self.lang, "gui.busy.list"),
            JobKind::Extract => t(self.lang, "gui.busy.extract"),
            JobKind::Create => t(self.lang, "gui.busy.create"),
            JobKind::OpenEntry => t(self.lang, "gui.busy.open"),
        }
        .to_string();
        (cancel, progress)
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.busy.is_some() {
            iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenClicked => {
                if self.is_busy() {
                    return Task::none();
                }
                Task::perform(pick_archive(), Message::ArchiveChosen)
            }
            Message::ExtractClicked => {
                if self.is_busy() {
                    return Task::none();
                }
                if self.archive.is_none() {
                    self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
                    return Task::none();
                }
                Task::perform(pick_folder(), Message::ExtractDirChosen)
            }
            Message::CreateClicked => {
                if self.is_busy() {
                    return Task::none();
                }
                Task::perform(pick_inputs(), Message::CreateInputsChosen)
            }
            Message::CancelClicked => {
                if let Some(busy) = &self.busy {
                    busy.cancel.cancel();
                }
                Task::none()
            }
            Message::FormatSelected(label) => {
                self.create_format = label.0;
                Task::none()
            }
            Message::MethodSelected(label) => {
                self.create_method = label;
                Task::none()
            }
            Message::LangSelected(label) => {
                self.lang = label.to_lang();
                if self.archive.is_none() && self.busy.is_none() {
                    self.status = t(self.lang, "gui.placeholder").to_string();
                }
                Task::none()
            }
            Message::PasswordChanged(value) => {
                self.password = value;
                Task::none()
            }
            Message::ArchiveChosen(Some(path)) => self.start_list(path),
            Message::ArchiveChosen(None) => Task::none(),
            Message::ExtractDirChosen(Some(dir)) => self.start_extract(dir),
            Message::ExtractDirChosen(None) => Task::none(),
            Message::CreateInputsChosen(Some(inputs)) => {
                if inputs.is_empty() {
                    self.last_error = Some(t(self.lang, "gui.err.need_inputs").into());
                    return Task::none();
                }
                self.pending_create_inputs = Some(inputs);
                let fmt = self.create_format;
                Task::perform(pick_create_output(fmt), Message::CreateOutputChosen)
            }
            Message::CreateInputsChosen(None) => Task::none(),
            Message::CreateOutputChosen(Some(out)) => self.start_create(out),
            Message::CreateOutputChosen(None) => {
                self.pending_create_inputs = None;
                Task::none()
            }
            Message::OpenEntry(index) => self.start_open_entry(index),
            Message::Tick => {
                if let Some(busy) = &self.busy {
                    let snap = busy.progress.snapshot();
                    self.progress_fraction = snap.fraction;
                    self.progress_detail = snap.detail;
                }
                Task::none()
            }
            Message::JobFinished(outcome) => {
                self.busy = None;
                self.progress_fraction = 0.0;
                self.progress_detail.clear();
                self.apply_outcome(outcome)
            }
        }
    }

    fn start_list(&mut self, path: PathBuf) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let (cancel, progress) = self.begin_busy(JobKind::List);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(JobKind::List, cancel, progress, move |cancel, progress| {
            worker::list_archive(path, password, cancel, progress, lang)
        })
    }

    fn start_extract(&mut self, dir: PathBuf) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            return Task::none();
        };
        let (cancel, progress) = self.begin_busy(JobKind::Extract);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(
            JobKind::Extract,
            cancel,
            progress,
            move |cancel, progress| {
                worker::extract_archive(archive, dir, password, cancel, progress, lang)
            },
        )
    }

    fn start_create(&mut self, out: PathBuf) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(inputs) = self.pending_create_inputs.take() else {
            return Task::none();
        };
        let format = self.create_format;
        let opts = self.create_options();
        let (cancel, progress) = self.begin_busy(JobKind::Create);
        let lang = self.lang;
        spawn_job(JobKind::Create, cancel, progress, move |cancel, progress| {
            worker::create_archive(out, format, inputs, opts, cancel, progress, lang)
        })
    }

    fn start_open_entry(&mut self, index: usize) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            return Task::none();
        };
        let Some(entry) = self.entries.get(index).cloned() else {
            return Task::none();
        };
        if entry.is_dir {
            self.last_error = Some(t(self.lang, "gui.err.dir_open").into());
            return Task::none();
        }
        let (cancel, progress) = self.begin_busy(JobKind::OpenEntry);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(
            JobKind::OpenEntry,
            cancel,
            progress,
            move |cancel, progress| {
                worker::open_entry(archive, entry, password, cancel, progress, lang)
            },
        )
    }

    fn apply_outcome(&mut self, outcome: JobOutcome) -> Task<Message> {
        match outcome {
            JobOutcome::Listed {
                path,
                entries,
                error,
            } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    self.status = t(self.lang, "gui.status.ready").into();
                } else {
                    self.archive = Some(path.clone());
                    self.entries = entries;
                    self.last_error = None;
                    self.status = format!(
                        "{} {} — {} {}",
                        t(self.lang, "gui.status.opened"),
                        path.display(),
                        self.entries.len(),
                        t(self.lang, "gui.entries"),
                    );
                }
                Task::none()
            }
            JobOutcome::Extracted { dest, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                } else {
                    self.last_error = None;
                    self.status = format!(
                        "{} → {}",
                        t(self.lang, "gui.status.extracted"),
                        dest.display()
                    );
                }
                Task::none()
            }
            JobOutcome::Created {
                path,
                format,
                error,
            } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    Task::none()
                } else {
                    self.last_error = None;
                    self.status = format!(
                        "{} {} ({})",
                        t(self.lang, "gui.status.created"),
                        path.display(),
                        format.as_str()
                    );
                    self.start_list(path)
                }
            }
            JobOutcome::Opened {
                name,
                temp,
                error,
            } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                } else {
                    self.open_temp = temp;
                    self.last_error = None;
                    self.status = format!("{} {}", t(self.lang, "gui.status.opened"), name);
                }
                Task::none()
            }
            JobOutcome::Canceled => {
                self.status = t(self.lang, "gui.status.canceled").into();
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let formats: Vec<FormatLabel> = FormatId::create_choices()
            .iter()
            .copied()
            .map(FormatLabel)
            .collect();
        let methods: Vec<MethodLabel> = MethodLabel::all().to_vec();
        let langs = vec![LangLabel::Zh, LangLabel::En];
        let busy = self.is_busy();

        let open_btn = {
            let b = button(t(self.lang, "gui.open_archive"));
            if busy {
                b
            } else {
                b.on_press(Message::OpenClicked)
            }
        };
        let extract_btn = {
            let b = button(t(self.lang, "gui.extract"));
            if busy {
                b
            } else {
                b.on_press(Message::ExtractClicked)
            }
        };
        let create_btn = {
            let b = button(t(self.lang, "gui.create"));
            if busy {
                b
            } else {
                b.on_press(Message::CreateClicked)
            }
        };
        let cancel_btn = {
            let b = button(t(self.lang, "gui.cancel"));
            if busy {
                b.on_press(Message::CancelClicked)
            } else {
                b
            }
        };

        let toolbar = row![
            open_btn,
            Space::with_width(6),
            extract_btn,
            Space::with_width(6),
            create_btn,
            Space::with_width(6),
            cancel_btn,
            Space::with_width(16),
            text(t(self.lang, "gui.format")),
            pick_list(
                formats,
                Some(FormatLabel(self.create_format)),
                Message::FormatSelected,
            )
            .width(90),
            Space::with_width(8),
            text(t(self.lang, "gui.method")),
            pick_list(
                methods,
                Some(self.create_method),
                Message::MethodSelected,
            )
            .width(100),
            Space::with_width(8),
            text(t(self.lang, "gui.password")),
            text_input("", &self.password)
                .secure(true)
                .on_input(Message::PasswordChanged)
                .width(140),
            Space::with_width(Length::Fill),
            text(t(self.lang, "gui.lang")),
            pick_list(
                langs,
                Some(LangLabel::from_lang(self.lang)),
                Message::LangSelected,
            )
            .width(72),
        ]
        .padding(8)
        .spacing(4)
        .align_y(iced::Alignment::Center);

        let header = row![
            text(t(self.lang, "gui.col.name")).width(Length::FillPortion(6)),
            text(t(self.lang, "gui.col.size")).width(Length::FillPortion(2)),
            text(t(self.lang, "gui.col.method")).width(Length::FillPortion(2)),
            text(t(self.lang, "gui.col.action")).width(Length::FillPortion(2)),
        ]
        .padding([4, 8])
        .spacing(8);

        let mut list_col = Column::new().spacing(2).padding(4);
        for (i, entry) in self.entries.iter().enumerate() {
            let name = if entry.is_dir {
                format!("[DIR] {}", entry.name)
            } else {
                entry.name.clone()
            };
            let size = entry
                .uncompressed_size
                .map(|s| human_size(s))
                .unwrap_or_else(|| "-".into());
            let method = entry.method.clone().unwrap_or_else(|| "-".into());
            let open_btn = {
                let b = button(t(self.lang, "gui.open_entry"));
                if busy || entry.is_dir {
                    b
                } else {
                    b.on_press(Message::OpenEntry(i))
                }
            };
            list_col = list_col.push(
                row![
                    text(name).width(Length::FillPortion(6)),
                    text(size).width(Length::FillPortion(2)),
                    text(method).width(Length::FillPortion(2)),
                    open_btn.width(Length::FillPortion(2)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }

        let err = self
            .last_error
            .as_ref()
            .map(|e| text(e.clone()).size(14))
            .unwrap_or_else(|| text("").size(14));

        let progress_row = if busy {
            column![
                progress_bar(0.0..=1.0, self.progress_fraction),
                text(self.progress_detail.clone()).size(13),
            ]
            .spacing(4)
        } else {
            column![Space::with_height(0)]
        };

        let content = column![
            toolbar,
            text(self.status.clone()).size(15),
            err,
            progress_row,
            header,
            scrollable(list_col).height(Length::Fill),
        ]
        .spacing(8)
        .padding(12);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn human_size(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let v = n as f64;
    if v >= GB {
        format!("{:.2} GB", v / GB)
    } else if v >= MB {
        format!("{:.2} MB", v / MB)
    } else if v >= KB {
        format!("{:.1} KB", v / KB)
    } else {
        format!("{n} B")
    }
}

async fn pick_archive() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .add_filter(
            "Archives",
            &[
                "zip", "zipx", "tar", "gz", "tgz", "xz", "zst", "bz2", "7z", "rar", "exe", "lzh",
                "iso", "zpaq", "001",
            ],
        )
        .pick_file()
        .await
        .map(|f| f.path().to_path_buf())
}

async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|f| f.path().to_path_buf())
}

async fn pick_inputs() -> Option<Vec<PathBuf>> {
    rfd::AsyncFileDialog::new()
        .pick_files()
        .await
        .map(|files| files.iter().map(|f| f.path().to_path_buf()).collect())
}

async fn pick_create_output(format: FormatId) -> Option<PathBuf> {
    let ext = format.as_str();
    let default_name = format!("archive.{ext}");
    rfd::AsyncFileDialog::new()
        .set_file_name(&default_name)
        .add_filter(ext, &[ext])
        .save_file()
        .await
        .map(|f| f.path().to_path_buf())
}
