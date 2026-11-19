//! Oxizip iced GUI — list / extract / create; open via system default app.

// Release builds: no console window on Windows. Keep console in debug for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icons;
mod licenses;
mod menu;
mod prefs;
mod setup;
mod shell_ops;
mod worker;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iced::gradient::Linear;
use iced::widget::{
    button, checkbox, column, container, image, pick_list, progress_bar, row, scrollable, stack,
    text, text_input, Column, Space,
};
use iced::{
    window, Alignment, Background, Border, Color, Element, Gradient, Length, Point, Radians, Size,
    Subscription, Task, Theme,
};
use icons::IconSet;
use licenses::{catalog, ComponentKind};
use menu::{MenuAction, MenuPopup};
use oxizip_engine::{CreateOptions, EntryMeta, FormatId, MethodId, PasswordRecoverOptions};
use oxizip_i18n::{t, Lang};
use prefs::{ColorAccent, ListViewMode, Prefs, ToolbarSize};
use std::collections::BTreeMap;
use tempfile::TempDir;
use worker::{spawn_job, JobKind, JobOutcome, ProgressSlot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Screen {
    #[default]
    Main,
    Setup,
    Licenses,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Overlay {
    #[default]
    None,
    About,
    Comment,
    Rename,
    PasswordManager,
    Recover,
    Checksum,
    Toolbar,
    Color,
    Find,
    ConfirmDelete(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolbarFlag {
    Close,
    Open,
    Extract,
    ExtractHere,
    ExtractSmart,
    Test,
    Add,
    Create,
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

/// Single size for all GUI screens (association, main, licenses, …).
const APP_WINDOW_SIZE: Size = Size::new(775.0, 480.0);
const CONTROL_RADIUS: f32 = 8.0;

fn center_on_monitor(size: Size, monitor: Size) -> Point {
    Point::new(
        ((monitor.width - size.width) / 2.0).max(0.0),
        ((monitor.height - size.height) / 2.0).max(0.0),
    )
}

fn main() -> iced::Result {
    let mut window = iced::window::Settings {
        size: APP_WINDOW_SIZE,
        // `Centered` is unreliable on Windows; compute from monitor size.
        position: window::Position::SpecificWith(center_on_monitor),
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
    menu_popup: MenuPopup,
    prefs: Prefs,
    selected: BTreeSet<usize>,
    find_filter: String,
    archive_comment: String,
    overlay: Overlay,
    dialog_text: String,
    dialog_result: String,
    recover_wordlist: String,
    recover_max_len: String,
    tree_prefix: Option<String>,
    clipboard_temp: Option<Arc<TempDir>>,
    pending_repair: Option<PathBuf>,
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
            menu_popup: MenuPopup::None,
            prefs: Prefs::default(),
            selected: BTreeSet::new(),
            find_filter: String::new(),
            archive_comment: String::new(),
            overlay: Overlay::None,
            dialog_text: String::new(),
            dialog_result: String::new(),
            recover_wordlist: String::new(),
            recover_max_len: "4".into(),
            tree_prefix: None,
            clipboard_temp: None,
            pending_repair: None,
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
    CloseArchive,
    CancelClicked,
    ToggleMenu(MenuPopup),
    MenuAction(MenuAction),
    DismissOverlay,
    DialogTextChanged(String),
    CommentSave,
    RenameConfirm,
    PassmanAdd,
    PassmanRemove(usize),
    PassmanUse(usize),
    RecoverStart,
    RecoverMaxChanged(String),
    PickWordlist,
    WordlistChosen(Option<PathBuf>),
    FindApply,
    FindClear,
    SettingsBack,
    SettingsFormat(FormatLabel),
    SettingsMethod(MethodLabel),
    SettingsOpenDest(bool),
    ToolbarFlagToggled(ToolbarFlag, bool),
    ColorPicked(ColorAccent),
    ToggleSelect(usize),
    ToggleSelectAll,
    SetTreePrefix(Option<String>),
    SaveAsChosen(Option<PathBuf>),
    SaveAsFinished(Result<PathBuf, String>),
    RepairSourceChosen(Option<PathBuf>),
    RepairDestChosen(Option<PathBuf>),
    ConfirmDeleteArchive,
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
    HideLicenses,
    LicenseFilterSelected(LicenseFilter),
    SetupExtToggled(&'static str, bool),
    SetupToggleSelectAll,
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
        let prefs = Prefs::load();
        let lang = prefs.lang_enum();
        let create_format = prefs.default_format_id();
        let create_method = method_label_from_prefs(&prefs);
        let always_on_top = prefs.always_on_top;
        let mut app = Self {
            lang,
            prefs,
            status: t(lang, "gui.placeholder").to_string(),
            create_format,
            create_method,
            screen,
            pending_open,
            ..Self::default()
        };
        let open_task = if app.screen == Screen::Main {
            app.take_pending_open_task()
        } else {
            Task::none()
        };
        let top_task = if always_on_top {
            apply_window_level(true)
        } else {
            Task::none()
        };
        (app, Task::batch([open_task, top_task]))
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
            JobKind::Rename => t(self.lang, "gui.busy.rename"),
            JobKind::Repair => t(self.lang, "gui.busy.repair"),
            JobKind::Checksum => t(self.lang, "gui.busy.checksum"),
            JobKind::RecoverPassword => t(self.lang, "gui.busy.recover"),
            JobKind::WriteComment => t(self.lang, "gui.busy.comment"),
            JobKind::ExtractForClipboard => t(self.lang, "gui.busy.copy"),
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
            Message::CloseArchive => {
                if self.is_busy() {
                    return Task::none();
                }
                self.clear_archive();
                Task::none()
            }
            Message::CancelClicked => {
                if let Some(busy) = &self.busy {
                    busy.cancel.cancel();
                }
                Task::none()
            }
            Message::ToggleMenu(target) => {
                self.menu_popup = if self.menu_popup.root() == target {
                    MenuPopup::None
                } else {
                    target
                };
                Task::none()
            }
            Message::MenuAction(action) => self.handle_menu(action),
            Message::DismissOverlay => {
                self.overlay = Overlay::None;
                self.dialog_result.clear();
                Task::none()
            }
            Message::DialogTextChanged(value) => {
                self.dialog_text = value;
                Task::none()
            }
            Message::CommentSave => self.start_write_comment(),
            Message::RenameConfirm => self.start_rename(),
            Message::PassmanAdd => {
                let pw = self.dialog_text.trim().to_string();
                if !pw.is_empty() && !self.prefs.passwords.iter().any(|p| p == &pw) {
                    self.prefs.passwords.push(pw);
                    self.prefs.save();
                }
                self.dialog_text.clear();
                Task::none()
            }
            Message::PassmanRemove(index) => {
                if index < self.prefs.passwords.len() {
                    self.prefs.passwords.remove(index);
                    self.prefs.save();
                }
                Task::none()
            }
            Message::PassmanUse(index) => {
                if let Some(pw) = self.prefs.passwords.get(index) {
                    self.password = pw.clone();
                    self.overlay = Overlay::None;
                    self.status = t(self.lang, "gui.password").into();
                }
                Task::none()
            }
            Message::RecoverStart => self.start_recover(),
            Message::RecoverMaxChanged(value) => {
                self.recover_max_len = value;
                Task::none()
            }
            Message::PickWordlist => Task::perform(pick_wordlist(), Message::WordlistChosen),
            Message::WordlistChosen(path) => {
                self.recover_wordlist = path.map(|p| p.display().to_string()).unwrap_or_default();
                Task::none()
            }
            Message::FindApply => {
                self.find_filter = self.dialog_text.trim().to_string();
                self.overlay = Overlay::None;
                Task::none()
            }
            Message::FindClear => {
                self.find_filter.clear();
                self.dialog_text.clear();
                self.overlay = Overlay::None;
                Task::none()
            }
            Message::SettingsBack => {
                self.screen = Screen::Main;
                Task::none()
            }
            Message::SettingsFormat(label) => {
                self.create_format = label.0;
                self.prefs.set_default_format(label.0);
                self.prefs.save();
                Task::none()
            }
            Message::SettingsMethod(label) => {
                self.create_method = label;
                match label.to_option() {
                    Some(method) => self.prefs.set_default_method(method),
                    None => self.prefs.set_default_method_auto(),
                }
                self.prefs.save();
                Task::none()
            }
            Message::SettingsOpenDest(on) => {
                self.prefs.open_dest_after_extract = on;
                self.prefs.save();
                Task::none()
            }
            Message::ToolbarFlagToggled(flag, on) => {
                match flag {
                    ToolbarFlag::Close => self.prefs.toolbar_buttons.close = on,
                    ToolbarFlag::Open => self.prefs.toolbar_buttons.open = on,
                    ToolbarFlag::Extract => self.prefs.toolbar_buttons.extract = on,
                    ToolbarFlag::ExtractHere => self.prefs.toolbar_buttons.extract_here = on,
                    ToolbarFlag::ExtractSmart => self.prefs.toolbar_buttons.extract_smart = on,
                    ToolbarFlag::Test => self.prefs.toolbar_buttons.test = on,
                    ToolbarFlag::Add => self.prefs.toolbar_buttons.add = on,
                    ToolbarFlag::Create => self.prefs.toolbar_buttons.create = on,
                }
                self.prefs.save();
                Task::none()
            }
            Message::ColorPicked(accent) => {
                self.prefs.color_accent = accent;
                self.prefs.save();
                self.overlay = Overlay::None;
                Task::none()
            }
            Message::ToggleSelect(index) => {
                if !self.selected.insert(index) {
                    self.selected.remove(&index);
                }
                Task::none()
            }
            Message::ToggleSelectAll => {
                let visible: Vec<usize> = self.visible_indices();
                if visible.iter().all(|i| self.selected.contains(i)) {
                    for i in visible {
                        self.selected.remove(&i);
                    }
                } else {
                    self.selected.extend(visible);
                }
                Task::none()
            }
            Message::SetTreePrefix(prefix) => {
                self.tree_prefix = prefix;
                Task::none()
            }
            Message::SaveAsChosen(Some(dest)) => {
                let Some(src) = self.archive.clone() else {
                    return Task::none();
                };
                if src == dest {
                    self.last_error = Some(t(self.lang, "gui.err.same_path").into());
                    return Task::none();
                }
                Task::perform(copy_file(src, dest), Message::SaveAsFinished)
            }
            Message::SaveAsChosen(None) => Task::none(),
            Message::SaveAsFinished(Ok(dest)) => {
                self.last_error = None;
                self.status = format!("{} {}", t(self.lang, "gui.status.saved_as"), dest.display());
                self.prefs.push_recent(&dest);
                self.prefs.save();
                self.start_list(dest)
            }
            Message::SaveAsFinished(Err(err)) => {
                self.last_error = Some(format!("{}: {err}", t(self.lang, "gui.err.shell")));
                Task::none()
            }
            Message::RepairSourceChosen(Some(path)) => {
                self.pending_repair = Some(path.clone());
                Task::perform(pick_repair_output(path), Message::RepairDestChosen)
            }
            Message::RepairSourceChosen(None) => Task::none(),
            Message::RepairDestChosen(Some(dest)) => {
                let Some(src) = self.pending_repair.take() else {
                    return Task::none();
                };
                self.start_repair(src, Some(dest))
            }
            Message::RepairDestChosen(None) => {
                self.pending_repair = None;
                Task::none()
            }
            Message::ConfirmDeleteArchive => {
                let permanent = matches!(self.overlay, Overlay::ConfirmDelete(true));
                self.overlay = Overlay::None;
                self.delete_open_archive(permanent)
            }
            Message::FormatSelected(label) => {
                self.create_format = label.0;
                Task::none()
            }
            Message::MethodSelected(label) => {
                self.create_method = label;
                Task::none()
            }
            Message::LangSelected(label) => self.set_lang(label.to_lang()),
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
            Message::HideLicenses => {
                self.screen = Screen::Main;
                Task::none()
            }
            Message::SetupExtToggled(ext, on) => {
                if let Some(slot) = self.setup_exts.get_mut(ext) {
                    *slot = on;
                }
                Task::none()
            }
            Message::SetupToggleSelectAll => {
                let all_on = self.setup_all_selected();
                for on in self.setup_exts.values_mut() {
                    *on = !all_on;
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
                worker::delete_files(archive, vec![entry.name], password, cancel, progress, lang)
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
        spawn_job(
            JobKind::Create,
            cancel,
            progress,
            move |cancel, progress| {
                worker::create_archive(out, format, inputs, opts, cancel, progress, lang)
            },
        )
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
                    self.archive_comment =
                        oxizip_engine::read_archive_comment(&path).unwrap_or_default();
                    self.archive = Some(path.clone());
                    self.entries = entries;
                    self.selected.clear();
                    self.tree_prefix = None;
                    self.last_error = None;
                    self.prefs.push_recent(&path);
                    self.prefs.save();
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
                    if self.prefs.open_dest_after_extract {
                        if let Err(err) = shell_ops::open_folder(&dest) {
                            self.last_error =
                                Some(format!("{}: {err}", t(self.lang, "gui.err.shell")));
                        }
                    }
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
            JobOutcome::Opened { name, temp, error } => {
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
                    self.status =
                        format!("{} {}", t(self.lang, "gui.status.tested"), path.display());
                    Task::none()
                }
            }
            JobOutcome::Edited { path, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    Task::none()
                } else {
                    self.last_error = None;
                    self.status =
                        format!("{} {}", t(self.lang, "gui.status.edited"), path.display());
                    self.start_list(path)
                }
            }
            JobOutcome::Renamed { path, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    Task::none()
                } else {
                    self.last_error = None;
                    self.overlay = Overlay::None;
                    self.status =
                        format!("{} {}", t(self.lang, "gui.status.renamed"), path.display());
                    self.start_list(path)
                }
            }
            JobOutcome::Repaired { path, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    Task::none()
                } else {
                    self.last_error = None;
                    self.status =
                        format!("{} {}", t(self.lang, "gui.status.repaired"), path.display());
                    self.prefs.push_recent(&path);
                    self.prefs.save();
                    self.start_list(path)
                }
            }
            JobOutcome::Checksummed { report, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    self.dialog_result.clear();
                } else {
                    self.last_error = None;
                    self.dialog_result = report;
                    self.overlay = Overlay::Checksum;
                    self.status = t(self.lang, "gui.status.checksum").into();
                }
                Task::none()
            }
            JobOutcome::PasswordRecovered { password, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    self.dialog_result.clear();
                } else {
                    self.last_error = None;
                    self.password = password.clone();
                    if !password.is_empty() && !self.prefs.passwords.iter().any(|p| p == &password)
                    {
                        self.prefs.passwords.push(password.clone());
                        self.prefs.save();
                    }
                    self.dialog_result = if password.is_empty() {
                        t(self.lang, "gui.password").into()
                    } else {
                        password
                    };
                    self.overlay = Overlay::Recover;
                    self.status = t(self.lang, "gui.status.recovered").into();
                }
                Task::none()
            }
            JobOutcome::CommentWritten {
                path,
                comment,
                error,
            } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                } else {
                    self.last_error = None;
                    self.archive_comment = comment;
                    self.overlay = Overlay::None;
                    self.status =
                        format!("{} {}", t(self.lang, "gui.status.comment"), path.display());
                }
                Task::none()
            }
            JobOutcome::ClipboardFiles { paths, temp, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    self.clipboard_temp = None;
                } else {
                    match shell_ops::clipboard_set_files(&paths) {
                        Ok(()) => {
                            self.clipboard_temp = temp;
                            self.last_error = None;
                            self.status =
                                format!("{} ({})", t(self.lang, "gui.status.copied"), paths.len());
                        }
                        Err(err) => {
                            self.clipboard_temp = None;
                            self.last_error =
                                Some(format!("{}: {err}", t(self.lang, "gui.err.clipboard")));
                        }
                    }
                }
                Task::none()
            }
            JobOutcome::Canceled => {
                self.status = t(self.lang, "gui.status.canceled").into();
                Task::none()
            }
        }
    }

    fn clear_archive(&mut self) {
        self.archive = None;
        self.archive_format = None;
        self.entries.clear();
        self.selected.clear();
        self.archive_comment.clear();
        self.tree_prefix = None;
        self.open_temp = None;
        self.last_error = None;
        self.status = t(self.lang, "gui.placeholder").to_string();
    }

    fn set_lang(&mut self, lang: Lang) -> Task<Message> {
        self.lang = lang;
        self.prefs.set_lang(lang);
        self.prefs.save();
        self.menu_popup = MenuPopup::None;
        if self.archive.is_none() && self.busy.is_none() {
            self.status = t(self.lang, "gui.placeholder").to_string();
        }
        Task::none()
    }

    fn menu_ctx(&self) -> menu::MenuCtx<'_> {
        menu::MenuCtx {
            lang: self.lang,
            has_archive: self.archive.is_some(),
            busy: self.is_busy(),
            selection_count: self.selected.len(),
            is_zip_comment: self.supports_comment(),
            prefs: &self.prefs,
        }
    }

    fn supports_comment(&self) -> bool {
        matches!(
            self.archive_format,
            Some(FormatId::Zip | FormatId::Zipx | FormatId::ExeSfx)
        )
    }

    fn visible_indices(&self) -> Vec<usize> {
        let filter = self.find_filter.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry_visible(entry, self.tree_prefix.as_deref(), &filter))
            .map(|(index, _)| index)
            .collect()
    }

    fn selected_file_names(&self) -> Vec<String> {
        let selected: Vec<&EntryMeta> = self
            .selected
            .iter()
            .filter_map(|index| self.entries.get(*index))
            .collect();
        if selected.is_empty() {
            return Vec::new();
        }
        self.entries
            .iter()
            .filter(|entry| {
                selected.iter().any(|picked| entry_chosen(entry, picked)) && !entry.is_dir
            })
            .map(|entry| entry.name.clone())
            .collect()
    }

    fn shell_error(&mut self, err: String) {
        self.last_error = Some(format!("{}: {err}", t(self.lang, "gui.err.shell")));
    }

    fn handle_menu(&mut self, action: MenuAction) -> Task<Message> {
        let submenu = matches!(
            action,
            MenuAction::OpenFileRecent
                | MenuAction::OpenFileExtract
                | MenuAction::OpenSettingsLang
                | MenuAction::OpenViewToolbar
                | MenuAction::OpenViewList
                | MenuAction::OpenViewColumn
        );
        if !submenu {
            self.menu_popup = MenuPopup::None;
        }
        match action {
            MenuAction::Open => {
                if self.is_busy() {
                    return Task::none();
                }
                Task::perform(pick_archive(), Message::ArchiveChosen)
            }
            MenuAction::Recent(index) => {
                let Some(path) = self.prefs.recent.get(index).cloned() else {
                    return Task::none();
                };
                self.screen = Screen::Main;
                self.start_list(PathBuf::from(path))
            }
            MenuAction::ClearRecent => {
                self.prefs.clear_recent();
                self.prefs.save();
                Task::none()
            }
            MenuAction::Close => {
                if self.is_busy() {
                    return Task::none();
                }
                self.clear_archive();
                self.screen = Screen::Main;
                Task::none()
            }
            MenuAction::Create => {
                if self.is_busy() {
                    return Task::none();
                }
                self.screen = Screen::Main;
                Task::perform(pick_inputs(), Message::CreateInputsChosen)
            }
            MenuAction::NewWindow => {
                if let Err(err) = shell_ops::spawn_new_window() {
                    self.shell_error(err);
                }
                Task::none()
            }
            MenuAction::NewWindowAdmin => {
                if let Err(err) = shell_ops::spawn_new_window_admin() {
                    self.shell_error(err);
                }
                Task::none()
            }
            MenuAction::SaveAs => {
                let Some(src) = self.archive.clone() else {
                    self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
                    return Task::none();
                };
                let name = src
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "archive.zip".into());
                Task::perform(pick_save_as(name), Message::SaveAsChosen)
            }
            MenuAction::DeleteArchive => {
                self.overlay = Overlay::ConfirmDelete(false);
                Task::none()
            }
            MenuAction::DeleteArchivePermanent => {
                self.overlay = Overlay::ConfirmDelete(true);
                Task::none()
            }
            MenuAction::OpenArchiveFolder => {
                let Some(path) = self.archive.clone() else {
                    self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
                    return Task::none();
                };
                if let Err(err) = shell_ops::reveal_in_file_manager(&path) {
                    self.shell_error(err);
                }
                Task::none()
            }
            MenuAction::Test => self.start_test(),
            MenuAction::ExtractTo => {
                if self.is_busy() || self.archive.is_none() {
                    self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
                    return Task::none();
                }
                Task::perform(pick_folder(), Message::ExtractDirChosen)
            }
            MenuAction::ExtractHere => {
                let Some(archive) = self.archive.clone() else {
                    self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
                    return Task::none();
                };
                let dest = archive
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                self.start_extract(dest)
            }
            MenuAction::ExtractSmart => self.start_extract_smart(),
            MenuAction::Exit => iced::exit(),
            MenuAction::Rename => self.open_rename_dialog(),
            MenuAction::Add => {
                if self.is_busy() || self.archive.is_none() {
                    self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
                    return Task::none();
                }
                Task::perform(pick_inputs(), Message::AddInputsChosen)
            }
            MenuAction::DeleteEntries => self.start_delete_selected(),
            MenuAction::EditComment => {
                if !self.supports_comment() {
                    self.last_error = Some(t(self.lang, "error.unsupported_format").into());
                    return Task::none();
                }
                self.dialog_text = self.archive_comment.clone();
                self.overlay = Overlay::Comment;
                Task::none()
            }
            MenuAction::CopyFilesClipboard => self.start_copy_files(),
            MenuAction::PasteFilesClipboard => self.paste_files(),
            MenuAction::CopyFullPaths => self.copy_full_paths(),
            MenuAction::Find => {
                self.dialog_text = self.find_filter.clone();
                self.overlay = Overlay::Find;
                Task::none()
            }
            MenuAction::ShowSettings => {
                self.screen = Screen::Settings;
                Task::none()
            }
            MenuAction::LangZh => self.set_lang(Lang::ZhCn),
            MenuAction::LangEn => self.set_lang(Lang::En),
            MenuAction::StartScreen => {
                if self.is_busy() {
                    return Task::none();
                }
                self.clear_archive();
                self.screen = Screen::Main;
                Task::none()
            }
            MenuAction::ToggleTree => {
                self.prefs.show_tree = !self.prefs.show_tree;
                if !self.prefs.show_tree {
                    self.tree_prefix = None;
                }
                self.prefs.save();
                Task::none()
            }
            MenuAction::ToggleComment => {
                self.prefs.show_comment = !self.prefs.show_comment;
                self.prefs.save();
                Task::none()
            }
            MenuAction::ToggleStatusBar => {
                self.prefs.show_status_bar = !self.prefs.show_status_bar;
                self.prefs.save();
                Task::none()
            }
            MenuAction::ToolbarLarge => self.set_toolbar(ToolbarSize::Large),
            MenuAction::ToolbarSmall => self.set_toolbar(ToolbarSize::Small),
            MenuAction::ToolbarHide => self.set_toolbar(ToolbarSize::Hidden),
            MenuAction::ToolbarCustomize => {
                self.overlay = Overlay::Toolbar;
                Task::none()
            }
            MenuAction::ListIcons => self.set_list_view(ListViewMode::Icons),
            MenuAction::ListList => self.set_list_view(ListViewMode::List),
            MenuAction::ListDetails => self.set_list_view(ListViewMode::Details),
            MenuAction::ListLargeIcons => self.set_list_view(ListViewMode::LargeIcons),
            MenuAction::ToggleColSize => {
                self.prefs.columns.size = !self.prefs.columns.size;
                self.prefs.save();
                Task::none()
            }
            MenuAction::ToggleColCompressed => {
                self.prefs.columns.compressed = !self.prefs.columns.compressed;
                self.prefs.save();
                Task::none()
            }
            MenuAction::ToggleColMethod => {
                self.prefs.columns.method = !self.prefs.columns.method;
                self.prefs.save();
                Task::none()
            }
            MenuAction::ToggleColModified => {
                self.prefs.columns.modified = !self.prefs.columns.modified;
                self.prefs.save();
                Task::none()
            }
            MenuAction::ToggleAlwaysOnTop => {
                self.prefs.always_on_top = !self.prefs.always_on_top;
                self.prefs.save();
                apply_window_level(self.prefs.always_on_top)
            }
            MenuAction::ColorSettings => {
                self.overlay = Overlay::Color;
                Task::none()
            }
            MenuAction::Repair => {
                if self.is_busy() {
                    return Task::none();
                }
                if let Some(path) = self.archive.clone() {
                    self.pending_repair = Some(path.clone());
                    Task::perform(pick_repair_output(path), Message::RepairDestChosen)
                } else {
                    Task::perform(pick_archive(), Message::RepairSourceChosen)
                }
            }
            MenuAction::RecoverPassword => {
                self.dialog_result.clear();
                self.overlay = Overlay::Recover;
                Task::none()
            }
            MenuAction::Checksum => self.start_checksum(),
            MenuAction::About => {
                self.overlay = Overlay::About;
                Task::none()
            }
            MenuAction::OpenHelp => {
                let prefer_zh = self.lang == Lang::ZhCn;
                match shell_ops::find_readme(prefer_zh) {
                    Some(path) => {
                        if let Err(err) = open::that(&path) {
                            self.shell_error(err.to_string());
                        }
                    }
                    None => self.last_error = Some(t(self.lang, "gui.err.readme").into()),
                }
                Task::none()
            }
            MenuAction::OpenInstallFolder => {
                match std::env::current_exe()
                    .ok()
                    .and_then(|exe| exe.parent().map(Path::to_path_buf))
                {
                    Some(dir) => {
                        if let Err(err) = shell_ops::open_folder(&dir) {
                            self.shell_error(err);
                        }
                    }
                    None => self.last_error = Some(t(self.lang, "gui.err.shell").into()),
                }
                Task::none()
            }
            MenuAction::Licenses => {
                self.screen = Screen::Licenses;
                Task::none()
            }
            MenuAction::PasswordManager => {
                self.dialog_text.clear();
                self.overlay = Overlay::PasswordManager;
                Task::none()
            }
            MenuAction::Associations => {
                self.screen = Screen::Setup;
                self.setup_exts = setup::default_assoc_selection();
                self.last_error = None;
                Task::none()
            }
            MenuAction::OpenSettingsLang => {
                self.menu_popup = if self.menu_popup == MenuPopup::SettingsLang {
                    MenuPopup::Settings
                } else {
                    MenuPopup::SettingsLang
                };
                Task::none()
            }
            MenuAction::OpenFileRecent => {
                self.menu_popup = if self.menu_popup == MenuPopup::FileRecent {
                    MenuPopup::File
                } else {
                    MenuPopup::FileRecent
                };
                Task::none()
            }
            MenuAction::OpenFileExtract => {
                self.menu_popup = if self.menu_popup == MenuPopup::FileExtract {
                    MenuPopup::File
                } else {
                    MenuPopup::FileExtract
                };
                Task::none()
            }
            MenuAction::OpenViewToolbar => {
                self.menu_popup = if self.menu_popup == MenuPopup::ViewToolbar {
                    MenuPopup::View
                } else {
                    MenuPopup::ViewToolbar
                };
                Task::none()
            }
            MenuAction::OpenViewList => {
                self.menu_popup = if self.menu_popup == MenuPopup::ViewList {
                    MenuPopup::View
                } else {
                    MenuPopup::ViewList
                };
                Task::none()
            }
            MenuAction::OpenViewColumn => {
                self.menu_popup = if self.menu_popup == MenuPopup::ViewColumn {
                    MenuPopup::View
                } else {
                    MenuPopup::ViewColumn
                };
                Task::none()
            }
        }
    }

    fn set_toolbar(&mut self, size: ToolbarSize) -> Task<Message> {
        self.prefs.toolbar = size;
        self.prefs.save();
        Task::none()
    }

    fn set_list_view(&mut self, mode: ListViewMode) -> Task<Message> {
        self.prefs.list_view = mode;
        self.prefs.save();
        Task::none()
    }

    fn open_rename_dialog(&mut self) -> Task<Message> {
        if self.selected.len() != 1 {
            self.last_error = Some(t(self.lang, "gui.err.one_selection").into());
            return Task::none();
        }
        let index = *self.selected.iter().next().expect("selection length is 1");
        let Some(entry) = self.entries.get(index) else {
            return Task::none();
        };
        self.dialog_text = entry.name.clone();
        self.overlay = Overlay::Rename;
        Task::none()
    }

    fn delete_open_archive(&mut self, permanent: bool) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(path) = self.archive.clone() else {
            self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
            return Task::none();
        };
        let result = if permanent {
            shell_ops::delete_file_permanent(&path)
        } else {
            shell_ops::trash_file(&path)
        };
        match result {
            Ok(()) => {
                self.clear_archive();
                self.status = t(self.lang, "gui.status.deleted_archive").into();
            }
            Err(err) => self.shell_error(err),
        }
        Task::none()
    }

    fn paste_files(&mut self) -> Task<Message> {
        if self.is_busy() || self.archive.is_none() {
            self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
            return Task::none();
        }
        match shell_ops::clipboard_file_paths() {
            Ok(paths) => self.start_add(paths),
            Err(err) => {
                self.last_error = Some(format!("{}: {err}", t(self.lang, "gui.err.clipboard")));
                Task::none()
            }
        }
    }

    fn copy_full_paths(&mut self) -> Task<Message> {
        let Some(archive) = self.archive.clone() else {
            self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
            return Task::none();
        };
        if self.selected.is_empty() {
            self.last_error = Some(t(self.lang, "gui.err.need_selection").into());
            return Task::none();
        }
        let mut lines = Vec::new();
        for index in &self.selected {
            if let Some(entry) = self.entries.get(*index) {
                lines.push(format!(
                    "{}{}{}",
                    archive.display(),
                    std::path::MAIN_SEPARATOR,
                    entry.name
                ));
            }
        }
        match shell_ops::clipboard_set_text(&lines.join("\n")) {
            Ok(()) => {
                self.last_error = None;
                self.status = t(self.lang, "gui.status.copied").into();
            }
            Err(err) => {
                self.last_error = Some(format!("{}: {err}", t(self.lang, "gui.err.clipboard")));
            }
        }
        Task::none()
    }

    fn start_delete_selected(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            return Task::none();
        };
        let names: Vec<String> = self
            .selected
            .iter()
            .filter_map(|index| self.entries.get(*index).map(|entry| entry.name.clone()))
            .collect();
        if names.is_empty() {
            self.last_error = Some(t(self.lang, "gui.err.need_selection").into());
            return Task::none();
        }
        let (cancel, progress) = self.begin_busy(JobKind::Delete);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(
            JobKind::Delete,
            cancel,
            progress,
            move |cancel, progress| {
                worker::delete_files(archive, names, password, cancel, progress, lang)
            },
        )
    }

    fn start_rename(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            return Task::none();
        };
        if self.selected.len() != 1 {
            self.last_error = Some(t(self.lang, "gui.err.one_selection").into());
            return Task::none();
        }
        let index = *self.selected.iter().next().expect("selection length is 1");
        let Some(from) = self.entries.get(index).map(|entry| entry.name.clone()) else {
            return Task::none();
        };
        let to = self.dialog_text.trim().to_string();
        if to.is_empty() {
            self.last_error = Some(t(self.lang, "gui.err.rename_empty").into());
            return Task::none();
        }
        if to == from {
            self.overlay = Overlay::None;
            return Task::none();
        }
        let (cancel, progress) = self.begin_busy(JobKind::Rename);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(
            JobKind::Rename,
            cancel,
            progress,
            move |cancel, progress| {
                worker::rename_entry_job(archive, from, to, password, cancel, progress, lang)
            },
        )
    }

    fn start_write_comment(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            return Task::none();
        };
        let comment = self.dialog_text.clone();
        let (cancel, progress) = self.begin_busy(JobKind::WriteComment);
        let lang = self.lang;
        spawn_job(
            JobKind::WriteComment,
            cancel,
            progress,
            move |cancel, progress| {
                worker::write_comment_job(archive, comment, cancel, progress, lang)
            },
        )
    }

    fn start_copy_files(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            return Task::none();
        };
        let names = self.selected_file_names();
        if names.is_empty() {
            self.last_error = Some(t(self.lang, "gui.err.clipboard_empty").into());
            return Task::none();
        }
        let (cancel, progress) = self.begin_busy(JobKind::ExtractForClipboard);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(
            JobKind::ExtractForClipboard,
            cancel,
            progress,
            move |cancel, progress| {
                worker::extract_for_clipboard_job(archive, names, password, cancel, progress, lang)
            },
        )
    }

    fn start_checksum(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
            return Task::none();
        };
        let names = self.selected_file_names();
        let (cancel, progress) = self.begin_busy(JobKind::Checksum);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(
            JobKind::Checksum,
            cancel,
            progress,
            move |cancel, progress| {
                worker::checksum_job(archive, names, password, cancel, progress, lang)
            },
        )
    }

    fn start_recover(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(archive) = self.archive.clone() else {
            self.last_error = Some(t(self.lang, "gui.err.need_archive").into());
            return Task::none();
        };
        let max_len = self.recover_max_len.trim().parse::<u32>().unwrap_or(4);
        let wordlist = {
            let path = self.recover_wordlist.trim();
            if path.is_empty() {
                None
            } else {
                Some(PathBuf::from(path))
            }
        };
        let opts = PasswordRecoverOptions {
            wordlist,
            max_len,
            ..PasswordRecoverOptions::default()
        };
        let (cancel, progress) = self.begin_busy(JobKind::RecoverPassword);
        let lang = self.lang;
        self.overlay = Overlay::None;
        spawn_job(
            JobKind::RecoverPassword,
            cancel,
            progress,
            move |cancel, progress| {
                worker::recover_password_job(archive, opts, cancel, progress, lang)
            },
        )
    }

    fn start_repair(&mut self, archive: PathBuf, dest: Option<PathBuf>) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let (cancel, progress) = self.begin_busy(JobKind::Repair);
        let password = self.password_opt();
        let lang = self.lang;
        spawn_job(
            JobKind::Repair,
            cancel,
            progress,
            move |cancel, progress| {
                worker::repair_job(archive, dest, password, cancel, progress, lang)
            },
        )
    }

    fn view(&self) -> Element<'_, Message> {
        let page = match self.screen {
            Screen::Main => self.view_main(),
            Screen::Setup => self.view_setup(),
            Screen::Licenses => self.view_licenses(),
            Screen::Settings => self.shell_page(self.view_settings()),
        };
        if self.overlay == Overlay::None {
            page
        } else {
            stack![page, self.view_overlay()]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    fn setup_all_selected(&self) -> bool {
        !self.setup_exts.is_empty() && self.setup_exts.values().all(|on| *on)
    }

    fn view_setup(&self) -> Element<'_, Message> {
        const CONTROL_RADIUS: f32 = 8.0;

        let mut groups = Column::new().spacing(12).width(Length::Fill);
        for (group, exts) in setup::assoc_catalog() {
            let mut chips = Column::new().spacing(4).width(Length::Fill);
            let mut row_exts = row![].spacing(8).align_y(Alignment::Center);
            let mut count_in_row = 0usize;
            for ext in *exts {
                let checked = self.setup_exts.get(ext).copied().unwrap_or(false);
                let cell = checkbox(format!(".{ext}"), checked)
                    .on_toggle(move |on| Message::SetupExtToggled(ext, on))
                    .width(Length::Fixed(88.0));
                row_exts = row_exts.push(cell);
                count_in_row += 1;
                if count_in_row >= 5 {
                    chips = chips.push(row_exts);
                    row_exts = row![].spacing(8).align_y(Alignment::Center);
                    count_in_row = 0;
                }
            }
            if count_in_row > 0 {
                chips = chips.push(row_exts);
            }
            groups = groups.push(
                column![text(t(self.lang, group.i18n_key())).size(13), chips,]
                    .spacing(6)
                    .width(Length::Fill)
                    .align_x(Alignment::Start),
            );
        }

        let list = container(
            scrollable(groups.padding(12))
                .height(Length::Fill)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme: &iced::Theme| {
            let palette = theme.extended_palette();
            iced::widget::container::Style {
                background: Some(iced::Background::Color(palette.background.weak.color)),
                border: iced::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: CONTROL_RADIUS.into(),
                },
                ..Default::default()
            }
        });

        let err = self
            .last_error
            .as_ref()
            .map(|e| text(e.as_str()).size(12))
            .unwrap_or_else(|| text(" ").size(12));

        let toggle_label = if self.setup_all_selected() {
            t(self.lang, "gui.setup.clear_all")
        } else {
            t(self.lang, "gui.setup.select_all")
        };
        // Bottom: toggle (left) · skip (plain) · OK (right)
        let actions = row![
            button(text(toggle_label).size(12))
                .padding([6, 12])
                .height(32)
                .on_press(Message::SetupToggleSelectAll)
                .style(iced::widget::button::secondary),
            Space::with_width(Length::Fill),
            button(text(t(self.lang, "gui.setup.skip")).size(12))
                .padding([6, 10])
                .height(32)
                .on_press(Message::SetupSkip)
                .style(iced::widget::button::text),
            button(text(t(self.lang, "gui.setup.apply")).size(13))
                .padding([6, 16])
                .height(32)
                .on_press(Message::SetupConfirm)
                .style(iced::widget::button::primary),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let body = column![
            text(t(self.lang, "gui.setup.title"))
                .size(16)
                .width(Length::Fill),
            text(t(self.lang, "gui.setup.intro"))
                .size(12)
                .width(Length::Fill),
            list,
            err,
            actions,
        ]
        .spacing(10)
        .padding(16)
        .align_x(Alignment::Start)
        .width(Length::Fill)
        .height(Length::Fill);

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

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_main(&self) -> Element<'_, Message> {
        let body = if self.archive.is_none() {
            self.view_home()
        } else {
            self.view_archive()
        };
        self.shell_page(body)
    }

    fn shell_page<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        let dropdown_layer = menu::menu_dropdown_overlay(self.menu_popup, &self.menu_ctx());
        let content = stack![
            body,
            column![dropdown_layer, Space::with_height(Length::Fill)]
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill);
        column![menu::menu_bar_row(self.lang, self.menu_popup), content]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Home landing — self-drawn empty state (no third-party assets).
    fn view_home(&self) -> Element<'_, Message> {
        let busy = self.is_busy();

        let open_tile = home_glass_tile(
            self.icons.home_open(),
            t(self.lang, "gui.home.open"),
            if busy {
                None
            } else {
                Some(Message::OpenClicked)
            },
        );
        let create_tile = home_glass_tile(
            self.icons.home_create(),
            t(self.lang, "gui.home.create"),
            if busy {
                None
            } else {
                Some(Message::CreateClicked)
            },
        );

        let tiles = row![
            Space::with_width(Length::Fill),
            open_tile,
            Space::with_width(18),
            create_tile,
            Space::with_width(Length::Fill),
        ]
        .align_y(Alignment::Center);

        let err = self
            .last_error
            .as_ref()
            .map(|e| {
                text(e.clone())
                    .size(12)
                    .style(|_| iced::widget::text::Style {
                        color: Some(Color::from_rgb8(0xFF, 0xE0, 0xE0)),
                    })
            })
            .unwrap_or_else(|| {
                text("").size(12).style(|_| iced::widget::text::Style {
                    color: Some(Color::WHITE),
                })
            });

        let progress_area: Element<'_, Message> = if busy {
            column![
                progress_bar(0.0..=1.0, self.progress_fraction).height(8),
                text(self.progress_detail.clone())
                    .size(11)
                    .style(|_| iced::widget::text::Style {
                        color: Some(Color::WHITE),
                    }),
                button(t(self.lang, "gui.cancel"))
                    .style(rounded_secondary)
                    .on_press(Message::CancelClicked),
            ]
            .spacing(6)
            .align_x(Alignment::Center)
            .into()
        } else {
            Space::with_height(0).into()
        };

        let stage = column![
            Space::with_height(24),
            Space::with_height(Length::Fill),
            tiles,
            Space::with_height(Length::Fill),
            err,
            progress_area,
            Space::with_height(14),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center);

        container(stage)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(home_stage_style(self.prefs.color_accent))
            .into()
    }

    fn view_archive(&self) -> Element<'_, Message> {
        let formats: Vec<FormatLabel> = FormatId::create_choices()
            .iter()
            .copied()
            .map(FormatLabel)
            .collect();
        let methods: Vec<MethodLabel> = MethodLabel::all().to_vec();
        let busy = self.is_busy();

        let toolbar = self.view_toolbar(busy);

        let options = row![
            text(t(self.lang, "gui.format")).size(12),
            pick_list(
                formats,
                Some(FormatLabel(self.create_format)),
                Message::FormatSelected,
            )
            .width(80),
            Space::with_width(8),
            text(t(self.lang, "gui.method")).size(12),
            pick_list(methods, Some(self.create_method), Message::MethodSelected,).width(90),
            Space::with_width(8),
            text(t(self.lang, "gui.password")).size(12),
            text_input("", &self.password)
                .secure(true)
                .on_input(Message::PasswordChanged)
                .width(100),
            Space::with_width(Length::Fill),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let err = self
            .last_error
            .as_ref()
            .map(|e| text(e.clone()).size(12))
            .unwrap_or_else(|| text("").size(12));

        let progress_row: Element<'_, Message> = if busy {
            column![
                progress_bar(0.0..=1.0, self.progress_fraction).height(8),
                text(self.progress_detail.clone()).size(11),
            ]
            .spacing(4)
            .into()
        } else {
            Space::with_height(0).into()
        };

        let status_icon = match self.archive_format {
            Some(fmt) => self.icons.for_format(fmt),
            None => self.icons.oxizip(),
        };
        let status_row = row![
            image(status_icon).width(18).height(18),
            text(self.status.clone()).size(12),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut workspace = column![]
            .spacing(6)
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill);
        if self.prefs.toolbar != ToolbarSize::Hidden {
            workspace = workspace.push(toolbar);
        }
        workspace = workspace.push(options);
        if self.prefs.show_comment && self.supports_comment() {
            workspace = workspace.push(self.view_comment_pane());
        }
        if !self.find_filter.is_empty() {
            workspace = workspace.push(
                text(format!(
                    "{}: {}",
                    t(self.lang, "gui.find.title"),
                    self.find_filter
                ))
                .size(12),
            );
        }
        if self.prefs.show_status_bar {
            workspace = workspace.push(status_row);
        }
        workspace = workspace
            .push(err)
            .push(progress_row)
            .push(self.view_entry_browser(busy));

        container(workspace)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_toolbar(&self, busy: bool) -> Element<'_, Message> {
        let label_size = if matches!(self.prefs.toolbar, ToolbarSize::Small) {
            11.0
        } else {
            13.0
        };
        let specs = [
            (
                self.prefs.toolbar_buttons.close,
                "gui.close_archive",
                Message::CloseArchive,
                false,
            ),
            (
                self.prefs.toolbar_buttons.open,
                "gui.open_archive",
                Message::OpenClicked,
                false,
            ),
            (
                self.prefs.toolbar_buttons.extract,
                "gui.extract",
                Message::ExtractClicked,
                true,
            ),
            (
                self.prefs.toolbar_buttons.extract_here,
                "gui.extract_here",
                Message::ExtractHereClicked,
                false,
            ),
            (
                self.prefs.toolbar_buttons.extract_smart,
                "gui.extract_smart",
                Message::ExtractSmartClicked,
                false,
            ),
            (
                self.prefs.toolbar_buttons.test,
                "gui.test",
                Message::TestClicked,
                false,
            ),
            (
                self.prefs.toolbar_buttons.add,
                "gui.add",
                Message::AddClicked,
                false,
            ),
            (
                self.prefs.toolbar_buttons.create,
                "gui.create",
                Message::CreateClicked,
                false,
            ),
            (true, "gui.cancel", Message::CancelClicked, false),
        ];
        let mut tools = row![].spacing(6).align_y(Alignment::Center);
        for (visible, key, msg, primary) in specs {
            let is_cancel = matches!(msg, Message::CancelClicked);
            if !visible && !is_cancel {
                continue;
            }
            let style = if primary {
                rounded_primary
            } else {
                rounded_secondary
            };
            let mut btn = button(text(t(self.lang, key)).size(label_size)).style(style);
            let enabled = if is_cancel { busy } else { !busy };
            if enabled {
                btn = btn.on_press(msg);
            }
            tools = tools.push(btn);
        }
        tools.into()
    }

    fn view_comment_pane(&self) -> Element<'_, Message> {
        let body = if self.archive_comment.is_empty() {
            t(self.lang, "gui.comment.empty")
        } else {
            self.archive_comment.as_str()
        };
        container(text(body).size(12))
            .padding(8)
            .width(Length::Fill)
            .style(list_box_style)
            .into()
    }

    fn view_entry_browser(&self, busy: bool) -> Element<'_, Message> {
        let indices = self.visible_indices();
        let list: Element<'_, Message> = match self.prefs.list_view {
            ListViewMode::Details => self.view_details(busy, &indices),
            ListViewMode::List => self.view_name_rows(busy, &indices, 16.0, false),
            ListViewMode::Icons => self.view_icon_grid(busy, &indices, 32.0, 4),
            ListViewMode::LargeIcons => self.view_icon_grid(busy, &indices, 48.0, 3),
        };
        let list_box = container(scrollable(list).height(Length::Fill))
            .style(list_box_style)
            .padding(4)
            .width(Length::Fill)
            .height(Length::Fill);
        if self.prefs.show_tree {
            row![self.view_tree(), list_box]
                .spacing(8)
                .height(Length::Fill)
                .into()
        } else {
            list_box.into()
        }
    }

    fn view_tree(&self) -> Element<'_, Message> {
        let mut folders = BTreeSet::new();
        for entry in &self.entries {
            if let Some(folder) = top_folder(&entry.name, entry.is_dir) {
                folders.insert(folder);
            }
        }
        let all_style = rounded_text_btn;
        let mut col = column![button(text(t(self.lang, "gui.tree.all")).size(12))
            .style(all_style)
            .on_press(Message::SetTreePrefix(None))]
        .spacing(2)
        .padding(4);
        for folder in folders {
            let label = folder.clone();
            col = col.push(
                button(text(label.clone()).size(12))
                    .style(rounded_text_btn)
                    .on_press(Message::SetTreePrefix(Some(label))),
            );
        }
        container(scrollable(col).height(Length::Fill))
            .width(Length::Fixed(140.0))
            .height(Length::Fill)
            .style(list_box_style)
            .into()
    }

    fn view_details(&self, busy: bool, indices: &[usize]) -> Element<'_, Message> {
        let accent = accent_color(self.prefs.color_accent);
        let mut header = row![
            checkbox(
                "",
                indices.iter().all(|i| self.selected.contains(i)) && !indices.is_empty()
            )
            .on_toggle(|_| Message::ToggleSelectAll),
            text(t(self.lang, "gui.col.name"))
                .size(12)
                .width(Length::Fill),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if self.prefs.columns.size {
            header = header.push(
                text(t(self.lang, "gui.col.size"))
                    .size(12)
                    .width(Length::Fixed(72.0)),
            );
        }
        if self.prefs.columns.compressed {
            header = header.push(
                text(t(self.lang, "gui.col.compressed"))
                    .size(12)
                    .width(Length::Fixed(72.0)),
            );
        }
        if self.prefs.columns.method {
            header = header.push(
                text(t(self.lang, "gui.col.method"))
                    .size(12)
                    .width(Length::Fixed(64.0)),
            );
        }
        if self.prefs.columns.modified {
            header = header.push(
                text(t(self.lang, "gui.col.modified"))
                    .size(12)
                    .width(Length::Fixed(118.0)),
            );
        }
        header = header.push(Space::with_width(Length::Fixed(92.0)));
        let mut col =
            column![container(header)
                .padding([4, 6])
                .width(Length::Fill)
                .style(move |theme| {
                    let mut style = list_header_style(theme);
                    style.border.color = accent;
                    style
                })]
            .spacing(2);
        for index in indices {
            if let Some(entry) = self.entries.get(*index) {
                col = col.push(self.detail_row(*index, entry, busy));
            }
        }
        col.into()
    }

    fn detail_row(&self, index: usize, entry: &EntryMeta, busy: bool) -> Element<'_, Message> {
        let selected = self.selected.contains(&index);
        let name = if entry.is_dir {
            format!("[DIR] {}", entry.name)
        } else {
            entry.name.clone()
        };
        let icon = if entry.is_dir {
            self.icons.oxizip()
        } else {
            self.icons.for_path_name(&entry.name)
        };
        let mut line = row![
            checkbox("", selected).on_toggle(move |_| Message::ToggleSelect(index)),
            image(icon).width(16).height(16),
            text(name).size(12).width(Length::Fill),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if self.prefs.columns.size {
            let size = entry
                .uncompressed_size
                .map(human_size)
                .unwrap_or_else(|| "-".into());
            line = line.push(text(size).size(12).width(Length::Fixed(72.0)));
        }
        if self.prefs.columns.compressed {
            let size = entry
                .compressed_size
                .map(human_size)
                .unwrap_or_else(|| "-".into());
            line = line.push(text(size).size(12).width(Length::Fixed(72.0)));
        }
        if self.prefs.columns.method {
            let method = entry.method.clone().unwrap_or_else(|| "-".into());
            line = line.push(text(method).size(12).width(Length::Fixed(64.0)));
        }
        if self.prefs.columns.modified {
            line = line.push(
                text(fmt_modified(entry.modified))
                    .size(11)
                    .width(Length::Fixed(118.0)),
            );
        }
        let open_btn = {
            let b = button(text(t(self.lang, "gui.open_entry")).size(12)).style(rounded_text_btn);
            if busy || entry.is_dir {
                b
            } else {
                b.on_press(Message::OpenEntry(index))
            }
        };
        let del_btn = {
            let b = button(text(t(self.lang, "gui.delete_entry")).size(12)).style(rounded_text_btn);
            if busy {
                b
            } else {
                b.on_press(Message::DeleteEntry(index))
            }
        };
        line = line.push(open_btn).push(del_btn);
        container(line)
            .padding([2, 4])
            .width(Length::Fill)
            .style(move |_theme| row_style(selected))
            .into()
    }

    fn view_name_rows(
        &self,
        busy: bool,
        indices: &[usize],
        icon_px: f32,
        show_icon: bool,
    ) -> Element<'_, Message> {
        let _ = busy;
        let mut col = column![].spacing(2);
        for index in indices {
            let Some(entry) = self.entries.get(*index) else {
                continue;
            };
            let selected = self.selected.contains(index);
            let mut line = row![checkbox("", selected).on_toggle({
                let index = *index;
                move |_| Message::ToggleSelect(index)
            })]
            .spacing(6)
            .align_y(Alignment::Center);
            if show_icon {
                let icon = if entry.is_dir {
                    self.icons.oxizip()
                } else {
                    self.icons.for_path_name(&entry.name)
                };
                line = line.push(image(icon).width(icon_px).height(icon_px));
            }
            line = line.push(text(entry.name.clone()).size(12));
            col = col.push(
                container(line)
                    .padding([2, 4])
                    .width(Length::Fill)
                    .style(move |_theme| row_style(selected)),
            );
        }
        col.into()
    }

    fn view_icon_grid(
        &self,
        busy: bool,
        indices: &[usize],
        icon_px: f32,
        per_row: usize,
    ) -> Element<'_, Message> {
        let _ = busy;
        let mut col = column![].spacing(8);
        let mut line = row![].spacing(8);
        let mut count = 0usize;
        for index in indices {
            let Some(entry) = self.entries.get(*index) else {
                continue;
            };
            let selected = self.selected.contains(index);
            let icon = if entry.is_dir {
                self.icons.oxizip()
            } else {
                self.icons.for_path_name(&entry.name)
            };
            let cell = column![
                image(icon).width(icon_px).height(icon_px),
                text(entry.name.clone()).size(11),
            ]
            .spacing(4)
            .align_x(Alignment::Center)
            .width(Length::Fixed(icon_px + 36.0));
            let index = *index;
            line = line.push(
                button(cell)
                    .style(rounded_text_btn)
                    .on_press(Message::ToggleSelect(index))
                    .style(move |theme, status| {
                        let mut style = rounded_text_btn(theme, status);
                        if selected {
                            style.background =
                                Some(Background::Color(Color::from_rgba(0.37, 0.49, 0.89, 0.18)));
                        }
                        style
                    }),
            );
            count += 1;
            if count >= per_row {
                col = col.push(line);
                line = row![].spacing(8);
                count = 0;
            }
        }
        if count > 0 {
            col = col.push(line);
        }
        col.into()
    }

    fn view_settings(&self) -> Element<'_, Message> {
        let formats: Vec<FormatLabel> = FormatId::create_choices()
            .iter()
            .copied()
            .map(FormatLabel)
            .collect();
        let methods: Vec<MethodLabel> = MethodLabel::all().to_vec();
        let langs = vec![LangLabel::Zh, LangLabel::En];
        let current_lang = match self.lang {
            Lang::ZhCn => LangLabel::Zh,
            Lang::En => LangLabel::En,
        };
        let body = column![
            text(t(self.lang, "gui.settings.title")).size(16),
            row![
                text(t(self.lang, "gui.lang"))
                    .size(12)
                    .width(Length::Fixed(140.0)),
                pick_list(langs, Some(current_lang), Message::LangSelected).width(160),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text(t(self.lang, "gui.settings.default_format"))
                    .size(12)
                    .width(Length::Fixed(140.0)),
                pick_list(
                    formats,
                    Some(FormatLabel(self.create_format)),
                    Message::SettingsFormat,
                )
                .width(160),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text(t(self.lang, "gui.settings.default_method"))
                    .size(12)
                    .width(Length::Fixed(140.0)),
                pick_list(methods, Some(self.create_method), Message::SettingsMethod).width(160),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            checkbox(
                t(self.lang, "gui.settings.open_dest"),
                self.prefs.open_dest_after_extract,
            )
            .on_toggle(Message::SettingsOpenDest),
            checkbox(
                t(self.lang, "gui.menu.view.status"),
                self.prefs.show_status_bar
            )
            .on_toggle(|_| Message::MenuAction(MenuAction::ToggleStatusBar),),
            checkbox(t(self.lang, "gui.menu.view.top"), self.prefs.always_on_top)
                .on_toggle(|_| Message::MenuAction(MenuAction::ToggleAlwaysOnTop)),
            row![
                button(text(t(self.lang, "gui.settings.assoc")).size(12))
                    .style(rounded_secondary)
                    .on_press(Message::MenuAction(MenuAction::Associations)),
                button(text(t(self.lang, "gui.menu.settings.passman")).size(12))
                    .style(rounded_secondary)
                    .on_press(Message::MenuAction(MenuAction::PasswordManager)),
            ]
            .spacing(8),
            Space::with_height(Length::Fill),
            button(text(t(self.lang, "gui.settings.back")).size(13))
                .style(rounded_primary)
                .on_press(Message::SettingsBack),
        ]
        .spacing(10)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill);
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_overlay(&self) -> Element<'_, Message> {
        let card = match self.overlay {
            Overlay::None => return Space::with_height(0).into(),
            Overlay::About => self.overlay_card(
                t(self.lang, "gui.about.title"),
                column![
                    text(format!("Oxizip {}", env!("CARGO_PKG_VERSION"))).size(14),
                    text(t(self.lang, "gui.about.body")).size(12),
                    button(text(t(self.lang, "gui.dialog.close")).size(12))
                        .style(rounded_primary)
                        .on_press(Message::DismissOverlay),
                ]
                .spacing(8),
            ),
            Overlay::Comment => self.overlay_card(
                t(self.lang, "gui.comment.title"),
                column![
                    text_input("", &self.dialog_text)
                        .on_input(Message::DialogTextChanged)
                        .width(Length::Fill),
                    row![
                        button(text(t(self.lang, "gui.comment.save")).size(12))
                            .style(rounded_primary)
                            .on_press(Message::CommentSave),
                        button(text(t(self.lang, "gui.dialog.cancel")).size(12))
                            .style(rounded_secondary)
                            .on_press(Message::DismissOverlay),
                    ]
                    .spacing(8),
                ]
                .spacing(8),
            ),
            Overlay::Rename => self.overlay_card(
                t(self.lang, "gui.rename.title"),
                column![
                    text(t(self.lang, "gui.rename.name")).size(12),
                    text_input("", &self.dialog_text)
                        .on_input(Message::DialogTextChanged)
                        .on_submit(Message::RenameConfirm)
                        .width(Length::Fill),
                    row![
                        button(text(t(self.lang, "gui.dialog.ok")).size(12))
                            .style(rounded_primary)
                            .on_press(Message::RenameConfirm),
                        button(text(t(self.lang, "gui.dialog.cancel")).size(12))
                            .style(rounded_secondary)
                            .on_press(Message::DismissOverlay),
                    ]
                    .spacing(8),
                ]
                .spacing(8),
            ),
            Overlay::PasswordManager => self.overlay_card(
                t(self.lang, "gui.passman.title"),
                column![
                    text(t(self.lang, "gui.passman.hint")).size(11),
                    row![
                        text_input("", &self.dialog_text)
                            .on_input(Message::DialogTextChanged)
                            .width(Length::Fill),
                        button(text(t(self.lang, "gui.passman.add")).size(12))
                            .style(rounded_secondary)
                            .on_press(Message::PassmanAdd),
                    ]
                    .spacing(6),
                    self.view_password_list(),
                    button(text(t(self.lang, "gui.dialog.close")).size(12))
                        .style(rounded_primary)
                        .on_press(Message::DismissOverlay),
                ]
                .spacing(8),
            ),
            Overlay::Recover => self.overlay_card(
                t(self.lang, "gui.recover.title"),
                column![
                    text(t(self.lang, "gui.recover.hint")).size(11),
                    text(t(self.lang, "gui.recover.wordlist")).size(12),
                    row![
                        text(if self.recover_wordlist.is_empty() {
                            "—".to_string()
                        } else {
                            self.recover_wordlist.clone()
                        })
                        .size(11)
                        .width(Length::Fill),
                        button(text(t(self.lang, "gui.recover.browse")).size(12))
                            .style(rounded_secondary)
                            .on_press(Message::PickWordlist),
                    ]
                    .spacing(6),
                    row![
                        text(t(self.lang, "gui.recover.maxlen")).size(12),
                        text_input("", &self.recover_max_len)
                            .on_input(Message::RecoverMaxChanged)
                            .width(Length::Fixed(48.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(self.dialog_result.as_str()).size(12),
                    row![
                        button(text(t(self.lang, "gui.recover.start")).size(12))
                            .style(rounded_primary)
                            .on_press(Message::RecoverStart),
                        button(text(t(self.lang, "gui.dialog.close")).size(12))
                            .style(rounded_secondary)
                            .on_press(Message::DismissOverlay),
                    ]
                    .spacing(8),
                ]
                .spacing(8),
            ),
            Overlay::Checksum => self.overlay_card(
                t(self.lang, "gui.checksum.title"),
                column![
                    container(
                        scrollable(text(self.dialog_result.as_str()).size(11))
                            .height(Length::Fixed(180.0))
                    )
                    .style(list_box_style)
                    .padding(6)
                    .width(Length::Fill),
                    button(text(t(self.lang, "gui.dialog.close")).size(12))
                        .style(rounded_primary)
                        .on_press(Message::DismissOverlay),
                ]
                .spacing(8),
            ),
            Overlay::Toolbar => self.overlay_card(
                t(self.lang, "gui.toolbar.title"),
                column![
                    self.toolbar_flag_box(
                        ToolbarFlag::Close,
                        "gui.close_archive",
                        self.prefs.toolbar_buttons.close
                    ),
                    self.toolbar_flag_box(
                        ToolbarFlag::Open,
                        "gui.open_archive",
                        self.prefs.toolbar_buttons.open
                    ),
                    self.toolbar_flag_box(
                        ToolbarFlag::Extract,
                        "gui.extract",
                        self.prefs.toolbar_buttons.extract
                    ),
                    self.toolbar_flag_box(
                        ToolbarFlag::ExtractHere,
                        "gui.extract_here",
                        self.prefs.toolbar_buttons.extract_here,
                    ),
                    self.toolbar_flag_box(
                        ToolbarFlag::ExtractSmart,
                        "gui.extract_smart",
                        self.prefs.toolbar_buttons.extract_smart,
                    ),
                    self.toolbar_flag_box(
                        ToolbarFlag::Test,
                        "gui.test",
                        self.prefs.toolbar_buttons.test
                    ),
                    self.toolbar_flag_box(
                        ToolbarFlag::Add,
                        "gui.add",
                        self.prefs.toolbar_buttons.add
                    ),
                    self.toolbar_flag_box(
                        ToolbarFlag::Create,
                        "gui.create",
                        self.prefs.toolbar_buttons.create
                    ),
                    button(text(t(self.lang, "gui.dialog.close")).size(12))
                        .style(rounded_primary)
                        .on_press(Message::DismissOverlay),
                ]
                .spacing(4),
            ),
            Overlay::Color => self.overlay_card(
                t(self.lang, "gui.color.title"),
                column![
                    button(text(t(self.lang, "gui.color.brand")).size(12))
                        .style(rounded_secondary)
                        .on_press(Message::ColorPicked(ColorAccent::BrandBlue)),
                    button(text(t(self.lang, "gui.color.navy")).size(12))
                        .style(rounded_secondary)
                        .on_press(Message::ColorPicked(ColorAccent::DeepNavy)),
                    button(text(t(self.lang, "gui.color.teal")).size(12))
                        .style(rounded_secondary)
                        .on_press(Message::ColorPicked(ColorAccent::Teal)),
                    button(text(t(self.lang, "gui.dialog.cancel")).size(12))
                        .style(rounded_text_btn)
                        .on_press(Message::DismissOverlay),
                ]
                .spacing(6),
            ),
            Overlay::Find => self.overlay_card(
                t(self.lang, "gui.find.title"),
                column![
                    text(t(self.lang, "gui.find.hint")).size(12),
                    text_input("", &self.dialog_text)
                        .on_input(Message::DialogTextChanged)
                        .on_submit(Message::FindApply)
                        .width(Length::Fill),
                    row![
                        button(text(t(self.lang, "gui.find.apply")).size(12))
                            .style(rounded_primary)
                            .on_press(Message::FindApply),
                        button(text(t(self.lang, "gui.find.clear")).size(12))
                            .style(rounded_secondary)
                            .on_press(Message::FindClear),
                    ]
                    .spacing(8),
                ]
                .spacing(8),
            ),
            Overlay::ConfirmDelete(permanent) => {
                let hint = if permanent {
                    t(self.lang, "gui.delete.confirm_perm")
                } else {
                    t(self.lang, "gui.delete.confirm")
                };
                self.overlay_card(
                    t(self.lang, "gui.menu.file.delete"),
                    column![
                        text(hint).size(12),
                        row![
                            button(text(t(self.lang, "gui.dialog.ok")).size(12))
                                .style(button::danger)
                                .on_press(Message::ConfirmDeleteArchive),
                            button(text(t(self.lang, "gui.dialog.cancel")).size(12))
                                .style(rounded_secondary)
                                .on_press(Message::DismissOverlay),
                        ]
                        .spacing(8),
                    ]
                    .spacing(8),
                )
            }
        };
        stack![
            button(Space::new(Length::Fill, Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(dim_style)
                .on_press(Message::DismissOverlay),
            column![
                Space::with_height(Length::Fill),
                row![
                    Space::with_width(Length::Fill),
                    card,
                    Space::with_width(Length::Fill),
                ],
                Space::with_height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn overlay_card<'a>(
        &self,
        title: &'a str,
        body: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        container(
            column![text(title).size(16), body.into()]
                .spacing(8)
                .padding(12)
                .width(Length::Fixed(360.0)),
        )
        .style(dropdown_panel_style)
        .into()
    }

    fn view_password_list(&self) -> Element<'_, Message> {
        if self.prefs.passwords.is_empty() {
            return text(t(self.lang, "gui.passman.empty")).size(12).into();
        }
        let mut col = column![].spacing(4);
        for (index, password) in self.prefs.passwords.iter().enumerate() {
            col = col.push(
                row![
                    text(password.clone()).size(12).width(Length::Fill),
                    button(text(t(self.lang, "gui.passman.use")).size(11))
                        .style(rounded_text_btn)
                        .on_press(Message::PassmanUse(index)),
                    button(text(t(self.lang, "gui.passman.remove")).size(11))
                        .style(rounded_text_btn)
                        .on_press(Message::PassmanRemove(index)),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        }
        container(scrollable(col).height(Length::Fixed(120.0)))
            .style(list_box_style)
            .padding(6)
            .width(Length::Fill)
            .into()
    }

    fn toolbar_flag_box(
        &self,
        flag: ToolbarFlag,
        key: &'static str,
        on: bool,
    ) -> Element<'_, Message> {
        checkbox(t(self.lang, key), on)
            .on_toggle(move |value| Message::ToolbarFlagToggled(flag, value))
            .into()
    }
}

fn rounded_primary(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::primary(theme, status);
    style.border.radius = CONTROL_RADIUS.into();
    style
}

fn rounded_secondary(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::secondary(theme, status);
    style.border.radius = CONTROL_RADIUS.into();
    style
}

fn rounded_text_btn(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::text(theme, status);
    style.border.radius = 6.0.into();
    style
}

fn list_header_style(theme: &Theme) -> iced::widget::container::Style {
    let palette = theme.extended_palette();
    iced::widget::container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        ..Default::default()
    }
}

fn list_box_style(theme: &Theme) -> iced::widget::container::Style {
    let palette = theme.extended_palette();
    iced::widget::container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        ..Default::default()
    }
}

pub(crate) fn home_menu_bar_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(Color::WHITE)),
        border: Border {
            color: Color::from_rgb8(0xE0, 0xE0, 0xE0),
            width: 0.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn home_stage_style(accent: ColorAccent) -> impl Fn(&Theme) -> iced::widget::container::Style {
    let (c0, c1, c2) = match accent {
        ColorAccent::BrandBlue => (
            Color::from_rgb8(0x3D, 0x91, 0xF8),
            Color::from_rgb8(0x2C, 0x76, 0xF5),
            Color::from_rgb8(0x21, 0x56, 0xF2),
        ),
        ColorAccent::DeepNavy => (
            Color::from_rgb8(0x10, 0x30, 0x58),
            Color::from_rgb8(0x00, 0x20, 0x40),
            Color::from_rgb8(0x00, 0x10, 0x30),
        ),
        ColorAccent::Teal => (
            Color::from_rgb8(0x40, 0xC0, 0xD0),
            Color::from_rgb8(0x20, 0xA0, 0xC0),
            Color::from_rgb8(0x10, 0x70, 0x90),
        ),
    };
    move |_theme: &Theme| {
        let gradient = Linear::new(Radians(std::f32::consts::FRAC_PI_2))
            .add_stop(0.0, c0)
            .add_stop(0.45, c1)
            .add_stop(1.0, c2);
        iced::widget::container::Style {
            background: Some(Background::Gradient(Gradient::Linear(gradient))),
            ..Default::default()
        }
    }
}

pub(crate) fn home_menu_btn_style(_theme: &Theme, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Hovered | button::Status::Pressed => (
            Some(Background::Color(Color::from_rgb8(0xE8, 0xE8, 0xE8))),
            Color::BLACK,
        ),
        button::Status::Disabled => (None, Color::from_rgb8(0x99, 0x99, 0x99)),
        button::Status::Active => (None, Color::BLACK),
    };
    button::Style {
        background: bg,
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

pub(crate) fn dropdown_panel_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(Color::from_rgb8(0xFF, 0xFF, 0xFF))),
        border: Border {
            color: Color::from_rgb8(0xCC, 0xCC, 0xCC),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.18),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..Default::default()
    }
}

pub(crate) fn dropdown_item_style(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::text(theme, status);
    style.border.radius = 4.0.into();
    if matches!(status, button::Status::Hovered | button::Status::Pressed) {
        style.background = Some(Background::Color(Color::from_rgb8(0xE8, 0xF0, 0xFF)));
    }
    style
}

fn home_glass_style(_theme: &Theme, status: button::Status) -> button::Style {
    let (bg_a, border_a) = match status {
        button::Status::Hovered => (0.30, 0.45),
        button::Status::Pressed => (0.34, 0.50),
        button::Status::Disabled => (0.12, 0.18),
        button::Status::Active => (0.22, 0.32),
    };
    button::Style {
        background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, bg_a))),
        text_color: Color::WHITE,
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, border_a),
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: Default::default(),
    }
}

