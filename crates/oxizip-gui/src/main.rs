//! Oxizip iced GUI — list / extract / create; open via system default app.

// Release builds: no console window on Windows. Keep console in debug for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icons;
mod licenses;
mod setup;
mod worker;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use iced::widget::{
    button, checkbox, column, container, image, pick_list, progress_bar, row, scrollable, text,
    text_input, Column, Space,
};
use iced::{Alignment, Element, Length, Size, Subscription, Task};
use std::collections::BTreeMap;
use icons::IconSet;
use licenses::{catalog, ComponentKind};
use oxizip_engine::{CreateOptions, EntryMeta, FormatId, MethodId};
use oxizip_i18n::{t, Lang};
use tempfile::TempDir;
use worker::{spawn_job, JobKind, JobOutcome, ProgressSlot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Screen {
    #[default]
    Main,
    Setup,
    Licenses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LicenseFilter {
    All,
    SelfProject,
    Embedded,
    ExternalCli,
}

impl LicenseFilter {
    fn all() -> &'static [LicenseFilter] {
        &[
            Self::All,
            Self::SelfProject,
            Self::Embedded,
            Self::ExternalCli,
        ]
    }

    fn matches(self, kind: ComponentKind) -> bool {
        match self {
            Self::All => true,
            Self::SelfProject => kind == ComponentKind::SelfProject,
            Self::Embedded => kind == ComponentKind::Embedded,
            Self::ExternalCli => kind == ComponentKind::ExternalCli,
        }
    }

    fn label(self, lang: Lang) -> String {
        match self {
            Self::All => t(lang, "gui.licenses.filter_all").to_string(),
            Self::SelfProject => ComponentKind::SelfProject.label(lang).to_string(),
            Self::Embedded => ComponentKind::Embedded.label(lang).to_string(),
            Self::ExternalCli => ComponentKind::ExternalCli.label(lang).to_string(),
        }
    }
}

impl std::fmt::Display for LicenseFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Fallback English labels for pick_list Display; real UI uses custom text rows.
        f.write_str(match self {
            Self::All => "All",
            Self::SelfProject => "Project",
            Self::Embedded => "Embedded",
            Self::ExternalCli => "External CLI",
        })
    }
}

fn main() -> iced::Result {
    let mut window = iced::window::Settings {
        size: Size::new(980.0, 640.0),
        ..Default::default()
    };
    window.icon = icons::window_icon();

    iced::application(OxizipApp::title, OxizipApp::update, OxizipApp::view)
        .subscription(OxizipApp::subscription)
        .default_font(ui_font())
        .window(window)
        .run_with(OxizipApp::new)
}

/// Prefer a system CJK-capable font so zh-CN UI is not tofu boxes.
fn ui_font() -> iced::Font {
    #[cfg(windows)]
    {
        iced::Font::with_name("Microsoft YaHei UI")
    }
    #[cfg(target_os = "macos")]
    {
        iced::Font::with_name("PingFang SC")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Requires fonts-noto-cjk (or equivalent) on the system.
        iced::Font::with_name("Noto Sans CJK SC")
    }
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
    screen: Screen,
    license_filter: LicenseFilter,
    archive: Option<PathBuf>,
    archive_format: Option<FormatId>,
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
    icons: IconSet,
    setup_exts: BTreeMap<&'static str, bool>,
    pending_open: Option<PathBuf>,
}