fn home_glass_tile<'a>(
    icon: iced::widget::image::Handle,
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let body = column![
        image(icon).width(52).height(52),
        Space::with_height(16),
        text(label).size(14).style(|_| iced::widget::text::Style {
            color: Some(Color::WHITE),
        }),
    ]
    .spacing(0)
    .align_x(Alignment::Center);

    let mut btn = button(
        container(body)
            .width(Length::Fill)
            .padding([34, 18])
            .center_x(Length::Fill),
    )
    .width(210)
    .style(home_glass_style);

    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
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

async fn pick_save_as(name: String) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_file_name(&name)
        .save_file()
        .await
        .map(|f| f.path().to_path_buf())
}

async fn pick_repair_output(source: PathBuf) -> Option<PathBuf> {
    let name = source
        .file_name()
        .map(|s| format!("repaired-{}", s.to_string_lossy()))
        .unwrap_or_else(|| "repaired.zip".into());
    rfd::AsyncFileDialog::new()
        .set_file_name(&name)
        .save_file()
        .await
        .map(|f| f.path().to_path_buf())
}

async fn pick_wordlist() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .add_filter("Text", &["txt", "lst", "dic"])
        .pick_file()
        .await
        .map(|f| f.path().to_path_buf())
}

async fn copy_file(from: PathBuf, to: PathBuf) -> Result<PathBuf, String> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let result = std::fs::copy(&from, &to)
            .map(|_| to)
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    match rx.await {
        Ok(result) => result,
        Err(_) => Err("copy canceled".into()),
    }
}

fn method_label_from_prefs(prefs: &Prefs) -> MethodLabel {
    match prefs.default_method_id() {
        Some(method) => MethodLabel::Method(method),
        None => MethodLabel::Auto,
    }
}

fn apply_window_level(on_top: bool) -> Task<Message> {
    let level = if on_top {
        window::Level::AlwaysOnTop
    } else {
        window::Level::Normal
    };
    window::get_latest().then(move |id| match id {
        Some(id) => window::change_level(id, level),
        None => Task::none(),
    })
}

fn entry_visible(entry: &EntryMeta, prefix: Option<&str>, filter: &str) -> bool {
    if let Some(prefix) = prefix {
        let name = entry.name.trim_start_matches('/');
        let folder = prefix.trim_end_matches('/');
        if name != folder && !name.starts_with(&format!("{folder}/")) {
            return false;
        }
    }
    filter.is_empty() || entry.name.to_lowercase().contains(filter)
}

fn entry_chosen(entry: &EntryMeta, picked: &EntryMeta) -> bool {
    if picked.is_dir {
        let prefix = picked.name.trim_end_matches('/');
        entry.name == picked.name || entry.name.starts_with(&format!("{prefix}/"))
    } else {
        entry.name == picked.name
    }
}

fn top_folder(name: &str, is_dir: bool) -> Option<String> {
    let trimmed = name.trim_matches('/');
    let mut parts = trimmed.split('/');
    let first = parts.next().filter(|part| !part.is_empty())?;
    if parts.next().is_some() || is_dir {
        Some(first.to_string())
    } else {
        None
    }
}

fn fmt_modified(when: Option<SystemTime>) -> String {
    let Some(when) = when else {
        return "-".into();
    };
    let Ok(dur) = when.duration_since(UNIX_EPOCH) else {
        return "-".into();
    };
    let secs = dur.as_secs();
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let tod = secs % 86_400;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn accent_color(accent: ColorAccent) -> Color {
    match accent {
        ColorAccent::BrandBlue => Color::from_rgb8(0x5E, 0x7C, 0xE2),
        ColorAccent::DeepNavy => Color::from_rgb8(0x00, 0x20, 0x40),
        ColorAccent::Teal => Color::from_rgb8(0x20, 0xA0, 0xC0),
    }
}

fn row_style(selected: bool) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: if selected {
            Some(Background::Color(Color::from_rgba(0.37, 0.49, 0.89, 0.16)))
        } else {
            None
        },
        ..Default::default()
    }
}

fn dim_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.28))),
        text_color: Color::WHITE,
        border: Border::default(),
        shadow: iced::Shadow::default(),
    }
}