impl Default for OxizipApp {
    fn default() -> Self {
        Self {
            lang: Lang::ZhCn,
            screen: Screen::Main,
            license_filter: LicenseFilter::All,
            archive: None,
            archive_format: None,
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
            icons: IconSet::load(),
            setup_exts: setup::default_assoc_selection(),
            pending_open: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    OpenClicked,
    ExtractClicked,
    ExtractHereClicked,
    ExtractSmartClicked,
    TestClicked,
    AddClicked,
    DeleteEntry(usize),
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
    AddInputsChosen(Option<Vec<PathBuf>>),
    Tick,
    JobFinished(JobOutcome),
    ShowLicenses,
    HideLicenses,
    LicenseFilterSelected(LicenseFilter),
    ShowSetup,
    SetupExtToggled(&'static str, bool),
    SetupSelectAll,
    SetupClearAll,
    SetupConfirm,
    SetupSkip,
}

impl OxizipApp {
    fn new() -> (Self, Task<Message>) {
        let (force_setup, pending_open) = setup::parse_launch_args();
        let screen = if setup::needs_first_run_setup(force_setup) {
            Screen::Setup
        } else {
            Screen::Main
        };
        let mut app = Self {
            status: t(Lang::ZhCn, "gui.placeholder").to_string(),
            screen,
            pending_open,
            ..Self::default()
        };
        let task = if app.screen == Screen::Main {
            app.take_pending_open_task()
        } else {
            Task::none()
        };
        (app, task)
    }

    fn take_pending_open_task(&mut self) -> Task<Message> {
        if let Some(path) = self.pending_open.take() {
            self.start_list(path)
        } else {
            Task::none()
        }
    }

    fn finish_setup(&mut self, associate: bool) -> Task<Message> {
        if associate {
            let selected = setup::selected_exts(&self.setup_exts);
            if selected.is_empty() {
                self.last_error = Some(t(self.lang, "gui.setup.none").to_string());
                return Task::none();
            }
            let lang_arg = match self.lang {
                Lang::ZhCn => "zh-cn",
                Lang::En => "en",
            };
            match setup::install_associations(lang_arg, &selected) {
                Ok(msg) => {
                    self.status = format!("{} — {msg}", t(self.lang, "gui.setup.ok"));
                    self.last_error = None;
                }
                Err(e) => {
                    self.last_error = Some(format!("{}: {e}", t(self.lang, "gui.setup.failed")));
                    return Task::none();
                }
            }
        }
        setup::mark_setup_done();
        self.screen = Screen::Main;
        self.take_pending_open_task()
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
            JobKind::Test => t(self.lang, "gui.busy.test"),
            JobKind::Add => t(self.lang, "gui.busy.add"),
            JobKind::Delete => t(self.lang, "gui.busy.delete"),
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
            Message::ExtractHereClicked => {
                let Some(archive) = self.archive.clone() else {
                    self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
                    return Task::none();
                };
                let dest = archive
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                self.start_extract(dest)
            }
            Message::ExtractSmartClicked => self.start_extract_smart(),
            Message::TestClicked => self.start_test(),
            Message::AddClicked => {
                if self.is_busy() {
                    return Task::none();
                }
                if self.archive.is_none() {
                    self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
                    return Task::none();
                }
                Task::perform(pick_inputs(), Message::AddInputsChosen)
            }
            Message::DeleteEntry(index) => self.start_delete(index),
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
            Message::AddInputsChosen(Some(inputs)) => {
                if inputs.is_empty() {
                    self.last_error = Some(t(self.lang, "gui.err.need_inputs").into());
                    return Task::none();
                }
                self.start_add(inputs)
            }
            Message::AddInputsChosen(None) => Task::none(),
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
            Message::ShowLicenses => {
                self.screen = Screen::Licenses;
                Task::none()
            }
            Message::HideLicenses => {
                self.screen = Screen::Main;
                Task::none()
            }
            Message::ShowSetup => {
                self.screen = Screen::Setup;
                self.setup_exts = setup::default_assoc_selection();
                self.last_error = None;
                Task::none()
            }
            Message::SetupExtToggled(ext, on) => {
                if let Some(slot) = self.setup_exts.get_mut(ext) {
                    *slot = on;
                }
                Task::none()
            }
            Message::SetupSelectAll => {
                for on in self.setup_exts.values_mut() {
                    *on = true;
                }
                Task::none()
            }
            Message::SetupClearAll => {
                for on in self.setup_exts.values_mut() {
                    *on = false;
                }
                Task::none()
            }
            Message::SetupConfirm => self.finish_setup(true),
            Message::SetupSkip => self.finish_setup(false),
            Message::LicenseFilterSelected(filter) => {
                self.license_filter = filter;
                Task::none()
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

    fn start_extract_smart(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
            return Task::none();
        };
        let parent = archive
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = archive
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive".into());
        let tops: std::collections::BTreeSet<&str> = self
            .entries
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
        let dest = if tops.len() == 1 {
            parent
        } else {
            parent.join(stem)
        };
        self.start_extract(dest)
    }

    fn start_test(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
            return Task::none();
        };
        let (cancel, progress) = self.begin_busy(JobKind::Test);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(JobKind::Test, cancel, progress, move |cancel, progress| {
            worker::test_archive_job(archive, password, cancel, progress, lang)
        })
    }

    fn start_add(&mut self, inputs: Vec<PathBuf>) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            return Task::none();
        };
        let opts = self.create_options();
        let (cancel, progress) = self.begin_busy(JobKind::Add);
        let lang = self.lang;
        spawn_job(JobKind::Add, cancel, progress, move |cancel, progress| {
            worker::add_files(archive, inputs, opts, cancel, progress, lang)
        })
    }

    fn start_delete(&mut self, index: usize) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            return Task::none();
        };
        let Some(entry) = self.entries.get(index).cloned() else {
            return Task::none();
        };
        let (cancel, progress) = self.begin_busy(JobKind::Delete);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(
            JobKind::Delete,
            cancel,
            progress,
            move |cancel, progress| {
                worker::delete_files(
                    archive,
                    vec![entry.name],
                    password,
                    cancel,
                    progress,
                    lang,
                )
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
                    self.archive_format = oxizip_engine::detect(&path).ok();
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
            JobOutcome::Tested { path, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    Task::none()
                } else {
                    self.last_error = None;
                    self.status = format!(
                        "{} {}",
                        t(self.lang, "gui.status.tested"),
                        path.display()
                    );
                    Task::none()
                }
            }
            JobOutcome::Edited { path, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    Task::none()
                } else {
                    self.last_error = None;
                    self.status = format!(
                        "{} {}",
                        t(self.lang, "gui.status.edited"),
                        path.display()
                    );
                    self.start_list(path)
                }
            }
            JobOutcome::Canceled => {
                self.status = t(self.lang, "gui.status.canceled").into();
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::Main => self.view_main(),
            Screen::Setup => self.view_setup(),
            Screen::Licenses => self.view_licenses(),
        }
    }

    fn view_setup(&self) -> Element<'_, Message> {
        let mut groups = Column::new().spacing(12);
        for (group, exts) in setup::assoc_catalog() {
            let mut chips = Column::new().spacing(4);
            // Wrap extensions in rows of ~4.
            let mut row_exts = row![].spacing(12).align_y(Alignment::Center);
            let mut count_in_row = 0usize;
            for ext in *exts {
                let checked = self.setup_exts.get(ext).copied().unwrap_or(false);
                let cell = row![
                    image(self.icons.for_ext(ext)).width(16).height(16),
                    checkbox(format!(".{ext}"), checked)
                        .on_toggle(move |on| Message::SetupExtToggled(ext, on)),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .width(Length::Fixed(110.0));
                row_exts = row_exts.push(cell);
                count_in_row += 1;
                if count_in_row >= 4 {
                    chips = chips.push(row_exts);
                    row_exts = row![].spacing(12).align_y(Alignment::Center);
                    count_in_row = 0;
                }
            }
            if count_in_row > 0 {
                chips = chips.push(row_exts);
            }
            groups = groups.push(
                column![
                    text(t(self.lang, group.i18n_key())).size(14),
                    chips,
                ]
                .spacing(6),
            );
        }

        let list = container(
            scrollable(
                column![
                    text(t(self.lang, "gui.setup.list")).size(13),
                    groups,
                ]
                .spacing(10)
                .padding(12),
            )
            .height(Length::Fill),
        )
        .width(Length::Fixed(520.0))
        .height(Length::Fill)
        .style(|theme: &iced::Theme| {
            let palette = theme.extended_palette();
            iced::widget::container::Style {
                background: Some(iced::Background::Color(palette.background.weak.color)),
                border: iced::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }
        });

        let err = self
            .last_error
            .as_ref()
            .map(|e| text(e.as_str()).size(13))
            .unwrap_or_else(|| text(" ").size(13));

        let actions = row![
            button(text(t(self.lang, "gui.setup.select_all")))
                .on_press(Message::SetupSelectAll),
            button(text(t(self.lang, "gui.setup.clear_all"))).on_press(Message::SetupClearAll),
            Space::with_width(Length::Fill),
            button(text(t(self.lang, "gui.setup.skip"))).on_press(Message::SetupSkip),
            button(text(t(self.lang, "gui.setup.apply")).size(16))
                .padding([10, 18])
                .on_press(Message::SetupConfirm)
                .style(iced::widget::button::primary),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fixed(520.0));

        let body = column![
            row![
                image(self.icons.oxizip()).width(40).height(40),
                column![
                    text(t(self.lang, "gui.setup.title")).size(22),
                    text(t(self.lang, "gui.setup.intro")).size(13),
                ]
                .spacing(4),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .width(Length::Fixed(520.0)),
            list,
            err,
            actions,
        ]
        .spacing(12)
        .padding(24)
        .align_x(Alignment::Center);

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_licenses(&self) -> Element<'_, Message> {
        let filters: Vec<LicenseFilter> = LicenseFilter::all().to_vec();

        let toolbar = row![
            button(t(self.lang, "gui.licenses.back")).on_press(Message::HideLicenses),
            Space::with_width(16),
            text(t(self.lang, "gui.licenses.title")).size(20),
            Space::with_width(Length::Fill),
            text(t(self.lang, "gui.licenses.filter")),
            pick_list(
                filters,
                Some(self.license_filter),
                Message::LicenseFilterSelected,
            )
            .width(160),
            Space::with_width(8),
            text(self.license_filter.label(self.lang)).size(13),
        ]
        .padding(8)
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let intro = text(t(self.lang, "gui.licenses.intro")).size(13);

        let header = row![
            text(t(self.lang, "gui.licenses.col.kind")).width(Length::FillPortion(2)),
            text(t(self.lang, "gui.licenses.col.name")).width(Length::FillPortion(4)),
            text(t(self.lang, "gui.licenses.col.license")).width(Length::FillPortion(3)),
            text(t(self.lang, "gui.licenses.col.purpose")).width(Length::FillPortion(4)),
            text(t(self.lang, "gui.licenses.col.note")).width(Length::FillPortion(4)),
        ]
        .padding([4, 8])
        .spacing(8);

        let mut list_col = Column::new().spacing(4).padding(4);
        for item in catalog()
            .iter()
            .filter(|c| self.license_filter.matches(c.kind))
        {
            list_col = list_col.push(
                container(
                    row![
                        text(item.kind.label(self.lang)).width(Length::FillPortion(2)),
                        text(item.name).width(Length::FillPortion(4)),
                        text(item.license).width(Length::FillPortion(3)),
                        text(item.purpose(self.lang)).width(Length::FillPortion(4)),
                        text(item.note(self.lang)).width(Length::FillPortion(4)),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
                .padding(6),
            );
        }

        let body = column![
            toolbar,
            container(intro).padding(iced::Padding {
                top: 0.0,
                right: 12.0,
                bottom: 8.0,
                left: 12.0,
            }),
            header,
            container(scrollable(list_col)).height(Length::Fill),
            container(text(t(self.lang, "gui.licenses.footer")).size(12)).padding(8),
        ]
        .spacing(4);

        container(body).width(Length::Fill).height(Length::Fill).into()
    }

    fn view_main(&self) -> Element<'_, Message> {
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
        let extract_here_btn = {
            let b = button(t(self.lang, "gui.extract_here"));
            if busy || self.archive.is_none() {
                b
            } else {
                b.on_press(Message::ExtractHereClicked)
            }
        };
        let extract_smart_btn = {
            let b = button(t(self.lang, "gui.extract_smart"));
            if busy || self.archive.is_none() {
                b
            } else {
                b.on_press(Message::ExtractSmartClicked)
            }
        };
        let test_btn = {
            let b = button(t(self.lang, "gui.test"));
            if busy || self.archive.is_none() {
                b
            } else {
                b.on_press(Message::TestClicked)
            }
        };
        let add_btn = {
            let b = button(t(self.lang, "gui.add"));
            if busy || self.archive.is_none() {
                b
            } else {
                b.on_press(Message::AddClicked)
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
            extract_here_btn,
            Space::with_width(6),
            extract_smart_btn,
            Space::with_width(6),
            test_btn,
            Space::with_width(6),
            add_btn,
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
            button(t(self.lang, "gui.setup.menu")).on_press(Message::ShowSetup),
            Space::with_width(8),
            button(t(self.lang, "gui.licenses")).on_press(Message::ShowLicenses),
            Space::with_width(8),
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
            Space::with_width(22),
            text(t(self.lang, "gui.col.name")).width(Length::FillPortion(6)),
            text(t(self.lang, "gui.col.size")).width(Length::FillPortion(2)),
            text(t(self.lang, "gui.col.method")).width(Length::FillPortion(2)),
            text(t(self.lang, "gui.col.action")).width(Length::FillPortion(2)),
        ]
        .padding([4, 8])
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let mut list_col = Column::new().spacing(2).padding(4);
        for (i, entry) in self.entries.iter().enumerate() {
            let name = if entry.is_dir {
                format!("[DIR] {}", entry.name)
            } else {
                entry.name.clone()
            };
            let entry_icon = if entry.is_dir {
                self.icons.oxizip()
            } else {
                self.icons.for_path_name(&entry.name)
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
            let del_btn = {
                let b = button(t(self.lang, "gui.delete_entry"));
                if busy {
                    b
                } else {
                    b.on_press(Message::DeleteEntry(i))
                }
            };
            list_col = list_col.push(
                row![
                    image(entry_icon).width(16).height(16),
                    text(name).width(Length::FillPortion(5)),
                    text(size).width(Length::FillPortion(2)),
                    text(method).width(Length::FillPortion(2)),
                    open_btn.width(Length::FillPortion(1)),
                    del_btn.width(Length::FillPortion(1)),
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

        let status_icon = match self.archive_format {
            Some(fmt) => self.icons.for_format(fmt),
            None => self.icons.oxizip(),
        };
        let status_row = row![
            image(status_icon).width(20).height(20),
            text(self.status.clone()).size(15),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let content = column![
            toolbar,
            status_row,
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
                "iso", "zpaq", "cab", "wim", "esd", "swm", "alz", "egg", "001",
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
