//! Oxizip iced GUI — list / extract / create; open via system default app.

// Release builds: no console window on Windows. Keep console in debug for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icons;
mod job;
mod licenses;
mod menu;
mod prefs;
mod settings;
mod setup;
mod shell_ops;
mod theme;
mod worker;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iced::widget::{
    button, checkbox, column, container, image, mouse_area, pick_list, progress_bar, row,
    scrollable, stack, text, text_input, Column, Space,
};
use iced::{
    window, Alignment, Background, Border, Color, Element, Length, Point, Size, Subscription, Task,
    Theme,
};
use icons::IconSet;
use licenses::{catalog, ComponentKind};
use menu::{MenuAction, MenuPopup};
use oxizip_engine::{
    ConflictPolicy, CreateOptions, EntryMeta, FormatId, MethodId, PasswordRecoverOptions,
};
use oxizip_i18n::{t, Lang};
use prefs::{ColorAccent, ListViewMode, Prefs, ToolbarSize};
use settings::{SettingsMsg, SettingsTab, SettingsView};
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
    /// Compact shell job progress (create/compress).
    Job,
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

/// Single size for main GUI screens (association, main, licenses, …).
const APP_WINDOW_SIZE: Size = Size::new(775.0, 480.0);
/// Dedicated settings dialog (separate process / window).
const SETTINGS_WINDOW_SIZE: Size = Size::new(700.0, 460.0);

fn center_on_monitor(size: Size, monitor: Size) -> Point {
    Point::new(
        ((monitor.width - size.width) / 2.0).max(0.0),
        ((monitor.height - size.height) / 2.0).max(0.0),
    )
}

fn main() -> iced::Result {
    oxizip_i18n::init();
    oxizip_i18n::ensure_user_locales_scaffold();
    let job_mode = job::parse_job_args().is_some();
    let settings_mode = setup::parse_settings_launch().is_some();
    let mut window = iced::window::Settings {
        size: if job_mode {
            job::JOB_WINDOW_SIZE
        } else if settings_mode {
            SETTINGS_WINDOW_SIZE
        } else {
            APP_WINDOW_SIZE
        },
        resizable: false,
        decorations: false,
        transparent: true,
        // `Centered` is unreliable on Windows; compute from monitor size.
        position: window::Position::SpecificWith(center_on_monitor),
        ..Default::default()
    };
    window.icon = icons::window_icon();

    iced::application(OxizipApp::title, OxizipApp::update, OxizipApp::view)
        .subscription(OxizipApp::subscription)
        .default_font(ui_font())
        .theme(OxizipApp::theme)
        .window(window)
        .run_with(OxizipApp::new)
}

#[cfg(windows)]
fn apply_native_window_rounding(raw_id: u64) {
    // Win11 DWM rounded corners for the HWND backing this iced window.
    // Prefer ROUND over ROUNDSMALL so 700×460 settings matches the 775×480 main look.
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut core::ffi::c_void,
            attr: u32,
            value: *const core::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: i32 = 2;
    // Avoid a system-drawn dark border fighting the iced 1px frame on transparent windows.
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
    unsafe {
        let hwnd = raw_id as *mut core::ffi::c_void;
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&pref as *const i32).cast(),
            std::mem::size_of_val(&pref) as u32,
        );
        let border = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&border as *const u32).cast(),
            std::mem::size_of_val(&border) as u32,
        );
    }
}

#[cfg(not(windows))]
fn apply_native_window_rounding(_raw_id: u64) {}

fn frame_border(theme: &Theme) -> iced::widget::container::Style {
    theme::frame(theme)
}

fn caption_bar<'a>(
    title: String,
    brand_icon: iced::widget::image::Handle,
    on_drag: Message,
    on_minimize: Message,
    on_close: Message,
) -> Element<'a, Message> {
    let drag_zone = mouse_area(
        row![
            image(brand_icon).width(16).height(16),
            text(title).size(12).style(theme::text_secondary),
            Space::with_width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left: 12.0,
        }),
    )
    .on_press(on_drag);

    let min_btn = button(
        container(icons::ui_svg(
            icons::UiIcon::Min,
            12.0,
            Color::from_rgb8(0x72, 0x78, 0x87),
        ))
        .center_x(Length::Fill)
        .center_y(Length::Fill),
    )
    .width(38)
    .height(28)
    .padding(0)
    .style(rounded_text_btn)
    .on_press(on_minimize);

    let close_btn = button(
        container(text("×").size(14))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(38)
    .height(28)
    .padding(0)
    .style(theme::btn_caption_close)
    .on_press(on_close);

    column![
        container(
            row![drag_zone, min_btn, close_btn]
                .spacing(2)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .height(32),
        )
        .width(Length::Fill)
        .style(theme::caption_bar),
        container(Space::with_height(1))
            .width(Length::Fill)
            .style(theme::divider),
    ]
    .into()
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
    progress_file_fraction: f32,
    progress_detail: String,
    /// Shell `--job` compact dialog state.
    job_verb: job::JobVerb,
    job_out: Option<PathBuf>,
    job_started: Option<std::time::Instant>,
    job_close_when_done: bool,
    job_open_folder: bool,
    job_finished: bool,
    /// Close after `WindowId` arrives when the job finished too quickly.
    job_pending_close: bool,
    /// Shell extract waiting for overwrite / auto-rename choice.
    pending_shell_extract: Option<job::ExtractJob>,
    /// Main-window extract (archive, dest) waiting for conflict choice.
    pending_main_extract: Option<(PathBuf, PathBuf)>,
    job_conflicts: Vec<String>,
    open_temp: Option<Arc<TempDir>>,
    icons: IconSet,
    setup_exts: BTreeMap<&'static str, bool>,
    pending_open: Option<PathBuf>,
    menu_popup: MenuPopup,
    prefs: Prefs,
    ui_theme: Theme,
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
    window_id: Option<window::Id>,
    settings_tab: SettingsTab,
    /// Short feedback line on the Settings screen (never the full CLI log).
    settings_status: String,
    /// Dedicated `--settings` process.
    settings_mode: bool,
    /// Draft prefs edited in the settings dialog; committed only on OK.
    settings_draft: Prefs,
    settings_draft_exts: BTreeMap<&'static str, bool>,
    /// Reload prefs.json while a spawned settings dialog may still be open.
    prefs_watch_until: Option<std::time::Instant>,
    prefs_mtime: Option<std::time::SystemTime>,
    /// Native HWND of the main iced window (for modal disable).
    main_hwnd: Option<u64>,
    /// Child `--settings` process; main stays visible but non-operable.
    settings_child: Option<std::process::Child>,
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
            progress_file_fraction: 0.0,
            progress_detail: String::new(),
            job_verb: job::JobVerb::Create,
            job_out: None,
            job_started: None,
            job_close_when_done: true,
            job_open_folder: false,
            job_finished: false,
            job_pending_close: false,
            pending_shell_extract: None,
            pending_main_extract: None,
            job_conflicts: Vec::new(),
            open_temp: None,
            icons: IconSet::load(),
            setup_exts: setup::default_assoc_selection(),
            pending_open: None,
            menu_popup: MenuPopup::None,
            prefs: Prefs::default(),
            ui_theme: theme::build(prefs::ThemeMode::Light, ColorAccent::default()),
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
            window_id: None,
            settings_tab: SettingsTab::General,
            settings_status: String::new(),
            settings_mode: false,
            settings_draft: Prefs::default(),
            settings_draft_exts: setup::default_assoc_selection(),
            prefs_watch_until: None,
            prefs_mtime: None,
            main_hwnd: None,
            settings_child: None,
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
    Settings(SettingsMsg),
    SettingsBrowseTemp(Option<PathBuf>),
    SettingsBrowseNewArchiveFolder(Option<PathBuf>),
    SettingsExportPath(Option<PathBuf>),
    SettingsImportPath(Option<PathBuf>),
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
    PasswordChanged(String),
    OpenEntry(usize),
    ArchiveChosen(Option<PathBuf>),
    ExtractDirChosen(Option<PathBuf>),
    ShellExtractDialogChosen(Option<PathBuf>),
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
    JobCloseWhenDone(bool),
    JobOpenFolder(bool),
    JobConflictOverwrite,
    JobConflictRename,
    JobConflictCancel,
    WindowId(Option<window::Id>),
    RawWindowId(u64),
    /// Click on the modal blocker while settings child is open (ignored).
    SettingsModalNop,
    TitleDrag,
    Minimize,
    Close,
}

impl OxizipApp {
    fn new() -> (Self, Task<Message>) {
        let pending_job = job::parse_job_args();
        let settings_tab_arg = setup::parse_settings_launch();
        let settings_mode = settings_tab_arg.is_some();
        let (force_setup, pending_open) = setup::parse_launch_args();
        let screen = if pending_job.is_some() {
            Screen::Job
        } else if settings_mode {
            Screen::Settings
        } else if setup::needs_first_run_setup(force_setup) {
            Screen::Setup
        } else {
            Screen::Main
        };
        let prefs = Prefs::load();
        let lang = prefs.lang_enum();
        let create_format = prefs.default_format_id();
        let create_method = method_label_from_prefs(&prefs);
        let always_on_top = prefs.always_on_top && !settings_mode;
        let settings_tab = settings_tab_arg
            .as_deref()
            .and_then(SettingsTab::parse_cli)
            .unwrap_or(SettingsTab::General);
        let settings_draft = prefs.clone();
        let settings_draft_exts = setup::default_assoc_selection();
        let prefs_mtime = std::fs::metadata(prefs::prefs_path())
            .and_then(|m| m.modified())
            .ok();
        let mut app = Self {
            lang,
            ui_theme: theme::build(prefs.theme_mode, prefs.color_accent),
            prefs,
            status: t(lang, "gui.placeholder").to_string(),
            create_format,
            create_method,
            screen,
            pending_open,
            settings_mode,
            settings_tab,
            settings_draft,
            settings_draft_exts,
            prefs_mtime,
            ..Self::default()
        };
        let job_task = match pending_job {
            Some(job::PendingJob::Create(spec)) => app.start_shell_create(spec),
            Some(job::PendingJob::Extract(spec)) => app.start_shell_extract(spec),
            None => Task::none(),
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
        let boot = window::get_oldest().map(Message::WindowId);
        (app, Task::batch([boot, job_task, open_task, top_task]))
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
            let lang_arg = self.lang.cli_code();
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

    fn lang_arg(&self) -> &'static str {
        self.lang.cli_code()
    }

    fn save_prefs(&mut self) {
        self.prefs.save();
        self.prefs.write_shell_menu_sidecar();
    }

    /// Settings-page feedback only — keep it short so the layout is not crushed.
    fn set_settings_status(&mut self, msg: impl Into<String>) {
        let mut s = msg.into();
        // Drop verbose CLI tails after the first sentence / em dash detail.
        if let Some((head, _)) = s.split_once(" (") {
            s = head.trim_end_matches('—').trim().to_string();
        }
        if s.chars().count() > 80 {
            s = s.chars().take(77).collect::<String>() + "…";
        }
        self.settings_status = s;
    }

    fn commit_settings_ok(&mut self) -> Task<Message> {
        self.prefs = self.settings_draft.clone();
        self.ui_theme = theme::build(self.prefs.theme_mode, self.prefs.color_accent);
        self.setup_exts = self.settings_draft_exts.clone();
        self.lang = self.prefs.lang_enum();
        self.create_format = self.prefs.default_format_id();
        self.create_method = method_label_from_prefs(&self.prefs);
        self.save_prefs();
        self.prefs_mtime = std::fs::metadata(prefs::prefs_path())
            .and_then(|m| m.modified())
            .ok();

        let lang_arg = self.lang_arg();
        let selected = setup::selected_exts(&self.setup_exts);
        if !selected.is_empty() {
            match setup::install_associations(lang_arg, &selected) {
                Ok(_) => {
                    setup::mark_setup_done();
                    self.last_error = None;
                }
                Err(e) => {
                    self.last_error = Some(format!("{}: {e}", t(self.lang, "gui.setup.failed")));
                    self.set_settings_status(t(self.lang, "gui.setup.failed"));
                    return Task::none();
                }
            }
        }
        if let Err(e) = setup::install_menus_only(lang_arg) {
            self.last_error = Some(format!("{}: {e}", t(self.lang, "gui.setup.failed")));
            self.set_settings_status(t(self.lang, "gui.setup.failed"));
            return Task::none();
        }
        self.set_settings_status(t(self.lang, "gui.settings.status.assoc_ok"));
        if self.settings_mode {
            // Standalone `--settings` process: close after apply.
            return self.window_id.map(window::close).unwrap_or_else(Task::none);
        }
        self.leave_settings()
    }

    fn open_settings_modal(&mut self, tab: Option<&str>) -> Task<Message> {
        // Already running as the settings process — stay in-process.
        if self.settings_mode {
            self.settings_tab = tab
                .and_then(SettingsTab::parse_cli)
                .unwrap_or(SettingsTab::General);
            self.settings_draft = self.prefs.clone();
            self.settings_draft_exts = self.setup_exts.clone();
            if self.settings_draft_exts.is_empty() {
                self.settings_draft_exts = setup::default_assoc_selection();
            }
            self.settings_status.clear();
            self.last_error = None;
            self.menu_popup = MenuPopup::None;
            self.screen = Screen::Settings;
            return Task::none();
        }
        // Main window: spawn a sibling `--settings` process and disable this HWND.
        if self.settings_child.is_some() {
            return Task::none();
        }
        self.menu_popup = MenuPopup::None;
        let tab_name = tab
            .and_then(SettingsTab::parse_cli)
            .map(SettingsTab::cli_name);
        match shell_ops::spawn_settings_window(tab_name) {
            Ok(child) => {
                self.settings_child = Some(child);
                if let Some(hwnd) = self.main_hwnd {
                    shell_ops::set_native_window_enabled(hwnd, false);
                }
                self.prefs_watch_until =
                    Some(std::time::Instant::now() + Duration::from_secs(3600));
            }
            Err(e) => {
                self.last_error = Some(format!("{}: {e}", t(self.lang, "gui.setup.failed")));
            }
        }
        Task::none()
    }

    fn leave_settings(&mut self) -> Task<Message> {
        if self.settings_mode {
            return self.window_id.map(window::close).unwrap_or_else(Task::none);
        }
        // In-process Settings screen is no longer used from main; keep as safety.
        self.screen = Screen::Main;
        self.settings_status.clear();
        Task::none()
    }

    fn finish_settings_child(&mut self) -> Task<Message> {
        if let Some(mut child) = self.settings_child.take() {
            let _ = child.try_wait();
        }
        if let Some(hwnd) = self.main_hwnd {
            shell_ops::set_native_window_enabled(hwnd, true);
        }
        self.prefs_watch_until = None;
        self.maybe_reload_prefs_from_disk()
    }

    fn poll_settings_child(&mut self) -> Task<Message> {
        let Some(child) = self.settings_child.as_mut() else {
            return Task::none();
        };
        match child.try_wait() {
            Ok(Some(_)) => self.finish_settings_child(),
            Ok(None) => Task::none(),
            Err(_) => self.finish_settings_child(),
        }
    }

    fn maybe_reload_prefs_from_disk(&mut self) -> Task<Message> {
        if self.settings_mode || self.settings_child.is_some() {
            return Task::none();
        }
        let path = prefs::prefs_path();
        let Ok(meta) = std::fs::metadata(&path) else {
            return Task::none();
        };
        let Ok(modified) = meta.modified() else {
            return Task::none();
        };
        if self.prefs_mtime.is_some_and(|t| t >= modified) {
            return Task::none();
        }
        let loaded = Prefs::load();
        let on_top = loaded.always_on_top;
        self.lang = loaded.lang_enum();
        self.create_format = loaded.default_format_id();
        self.create_method = method_label_from_prefs(&loaded);
        self.ui_theme = theme::build(loaded.theme_mode, loaded.color_accent);
        self.prefs = loaded;
        self.prefs_mtime = Some(modified);
        apply_window_level(on_top)
    }

    fn handle_settings(&mut self, msg: SettingsMsg) -> Task<Message> {
        let draft = &mut self.settings_draft;
        match msg {
            SettingsMsg::Tab(tab) => {
                self.settings_tab = tab;
                Task::none()
            }
            SettingsMsg::Ok => self.commit_settings_ok(),
            SettingsMsg::Reset => {
                let passwords = draft.passwords.clone();
                let recent = draft.recent.clone();
                let lang = draft.lang_enum();
                *draft = Prefs::default();
                draft.passwords = passwords;
                draft.recent = recent;
                draft.set_lang(lang);
                self.settings_draft_exts = setup::default_assoc_selection();
                self.set_settings_status(t(self.lang, "gui.settings.status.reset"));
                Task::none()
            }
            SettingsMsg::ExtToggled(ext, on) => {
                self.settings_draft_exts.insert(ext, on);
                Task::none()
            }
            SettingsMsg::ToggleSelectAllAssoc => {
                let all_on = !self.settings_draft_exts.is_empty()
                    && self.settings_draft_exts.values().all(|v| *v);
                for v in self.settings_draft_exts.values_mut() {
                    *v = !all_on;
                }
                Task::none()
            }
            SettingsMsg::SelectAllAssocDefaults => {
                self.settings_draft_exts = setup::default_assoc_selection();
                for v in self.settings_draft_exts.values_mut() {
                    *v = true;
                }
                Task::none()
            }
            SettingsMsg::DoubleClick(action) => {
                draft.shell.double_click = action;
                Task::none()
            }
            SettingsMsg::DeleteAfterDblExtract(v) => {
                draft.shell.delete_after_dbl_extract = v;
                Task::none()
            }
            SettingsMsg::ShellExtractHere(v) => {
                draft.shell.extract_here = v;
                Task::none()
            }
            SettingsMsg::ShellExtractSmart(v) => {
                draft.shell.extract_smart = v;
                Task::none()
            }
            SettingsMsg::ShellExtractToFolder(v) => {
                draft.shell.extract_to_folder = v;
                Task::none()
            }
            SettingsMsg::ShellExtractHereDel(v) => {
                draft.shell.extract_here_del = v;
                Task::none()
            }
            SettingsMsg::ShellExtractSmartDel(v) => {
                draft.shell.extract_smart_del = v;
                Task::none()
            }
            SettingsMsg::ShellExtractToFolderDel(v) => {
                draft.shell.extract_to_folder_del = v;
                Task::none()
            }
            SettingsMsg::ShellExtractDialog(v) => {
                draft.shell.extract_dialog = v;
                Task::none()
            }
            SettingsMsg::ShellOpenWith(v) => {
                draft.shell.open_with = v;
                Task::none()
            }
            SettingsMsg::ShellTest(v) => {
                draft.shell.test = v;
                Task::none()
            }
            SettingsMsg::ShellChecksum(v) => {
                draft.shell.checksum = v;
                Task::none()
            }
            SettingsMsg::ShellCompressZip(v) => {
                draft.shell.compress_zip = v;
                Task::none()
            }
            SettingsMsg::ShellCompress7z(v) => {
                draft.shell.compress_7z = v;
                Task::none()
            }
            SettingsMsg::ShellCompressZipx(v) => {
                draft.shell.compress_zipx = v;
                Task::none()
            }
            SettingsMsg::ShellCompressTar(v) => {
                draft.shell.compress_tar = v;
                Task::none()
            }
            SettingsMsg::ShellCompressTgz(v) => {
                draft.shell.compress_tgz = v;
                Task::none()
            }
            SettingsMsg::ShellCompressGz(v) => {
                draft.shell.compress_gz = v;
                Task::none()
            }
            SettingsMsg::ShellCompressXz(v) => {
                draft.shell.compress_xz = v;
                Task::none()
            }
            SettingsMsg::ShellCompressZpaq(v) => {
                draft.shell.compress_zpaq = v;
                Task::none()
            }
            SettingsMsg::ShellCompressEachZip(v) => {
                draft.shell.compress_each_zip = v;
                Task::none()
            }
            SettingsMsg::ShellCompressEach7z(v) => {
                draft.shell.compress_each_7z = v;
                Task::none()
            }
            SettingsMsg::ShellCascaded(v) => {
                draft.shell.cascaded = v;
                Task::none()
            }
            SettingsMsg::ShellDisabled(v) => {
                draft.shell.disabled = v;
                Task::none()
            }
            SettingsMsg::ShellHideIcon(v) => {
                draft.shell.hide_icon = v;
                Task::none()
            }
            SettingsMsg::EscToExit(v) => {
                draft.esc_to_exit = v;
                Task::none()
            }
            SettingsMsg::ShowDotDot(v) => {
                draft.show_dotdot = v;
                Task::none()
            }
            SettingsMsg::ShowGrid(v) => {
                draft.show_gridlines = v;
                Task::none()
            }
            SettingsMsg::FullRowSelect(v) => {
                draft.full_row_select = v;
                Task::none()
            }
            SettingsMsg::SingleClickOpen(v) => {
                draft.single_click_open = v;
                Task::none()
            }
            SettingsMsg::AlwaysOnTop(v) => {
                draft.always_on_top = v;
                Task::none()
            }
            SettingsMsg::ShowStatusBar(v) => {
                draft.show_status_bar = v;
                Task::none()
            }
            SettingsMsg::TestAfterModify(v) => {
                draft.test_after_modify = v;
                Task::none()
            }
            SettingsMsg::ShowTree(v) => {
                draft.show_tree = v;
                Task::none()
            }
            SettingsMsg::ShowComment(v) => {
                draft.show_comment = v;
                Task::none()
            }
            SettingsMsg::OpenDest(v) => {
                draft.open_dest_after_extract = v;
                Task::none()
            }
            SettingsMsg::JobKeepOpenExtract(v) => {
                draft.job_keep_open_after_extract = v;
                Task::none()
            }
            SettingsMsg::JobKeepOpenCreate(v) => {
                draft.job_keep_open_after_create = v;
                Task::none()
            }
            SettingsMsg::EnterSingleRoot(v) => {
                draft.enter_single_root_folder = v;
                Task::none()
            }
            SettingsMsg::SmartSingleFileFolder(v) => {
                draft.smart_single_file_folder = v;
                Task::none()
            }
            SettingsMsg::DefaultFormat(id) => {
                draft.set_default_format(id);
                Task::none()
            }
            SettingsMsg::DefaultMethodAuto => {
                draft.set_default_method_auto();
                Task::none()
            }
            SettingsMsg::DefaultMethod(id) => {
                draft.set_default_method(id);
                Task::none()
            }
            SettingsMsg::CompressionLevel(level) => {
                draft.compression_level = level;
                Task::none()
            }
            SettingsMsg::Utf8Zip(v) => {
                draft.utf8_zip_names = v;
                Task::none()
            }
            SettingsMsg::ExcludeThumbsDb(v) => {
                draft.exclude_thumbs_db = v;
                Task::none()
            }
            SettingsMsg::TestAfterCompress(v) => {
                draft.test_after_compress = v;
                Task::none()
            }
            SettingsMsg::NoRootSingleFolder(v) => {
                draft.no_root_for_single_folder = v;
                Task::none()
            }
            SettingsMsg::NewArchiveUseLast(v) => {
                draft.new_archive_use_last = v;
                Task::none()
            }
            SettingsMsg::NewArchiveFolderChanged(s) => {
                draft.new_archive_folder = s;
                Task::none()
            }
            SettingsMsg::BrowseNewArchiveFolder => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::SettingsBrowseNewArchiveFolder,
            ),
            SettingsMsg::ExcludeFilterChanged(s) => {
                draft.exclude_filter = s;
                Task::none()
            }
            SettingsMsg::ColSize(v) => {
                draft.columns.size = v;
                Task::none()
            }
            SettingsMsg::ColCompressed(v) => {
                draft.columns.compressed = v;
                Task::none()
            }
            SettingsMsg::ColMethod(v) => {
                draft.columns.method = v;
                Task::none()
            }
            SettingsMsg::ColModified(v) => {
                draft.columns.modified = v;
                Task::none()
            }
            SettingsMsg::ColKind(v) => {
                draft.columns.kind = v;
                Task::none()
            }
            SettingsMsg::ColorAccent(accent) => {
                draft.color_accent = accent;
                Task::none()
            }
            SettingsMsg::CopyZoneId(v) => {
                draft.copy_zone_identifier = v;
                Task::none()
            }
            SettingsMsg::ShowRecent(v) => {
                draft.show_recent = v;
                Task::none()
            }
            SettingsMsg::UseLongPaths(v) => {
                draft.use_long_paths = v;
                Task::none()
            }
            SettingsMsg::DeleteOnCancel(v) => {
                draft.delete_on_extract_cancel = v;
                Task::none()
            }
            SettingsMsg::TitleFullPath(v) => {
                draft.title_full_path = v;
                Task::none()
            }
            SettingsMsg::AutoExpandTree(v) => {
                draft.auto_expand_tree = v;
                Task::none()
            }
            SettingsMsg::DeleteToTrash(v) => {
                draft.delete_to_trash = v;
                Task::none()
            }
            SettingsMsg::ShowStartScreen(v) => {
                draft.show_start_screen = v;
                Task::none()
            }
            SettingsMsg::DeleteTempAsap(v) => {
                draft.delete_temp_asap = v;
                Task::none()
            }
            SettingsMsg::TempFolderChanged(s) => {
                draft.temp_folder = s;
                Task::none()
            }
            SettingsMsg::BrowseTempFolder => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::SettingsBrowseTemp,
            ),
            SettingsMsg::ExportPrefs => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("JSON", &["json"])
                        .set_file_name("oxizip-prefs.json")
                        .save_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::SettingsExportPath,
            ),
            SettingsMsg::ImportPrefs => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("JSON", &["json"])
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::SettingsImportPath,
            ),
            SettingsMsg::ClearRecent => {
                draft.clear_recent();
                Task::none()
            }
        }
    }

    fn theme(&self) -> Theme {
        self.ui_theme.clone()
    }

    fn title(&self) -> String {
        if self.settings_mode || self.screen == Screen::Settings {
            return format!(
                "{} - {}",
                t(self.lang, "gui.settings.title"),
                t(self.lang, "app.name")
            );
        }
        if self.screen == Screen::Job {
            let pct = (self.progress_fraction * 100.0).round() as i32;
            let name = self
                .job_out
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let verb = match (self.job_verb, self.job_finished) {
                (job::JobVerb::Create, false) => t(self.lang, "gui.busy.create"),
                (job::JobVerb::Create, true) => t(self.lang, "gui.status.created"),
                (job::JobVerb::Extract, false) => t(self.lang, "gui.busy.extract"),
                (job::JobVerb::Extract, true) => t(self.lang, "gui.status.extracted"),
            };
            return format!("{pct}% {verb} {name} - {}", t(self.lang, "app.name"));
        }
        if self.prefs.title_full_path {
            if let Some(path) = self.archive.as_ref() {
                return format!("{} - {}", path.display(), t(self.lang, "app.name"));
            }
        }
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
        let exclude_patterns = self
            .prefs
            .exclude_filter
            .split([';', ','])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        CreateOptions {
            password: self.password_opt(),
            method: self.create_method.to_option(),
            level: self.prefs.compression_level.map(u32::from),
            exclude_thumbs_db: self.prefs.exclude_thumbs_db,
            no_root_for_single_folder: self.prefs.no_root_for_single_folder,
            utf8_names: self.prefs.utf8_zip_names,
            exclude_patterns,
            ..CreateOptions::default()
        }
    }

    fn extract_runtime_prefs(&self) -> worker::ExtractRuntimePrefs {
        worker::ExtractRuntimePrefs {
            copy_zone_identifier: self.prefs.copy_zone_identifier,
            delete_on_cancel: self.prefs.delete_on_extract_cancel,
            smart_single_file_folder: self.prefs.smart_single_file_folder,
        }
    }

    fn maybe_long_path(&self, path: PathBuf) -> PathBuf {
        #[cfg(windows)]
        {
            if !self.prefs.use_long_paths {
                return path;
            }
            let s = path.to_string_lossy();
            if s.len() < 240 || s.starts_with(r"\\?\") {
                return path;
            }
            if s.starts_with(r"\\") {
                return PathBuf::from(format!(r"\\?\UNC\{}", s.trim_start_matches(r"\\")));
            }
            return PathBuf::from(format!(r"\\?\{s}"));
        }
        #[cfg(not(windows))]
        {
            path
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
        self.progress_file_fraction = 0.0;
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
        let need_tick = self.busy.is_some()
            || self.settings_child.is_some()
            || self
                .prefs_watch_until
                .is_some_and(|until| std::time::Instant::now() < until);
        let tick = if need_tick {
            iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        };
        let esc = if self.prefs.esc_to_exit || self.settings_mode {
            iced::keyboard::on_key_press(|key, _mods| match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                    Some(Message::Close)
                }
                _ => None,
            })
        } else {
            Subscription::none()
        };
        Subscription::batch([tick, esc])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowId(id) => {
                self.window_id = id;
                let round = match id {
                    Some(id) => window::get_raw_id::<Message>(id).map(Message::RawWindowId),
                    None => Task::none(),
                };
                let close = if self.job_pending_close {
                    self.job_pending_close = false;
                    self.close_job_window()
                } else {
                    Task::none()
                };
                Task::batch([round, close])
            }
            Message::RawWindowId(raw) => {
                self.main_hwnd = Some(raw);
                apply_native_window_rounding(raw);
                // Settings child may have started before HWND was known.
                if self.settings_child.is_some() {
                    shell_ops::set_native_window_enabled(raw, false);
                }
                Task::none()
            }
            Message::SettingsModalNop => Task::none(),
            Message::TitleDrag => {
                if self.settings_child.is_some() {
                    return Task::none();
                }
                self.window_id.map(window::drag).unwrap_or_else(Task::none)
            }
            Message::Minimize => {
                if self.settings_child.is_some() {
                    return Task::none();
                }
                self.window_id
                    .map(|id| window::minimize(id, true))
                    .unwrap_or_else(Task::none)
            }
            Message::Close => {
                if self.settings_child.is_some() {
                    // Modal settings open: ignore main close.
                    return Task::none();
                }
                if self.screen == Screen::Job {
                    if let Some(busy) = &self.busy {
                        busy.cancel.cancel();
                    }
                }
                self.window_id.map(window::close).unwrap_or_else(Task::none)
            }
            Message::JobCloseWhenDone(on) => {
                self.job_close_when_done = on;
                Task::none()
            }
            Message::JobOpenFolder(on) => {
                self.job_open_folder = on;
                Task::none()
            }
            Message::JobConflictOverwrite => {
                self.continue_extract_after_conflict(ConflictPolicy::Overwrite)
            }
            Message::JobConflictRename => {
                self.continue_extract_after_conflict(ConflictPolicy::AutoRename)
            }
            Message::JobConflictCancel => {
                self.job_conflicts.clear();
                self.pending_shell_extract = None;
                self.pending_main_extract = None;
                self.busy = None;
                self.job_finished = true;
                self.status = t(self.lang, "gui.status.canceled").to_string();
                if self.screen == Screen::Job {
                    self.close_job_window()
                } else {
                    Task::none()
                }
            }
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
            Message::Settings(msg) => self.handle_settings(msg),
            Message::SettingsBrowseTemp(path) => {
                if let Some(path) = path {
                    self.settings_draft.temp_folder = path.to_string_lossy().into_owned();
                }
                Task::none()
            }
            Message::SettingsBrowseNewArchiveFolder(path) => {
                if let Some(path) = path {
                    self.settings_draft.new_archive_folder = path.to_string_lossy().into_owned();
                    self.settings_draft.new_archive_use_last = false;
                }
                Task::none()
            }
            Message::SettingsExportPath(path) => {
                if let Some(path) = path {
                    match self.settings_draft.export_to(&path) {
                        Ok(()) => {
                            self.set_settings_status(t(self.lang, "gui.settings.status.export_ok"));
                        }
                        Err(e) => {
                            self.last_error = Some(e);
                        }
                    }
                }
                Task::none()
            }
            Message::SettingsImportPath(path) => {
                if let Some(path) = path {
                    match Prefs::import_from(&path) {
                        Ok(prefs) => {
                            self.settings_draft = prefs;
                            self.set_settings_status(t(self.lang, "gui.settings.status.import_ok"));
                        }
                        Err(e) => {
                            self.last_error = Some(e);
                        }
                    }
                }
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
                self.ui_theme = theme::build(self.prefs.theme_mode, accent);
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
            Message::PasswordChanged(value) => {
                self.password = value;
                Task::none()
            }
            Message::ArchiveChosen(Some(path)) => self.start_list(path),
            Message::ArchiveChosen(None) => Task::none(),
            Message::ExtractDirChosen(Some(dir)) => self.start_extract(dir),
            Message::ExtractDirChosen(None) => Task::none(),
            Message::ShellExtractDialogChosen(Some(dir)) => {
                let Some(mut spec) = self.pending_shell_extract.take() else {
                    return Task::none();
                };
                spec.dest_override = Some(dir);
                self.pending_shell_extract = Some(spec.clone());
                self.run_shell_extract(spec, None)
            }
            Message::ShellExtractDialogChosen(None) => {
                self.pending_shell_extract = None;
                self.job_finished = true;
                self.status = t(self.lang, "gui.status.canceled").to_string();
                self.close_job_window()
            }
            Message::CreateInputsChosen(Some(inputs)) => {
                if inputs.is_empty() {
                    self.last_error = Some(t(self.lang, "gui.err.need_inputs").into());
                    return Task::none();
                }
                self.pending_create_inputs = Some(inputs);
                let fmt = self.create_format;
                let initial = self.prefs.new_archive_dir();
                Task::perform(
                    pick_create_output(fmt, initial),
                    Message::CreateOutputChosen,
                )
            }
            Message::CreateInputsChosen(None) => Task::none(),
            Message::CreateOutputChosen(Some(out)) => {
                self.prefs.remember_new_archive_dir(&out);
                self.save_prefs();
                self.start_create(out)
            }
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
                    self.progress_file_fraction = snap.file_fraction;
                    self.progress_detail = snap.detail;
                }
                let settings_done = self.poll_settings_child();
                if self
                    .prefs_watch_until
                    .is_some_and(|until| std::time::Instant::now() >= until)
                    && self.settings_child.is_none()
                {
                    self.prefs_watch_until = None;
                }
                Task::batch([settings_done, self.maybe_reload_prefs_from_disk()])
            }
            Message::JobFinished(outcome) => {
                self.busy = None;
                if matches!(outcome, JobOutcome::NeedsConflictChoice { .. }) {
                    // Keep progress detail; conflict UI needs pending extract state.
                    if self.screen == Screen::Job {
                        return self.apply_shell_job_outcome(outcome);
                    }
                    return self.apply_outcome(outcome);
                }
                if self.screen == Screen::Job {
                    self.apply_shell_job_outcome(outcome)
                } else {
                    self.progress_fraction = 0.0;
                    self.progress_file_fraction = 0.0;
                    self.progress_detail.clear();
                    self.apply_outcome(outcome)
                }
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
        let dir = self.maybe_long_path(dir);
        self.pending_main_extract = Some((archive.clone(), dir.clone()));
        self.job_conflicts.clear();
        let (cancel, progress) = self.begin_busy(JobKind::Extract);
        let password = self.password_opt();
        let prefs = self.extract_runtime_prefs();
        let lang = self.lang;
        spawn_job(
            JobKind::Extract,
            cancel,
            progress,
            move |cancel, progress| {
                worker::extract_archive(archive, dir, password, None, prefs, cancel, progress, lang)
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
        let dest = job::dest_smart(&archive, &self.entries, self.prefs.smart_single_file_folder);
        self.start_extract(self.maybe_long_path(dest))
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

    fn start_shell_create(&mut self, spec: job::CreateJob) -> Task<Message> {
        self.screen = Screen::Job;
        self.job_verb = job::JobVerb::Create;
        self.job_out = Some(spec.out.clone());
        self.job_started = Some(std::time::Instant::now());
        self.job_finished = false;
        self.job_close_when_done = !self.prefs.job_keep_open_after_create;
        self.job_open_folder = self.prefs.open_dest_after_extract;
        self.create_format = spec.format;
        let format = spec.format;
        let opts = self.create_options();
        let lang = self.lang;
        let (cancel, progress) = self.begin_busy(JobKind::Create);
        if spec.each {
            let inputs = spec.inputs;
            let ext = format.as_str().to_string();
            spawn_job(
                JobKind::Create,
                cancel,
                progress,
                move |cancel, progress| {
                    worker::create_archive_each(inputs, format, ext, opts, cancel, progress, lang)
                },
            )
        } else {
            let out = spec.out;
            let inputs = spec.inputs;
            spawn_job(
                JobKind::Create,
                cancel,
                progress,
                move |cancel, progress| {
                    worker::create_archive(out, format, inputs, opts, cancel, progress, lang)
                },
            )
        }
    }

    fn start_shell_extract(&mut self, spec: job::ExtractJob) -> Task<Message> {
        self.screen = Screen::Job;
        self.job_verb = job::JobVerb::Extract;
        self.job_out = spec.archives.first().cloned();
        self.job_started = Some(std::time::Instant::now());
        self.job_finished = false;
        self.job_close_when_done = !self.prefs.job_keep_open_after_extract;
        self.job_open_folder = self.prefs.open_dest_after_extract;
        self.job_conflicts.clear();
        if matches!(spec.mode, job::ExtractMode::Dialog) && spec.dest_override.is_none() {
            self.pending_shell_extract = Some(spec);
            return Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::ShellExtractDialogChosen,
            );
        }
        self.pending_shell_extract = Some(spec.clone());
        self.run_shell_extract(spec, None)
    }

    fn run_shell_extract(
        &mut self,
        spec: job::ExtractJob,
        conflict: Option<ConflictPolicy>,
    ) -> Task<Message> {
        let archives = spec.archives;
        let mode = spec.mode;
        let dest_override = spec.dest_override;
        let password = self.password_opt();
        let prefs = self.extract_runtime_prefs();
        let lang = self.lang;
        let (cancel, progress) = self.begin_busy(JobKind::Extract);
        spawn_job(
            JobKind::Extract,
            cancel,
            progress,
            move |cancel, progress| {
                worker::extract_shell_job(
                    archives,
                    mode,
                    dest_override,
                    password,
                    conflict,
                    prefs,
                    cancel,
                    progress,
                    lang,
                )
            },
        )
    }

    fn continue_extract_after_conflict(&mut self, policy: ConflictPolicy) -> Task<Message> {
        self.job_conflicts.clear();
        if let Some(spec) = self.pending_shell_extract.clone() {
            return self.run_shell_extract(spec, Some(policy));
        }
        if let Some((archive, dest)) = self.pending_main_extract.clone() {
            let password = self.password_opt();
            let prefs = self.extract_runtime_prefs();
            let lang = self.lang;
            let (cancel, progress) = self.begin_busy(JobKind::Extract);
            return spawn_job(
                JobKind::Extract,
                cancel,
                progress,
                move |cancel, progress| {
                    worker::extract_archive(
                        archive,
                        dest,
                        password,
                        Some(policy),
                        prefs,
                        cancel,
                        progress,
                        lang,
                    )
                },
            );
        }
        Task::none()
    }

    fn close_job_window(&mut self) -> Task<Message> {
        if let Some(id) = self.window_id {
            return window::close(id);
        }
        self.job_pending_close = true;
        window::get_oldest().then(|id| match id {
            Some(id) => window::close(id),
            None => Task::none(),
        })
    }

    fn apply_shell_job_outcome(&mut self, outcome: JobOutcome) -> Task<Message> {
        match outcome {
            JobOutcome::NeedsConflictChoice { conflicts } => {
                self.job_finished = false;
                self.progress_fraction = 0.0;
                self.progress_file_fraction = 0.0;
                self.job_conflicts = conflicts;
                self.status = t(self.lang, "gui.job.conflict_title").to_string();
                Task::none()
            }
            JobOutcome::Canceled => {
                self.job_finished = true;
                self.progress_fraction = 1.0;
                self.progress_file_fraction = 1.0;
                self.pending_shell_extract = None;
                self.status = t(self.lang, "gui.status.canceled").to_string();
                self.last_error = None;
                self.close_job_window()
            }
            JobOutcome::Created { path, error, .. } => {
                self.job_finished = true;
                self.progress_fraction = 1.0;
                self.progress_file_fraction = 1.0;
                if let Some(err) = error {
                    self.last_error = Some(err);
                    self.status = t(self.lang, "gui.job.failed").to_string();
                    self.progress_detail.clear();
                    return Task::none();
                }
                self.last_error = None;
                self.status = t(self.lang, "gui.status.created").to_string();
                self.job_out = Some(path.clone());
                if self.job_open_folder {
                    let _ = shell_ops::reveal_in_file_manager(&path);
                }
                if self.job_close_when_done {
                    self.close_job_window()
                } else {
                    Task::none()
                }
            }
            JobOutcome::Extracted { dest, error } => {
                self.job_finished = true;
                self.progress_fraction = 1.0;
                self.progress_file_fraction = 1.0;
                let delete_archives = self
                    .pending_shell_extract
                    .as_ref()
                    .map(|s| s.delete_archive)
                    .unwrap_or(false);
                let archives = self
                    .pending_shell_extract
                    .as_ref()
                    .map(|s| s.archives.clone())
                    .unwrap_or_default();
                self.pending_shell_extract = None;
                self.job_conflicts.clear();
                if let Some(err) = error {
                    self.last_error = Some(err);
                    self.status = t(self.lang, "gui.job.extract_failed").to_string();
                    self.progress_detail.clear();
                    return Task::none();
                }
                if delete_archives {
                    for archive in archives {
                        let _ = if self.prefs.delete_to_trash {
                            shell_ops::trash_file(&archive)
                        } else {
                            shell_ops::delete_file_permanent(&archive)
                        };
                    }
                }
                self.last_error = None;
                self.status = t(self.lang, "gui.status.extracted").to_string();
                self.job_out = Some(dest.clone());
                if self.job_open_folder {
                    let _ = shell_ops::open_folder(&dest);
                }
                if self.job_close_when_done {
                    self.close_job_window()
                } else {
                    Task::none()
                }
            }
            other => {
                self.job_finished = true;
                self.last_error = Some(format!("{other:?}"));
                Task::none()
            }
        }
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
        let temp_folder = self.prefs.temp_folder.clone();
        let copy_zone = self.prefs.copy_zone_identifier;
        let lang = self.lang;
        spawn_job(
            JobKind::OpenEntry,
            cancel,
            progress,
            move |cancel, progress| {
                worker::open_entry(
                    archive,
                    entry,
                    password,
                    temp_folder,
                    copy_zone,
                    cancel,
                    progress,
                    lang,
                )
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
                    if self.prefs.enter_single_root_folder {
                        let tops: std::collections::BTreeSet<String> = self
                            .entries
                            .iter()
                            .filter_map(|e| {
                                let name = e.name.trim_matches('/');
                                if name.is_empty() {
                                    None
                                } else {
                                    Some(name.split('/').next().unwrap_or(name).to_string())
                                }
                            })
                            .collect();
                        if tops.len() == 1 {
                            if let Some(root) = tops.into_iter().next() {
                                if self
                                    .entries
                                    .iter()
                                    .any(|e| e.is_dir && e.name.trim_matches('/') == root)
                                {
                                    self.tree_prefix = Some(format!("{root}/"));
                                }
                            }
                        }
                    }
                    if self.prefs.auto_expand_tree {
                        self.prefs.show_tree = true;
                    }
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
            JobOutcome::NeedsConflictChoice { conflicts } => {
                self.job_conflicts = conflicts;
                self.status = t(self.lang, "gui.job.conflict_title").to_string();
                Task::none()
            }
            JobOutcome::Extracted { dest, error } => {
                self.pending_main_extract = None;
                self.job_conflicts.clear();
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
                    if self.prefs.test_after_compress {
                        self.pending_open = Some(path.clone());
                        let (cancel, progress) = self.begin_busy(JobKind::Test);
                        let password = self.password_opt();
                        let lang = self.lang;
                        spawn_job(JobKind::Test, cancel, progress, move |cancel, progress| {
                            worker::test_archive_job(path, password, cancel, progress, lang)
                        })
                    } else {
                        self.start_list(path)
                    }
                }
            }
            JobOutcome::Opened { name, temp, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                } else if self.prefs.delete_temp_asap {
                    // Keep until next open so the system app can still read the file.
                    self.open_temp = temp;
                    self.last_error = None;
                    self.status = format!("{} {}", t(self.lang, "gui.status.opened"), name);
                } else {
                    // Leave files on disk (disable auto-clean of this temp dir).
                    if let Some(t) = temp {
                        if let Ok(dir) = Arc::try_unwrap(t) {
                            let _ = dir.keep();
                        }
                    }
                    self.open_temp = None;
                    self.last_error = None;
                    self.status = format!("{} {}", t(self.lang, "gui.status.opened"), name);
                }
                Task::none()
            }
            JobOutcome::Tested { path, error } => {
                if let Some(err) = error {
                    self.last_error = Some(err);
                    self.pending_open = None;
                    Task::none()
                } else {
                    self.last_error = None;
                    self.status =
                        format!("{} {}", t(self.lang, "gui.status.tested"), path.display());
                    if let Some(open_path) = self.pending_open.take() {
                        self.start_list(open_path)
                    } else {
                        Task::none()
                    }
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
                    if self.prefs.test_after_modify {
                        self.pending_open = Some(path.clone());
                        self.start_test()
                    } else {
                        self.start_list(path)
                    }
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
                    if self.prefs.test_after_modify {
                        self.pending_open = Some(path.clone());
                        self.start_test()
                    } else {
                        self.start_list(path)
                    }
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
                | MenuAction::OpenSettingsTheme
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
            MenuAction::ShowSettings => self.open_settings_modal(None),
            MenuAction::SetLang(lang) => self.set_lang(lang),
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
            MenuAction::ToggleColKind => {
                self.prefs.columns.kind = !self.prefs.columns.kind;
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
                self.last_error = None;
                self.open_settings_modal(Some("assoc"))
            }
            MenuAction::OpenSettingsLang => {
                // Keep Settings → Language as a cascade submenu (not the Settings page Lang tab).
                self.menu_popup = if self.menu_popup == MenuPopup::SettingsLang {
                    MenuPopup::Settings
                } else {
                    MenuPopup::SettingsLang
                };
                Task::none()
            }
            MenuAction::OpenSettingsTheme => {
                self.menu_popup = if self.menu_popup == MenuPopup::SettingsTheme {
                    MenuPopup::Settings
                } else {
                    MenuPopup::SettingsTheme
                };
                Task::none()
            }
            MenuAction::SetThemeMode(mode) => {
                self.prefs.theme_mode = mode;
                self.prefs.save();
                self.ui_theme = theme::build(self.prefs.theme_mode, self.prefs.color_accent);
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
        let use_trash = !permanent && self.prefs.delete_to_trash;
        let result = if use_trash {
            shell_ops::trash_file(&path)
        } else {
            shell_ops::delete_file_permanent(&path)
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
            Screen::Settings => self.view_settings(),
            Screen::Job => self.view_job(),
        };
        let with_conflict = if self.screen != Screen::Job && !self.job_conflicts.is_empty() {
            stack![page, self.view_conflict_modal()]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            page
        };
        let with_overlay = if self.overlay == Overlay::None {
            with_conflict
        } else {
            stack![with_conflict, self.view_overlay()]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };
        // Settings child open: keep main painted, but capture all input.
        let inner = if self.settings_child.is_some() {
            stack![
                with_overlay,
                mouse_area(
                    container(Space::new(Length::Fill, Length::Fill))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(theme::scrim)
                )
                .on_press(Message::SettingsModalNop)
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            with_overlay
        };
        self.framed_window(inner)
    }

    fn framed_window<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        let caption = caption_bar(
            self.title(),
            self.icons.oxizip(),
            Message::TitleDrag,
            Message::Minimize,
            Message::Close,
        );
        // Clip children to the rounded outer frame so resize (settings ↔ main) does not
        // leave square content corners poking through the transparent window.
        container(
            column![caption, body]
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(frame_border)
        .clip(true)
        .into()
    }

    fn setup_all_selected(&self) -> bool {
        !self.setup_exts.is_empty() && self.setup_exts.values().all(|on| *on)
    }

    fn view_setup(&self) -> Element<'_, Message> {
        const PER_ROW: usize = 6;
        let mut groups = Column::new().spacing(12).width(Length::Fill);
        let catalog = setup::assoc_catalog();
        let last_group = catalog.len().saturating_sub(1);
        for (gi, (group, exts)) in catalog.iter().enumerate() {
            let mut chips = Column::new().spacing(6).width(Length::Fill);
            for chunk in exts.chunks(PER_ROW) {
                let mut row_exts = row![].spacing(8).align_y(Alignment::Center);
                for ext in chunk {
                    let e = *ext;
                    let checked = self.setup_exts.get(e).copied().unwrap_or(false);
                    row_exts = row_exts.push(
                        checkbox(format!(".{e}"), checked)
                            .on_toggle(move |on| Message::SetupExtToggled(e, on))
                            .size(16)
                            .text_size(12)
                            .spacing(8)
                            .style(theme::checkbox_style)
                            .width(Length::FillPortion(1)),
                    );
                }
                for _ in chunk.len()..PER_ROW {
                    row_exts = row_exts.push(Space::with_width(Length::FillPortion(1)));
                }
                chips = chips.push(row_exts);
            }
            groups = groups.push(
                column![
                    text(t(self.lang, group.i18n_key()))
                        .size(12)
                        .style(theme::text_secondary),
                    chips,
                ]
                .spacing(6)
                .width(Length::Fill)
                .align_x(Alignment::Start),
            );
            if gi != last_group {
                groups = groups.push(
                    container(Space::with_height(1))
                        .width(Length::Fill)
                        .style(theme::divider),
                );
            }
        }

        let list = container(
            scrollable(groups.padding(14))
                .style(theme::scrollable_style)
                .height(Length::Fill)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::panel);

        let err: Element<'_, Message> = if let Some(e) = &self.last_error {
            container(text(e.as_str()).size(12).style(theme::text_danger))
                .padding([6, 10])
                .width(Length::Fill)
                .style(theme::error_banner)
                .into()
        } else {
            Space::with_height(0).into()
        };

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
                .style(rounded_secondary),
            Space::with_width(Length::Fill),
            button(text(t(self.lang, "gui.setup.skip")).size(12))
                .padding([6, 10])
                .height(32)
                .on_press(Message::SetupSkip)
                .style(rounded_text_btn),
            button(text(t(self.lang, "gui.setup.apply")).size(13))
                .padding([6, 20])
                .height(32)
                .on_press(Message::SetupConfirm)
                .style(rounded_primary),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let body = column![
            column![
                text(t(self.lang, "gui.setup.title"))
                    .size(16)
                    .style(theme::text_primary)
                    .width(Length::Fill),
                text(t(self.lang, "gui.setup.intro"))
                    .size(12)
                    .style(theme::text_secondary)
                    .width(Length::Fill),
            ]
            .spacing(4),
            list,
            err,
            actions,
        ]
        .spacing(12)
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
        let mut filter_pills = row![].spacing(4).align_y(Alignment::Center);
        for f in LicenseFilter::all() {
            let filter = *f;
            let active = self.license_filter == filter;
            filter_pills = filter_pills.push(
                button(text(filter.label(self.lang)).size(11))
                    .padding([5, 10])
                    .style(move |th, st| {
                        let mut s = theme::btn_ghost(th, st);
                        let tk = theme::tokens(th);
                        if active {
                            s.background = Some(Background::Color(tk.accent_soft));
                            s.text_color = tk.accent;
                        } else {
                            s.text_color = tk.text_2;
                        }
                        s.border.radius = theme::SMALL_RADIUS.into();
                        s
                    })
                    .on_press(Message::LicenseFilterSelected(filter)),
            );
        }

        let toolbar = row![
            button(text(t(self.lang, "gui.licenses.back")).size(12))
                .padding([6, 12])
                .style(rounded_secondary)
                .on_press(Message::HideLicenses),
            Space::with_width(10),
            text(t(self.lang, "gui.licenses.title"))
                .size(16)
                .style(theme::text_primary),
            Space::with_width(Length::Fill),
            filter_pills,
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let intro = text(t(self.lang, "gui.licenses.intro"))
            .size(11)
            .style(theme::text_secondary);

        let header = container(
            row![
                text(t(self.lang, "gui.licenses.col.kind"))
                    .size(11)
                    .style(theme::text_secondary)
                    .width(Length::FillPortion(2)),
                text(t(self.lang, "gui.licenses.col.name"))
                    .size(11)
                    .style(theme::text_secondary)
                    .width(Length::FillPortion(4)),
                text(t(self.lang, "gui.licenses.col.license"))
                    .size(11)
                    .style(theme::text_secondary)
                    .width(Length::FillPortion(3)),
                text(t(self.lang, "gui.licenses.col.purpose"))
                    .size(11)
                    .style(theme::text_secondary)
                    .width(Length::FillPortion(4)),
                text(t(self.lang, "gui.licenses.col.note"))
                    .size(11)
                    .style(theme::text_secondary)
                    .width(Length::FillPortion(4)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([7, 10])
        .width(Length::Fill)
        .style(list_header_style);

        let items: Vec<_> = catalog()
            .iter()
            .filter(|c| self.license_filter.matches(c.kind))
            .collect();
        let last = items.len().saturating_sub(1);
        let mut list_col = Column::new();
        for (i, item) in items.into_iter().enumerate() {
            let kind_badge = container(
                text(item.kind.label(self.lang))
                    .size(10)
                    .style(theme::text_secondary),
            )
            .padding([2, 6])
            .style(|th| theme::pill(th, false));
            let lic_badge = container(text(item.license).size(10).style(theme::text_primary))
                .padding([2, 6])
                .style(|th| theme::pill(th, true));

            list_col = list_col.push(
                container(
                    row![
                        container(kind_badge)
                            .width(Length::FillPortion(2))
                            .align_x(Alignment::Start),
                        text(item.name)
                            .size(12)
                            .style(theme::text_primary)
                            .width(Length::FillPortion(4)),
                        container(lic_badge)
                            .width(Length::FillPortion(3))
                            .align_x(Alignment::Start),
                        text(item.purpose(self.lang))
                            .size(12)
                            .style(theme::text_secondary)
                            .width(Length::FillPortion(4)),
                        text(item.note(self.lang))
                            .size(11)
                            .style(theme::text_muted)
                            .width(Length::FillPortion(4)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([7, 10])
                .width(Length::Fill),
            );
            if i != last {
                list_col = list_col.push(
                    container(Space::with_height(1))
                        .width(Length::Fill)
                        .style(theme::divider),
                );
            }
        }

        let table = container(
            column![
                header,
                container(Space::with_height(1))
                    .width(Length::Fill)
                    .style(theme::divider),
                scrollable(list_col)
                    .style(theme::scrollable_style)
                    .height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::panel);

        let body = column![
            toolbar,
            intro,
            table,
            text(t(self.lang, "gui.licenses.footer"))
                .size(11)
                .style(theme::text_muted),
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

    fn view_job(&self) -> Element<'_, Message> {
        if !self.job_conflicts.is_empty() {
            return self.view_job_conflict();
        }

        let pct = (self.progress_fraction * 100.0).round() as i32;
        let file_pct = (self.progress_file_fraction * 100.0).round() as i32;
        let (elapsed, total) = match self.job_started {
            Some(started) => job::eta_pair(started, self.progress_fraction),
            None => ("00:00:00".into(), "--:--:--".into()),
        };
        let (file_elapsed, file_total) = match self.job_started {
            Some(started) => job::eta_pair(started, self.progress_file_fraction),
            None => ("00:00:00".into(), "--:--:--".into()),
        };
        let current = if self.progress_detail.is_empty() {
            self.job_out
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        } else {
            // Show basename for long paths.
            Path::new(&self.progress_detail)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.progress_detail.clone())
        };

        let overall = column![
            row![
                text(t(self.lang, "gui.job.overall"))
                    .size(11)
                    .style(theme::text_secondary),
                Space::with_width(8),
                text(format!("{pct}%")).size(11).style(theme::text_primary),
                Space::with_width(Length::Fill),
                text(format!("{elapsed} / {total}"))
                    .size(11)
                    .style(theme::text_muted),
            ]
            .align_y(Alignment::Center),
            progress_bar(0.0..=1.0, self.progress_fraction)
                .height(10)
                .style(theme::progress_style),
        ]
        .spacing(4);

        let file_bar = column![
            row![
                text(t(self.lang, "gui.job.current"))
                    .size(11)
                    .style(theme::text_secondary),
                Space::with_width(8),
                text(format!("{file_pct}%"))
                    .size(11)
                    .style(theme::text_primary),
                Space::with_width(Length::Fill),
                text(format!("{file_elapsed} / {file_total}"))
                    .size(11)
                    .style(theme::text_muted),
            ]
            .align_y(Alignment::Center),
            progress_bar(0.0..=1.0, self.progress_file_fraction)
                .height(10)
                .style(theme::progress_style),
        ]
        .spacing(4);

        let err: Element<'_, Message> = if let Some(err) = &self.last_error {
            container(text(err.clone()).size(11).style(theme::text_danger))
                .padding([4, 8])
                .width(Length::Fill)
                .style(theme::error_banner)
                .into()
        } else {
            Space::with_height(0).into()
        };

        let stop_label = if self.job_finished {
            t(self.lang, "gui.job.close")
        } else {
            t(self.lang, "gui.job.stop")
        };
        let stop_msg = if self.is_busy() {
            Message::CancelClicked
        } else {
            Message::Close
        };

        let actions = column![
            container(Space::with_height(1))
                .width(Length::Fill)
                .style(theme::divider),
            row![
                checkbox(
                    t(self.lang, "gui.job.close_when_done"),
                    self.job_close_when_done
                )
                .on_toggle(Message::JobCloseWhenDone)
                .size(15)
                .text_size(11)
                .spacing(6)
                .style(theme::checkbox_style),
                Space::with_width(8),
                checkbox(t(self.lang, "gui.job.open_folder"), self.job_open_folder)
                    .on_toggle(Message::JobOpenFolder)
                    .size(15)
                    .text_size(11)
                    .spacing(6)
                    .style(theme::checkbox_style),
                Space::with_width(Length::Fill),
                button(text(stop_label).size(12))
                    .padding([6, 16])
                    .style(rounded_secondary)
                    .on_press(stop_msg),
            ]
            .align_y(Alignment::Center)
            .spacing(4),
        ]
        .spacing(8);

        container(
            column![
                overall,
                Space::with_height(6),
                file_bar,
                Space::with_height(4),
                text(current).size(11).style(theme::text_secondary),
                err,
                Space::with_height(Length::Fill),
                actions,
            ]
            .spacing(4)
            .padding(14)
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn conflict_prompt_column(&self) -> Column<'_, Message> {
        let sample = self
            .job_conflicts
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = if self.job_conflicts.len() > 3 {
            format!(" …(+{})", self.job_conflicts.len().saturating_sub(3))
        } else {
            String::new()
        };
        let body = format!(
            "{}{}{}",
            t(self.lang, "gui.job.conflict_body"),
            sample,
            more
        );
        column![
            text(t(self.lang, "gui.job.conflict_title")).size(14),
            Space::with_height(6),
            text(body).size(12),
            Space::with_height(12),
            row![
                button(text(t(self.lang, "gui.job.conflict_overwrite")).size(12))
                    .padding([8, 12])
                    .style(rounded_primary)
                    .on_press(Message::JobConflictOverwrite),
                button(text(t(self.lang, "gui.job.conflict_rename")).size(12))
                    .padding([8, 12])
                    .style(rounded_secondary)
                    .on_press(Message::JobConflictRename),
                Space::with_width(Length::Fill),
                button(text(t(self.lang, "gui.job.conflict_cancel")).size(12))
                    .padding([8, 12])
                    .style(rounded_secondary)
                    .on_press(Message::JobConflictCancel),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(4)
    }

    fn view_job_conflict(&self) -> Element<'_, Message> {
        container(
            self.conflict_prompt_column()
                .padding(12)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn view_conflict_modal(&self) -> Element<'_, Message> {
        container(
            container(self.conflict_prompt_column().padding(16).width(420)).style(theme::elevated),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::scrim)
        .into()
    }

    fn view_main(&self) -> Element<'_, Message> {
        let body = if self.archive.is_none() {
            if self.prefs.show_start_screen {
                self.view_home()
            } else {
                self.view_archive()
            }
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

        let brand_header = column![
            row![
                image(self.icons.oxizip()).width(28).height(28),
                text("Oxizip").size(18).style(theme::text_primary),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(t(self.lang, "gui.home.hint"))
                .size(12)
                .style(theme::text_secondary),
        ]
        .spacing(6)
        .align_x(Alignment::Center);

        let open_tile = home_glass_tile(
            self.icons.home_open(),
            t(self.lang, "gui.home.open"),
            t(self.lang, "gui.home.open_desc"),
            if busy {
                None
            } else {
                Some(Message::OpenClicked)
            },
        );
        let create_tile = home_glass_tile(
            self.icons.home_create(),
            t(self.lang, "gui.home.create"),
            t(self.lang, "gui.home.create_desc"),
            if busy {
                None
            } else {
                Some(Message::CreateClicked)
            },
        );

        let tiles = row![
            Space::with_width(Length::Fill),
            open_tile,
            Space::with_width(20),
            create_tile,
            Space::with_width(Length::Fill),
        ]
        .align_y(Alignment::Center);

        let err: Element<'_, Message> = if let Some(e) = &self.last_error {
            container(text(e.clone()).size(12).style(theme::text_danger))
                .padding([6, 12])
                .style(theme::error_banner)
                .into()
        } else {
            Space::with_height(0).into()
        };

        let progress_area: Element<'_, Message> = if busy {
            container(
                column![
                    progress_bar(0.0..=1.0, self.progress_fraction)
                        .height(8)
                        .style(theme::progress_style),
                    row![
                        text(self.progress_detail.clone())
                            .size(11)
                            .style(theme::text_secondary)
                            .width(Length::Fill),
                        button(text(t(self.lang, "gui.cancel")).size(11))
                            .padding([4, 12])
                            .style(rounded_secondary)
                            .on_press(Message::CancelClicked),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            )
            .width(360)
            .padding(10)
            .style(theme::panel)
            .into()
        } else {
            Space::with_height(0).into()
        };

        let stage = column![
            Space::with_height(Length::Fill),
            brand_header,
            Space::with_height(24),
            tiles,
            Space::with_height(Length::Fill),
            err,
            progress_area,
            Space::with_height(18),
        ]
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center);

        container(stage)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::home_stage)
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

        let options = container(
            row![
                text(t(self.lang, "gui.format"))
                    .size(11)
                    .style(theme::text_secondary),
                pick_list(
                    formats,
                    Some(FormatLabel(self.create_format)),
                    Message::FormatSelected,
                )
                .text_size(12)
                .padding([4, 8])
                .style(theme::pick_list_style)
                .menu_style(theme::pick_menu_style)
                .width(78),
                Space::with_width(6),
                text(t(self.lang, "gui.method"))
                    .size(11)
                    .style(theme::text_secondary),
                pick_list(methods, Some(self.create_method), Message::MethodSelected)
                    .text_size(12)
                    .padding([4, 8])
                    .style(theme::pick_list_style)
                    .menu_style(theme::pick_menu_style)
                    .width(92),
                Space::with_width(6),
                text(t(self.lang, "gui.password"))
                    .size(11)
                    .style(theme::text_secondary),
                text_input("", &self.password)
                    .secure(true)
                    .size(12)
                    .padding([4, 8])
                    .style(theme::input_style)
                    .on_input(Message::PasswordChanged)
                    .width(120),
                Space::with_width(Length::Fill),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([5, 10])
        .width(Length::Fill)
        .style(theme::panel);

        let err: Element<'_, Message> = if let Some(e) = &self.last_error {
            container(text(e.clone()).size(12).style(theme::text_danger))
                .padding([6, 10])
                .width(Length::Fill)
                .style(theme::error_banner)
                .into()
        } else {
            Space::with_height(0).into()
        };

        let progress_row: Element<'_, Message> = if busy {
            container(
                column![
                    progress_bar(0.0..=1.0, self.progress_fraction)
                        .height(6)
                        .style(theme::progress_style),
                    text(self.progress_detail.clone())
                        .size(11)
                        .style(theme::text_secondary),
                ]
                .spacing(4),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(theme::panel)
            .into()
        } else {
            Space::with_height(0).into()
        };

        let mut workspace = column![]
            .spacing(8)
            .padding(iced::Padding {
                top: 10.0,
                right: 12.0,
                bottom: 8.0,
                left: 12.0,
            })
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
                container(
                    row![
                        text(format!(
                            "{}: {}",
                            t(self.lang, "gui.find.title"),
                            self.find_filter
                        ))
                        .size(11)
                        .style(theme::text_primary)
                        .width(Length::Fill),
                        button(text("×").size(12))
                            .padding([1, 6])
                            .style(rounded_text_btn)
                            .on_press(Message::FindClear),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([4, 10])
                .width(Length::Fill)
                .style(|th| theme::pill(th, true)),
            );
        }
        if self.last_error.is_some() {
            workspace = workspace.push(err);
        }
        if busy {
            workspace = workspace.push(progress_row);
        }
        workspace = workspace.push(self.view_entry_browser(busy));

        let mut outer = column![workspace].width(Length::Fill).height(Length::Fill);

        if self.prefs.show_status_bar {
            let status_icon = match self.archive_format {
                Some(fmt) => self.icons.for_format(fmt),
                None => self.icons.oxizip(),
            };
            let visible_count = self.visible_indices().len();
            let sel_count = self.selected.len();
            let counts = if sel_count > 0 {
                format!(
                    "{} {}  ·  {} {}",
                    visible_count,
                    t(self.lang, "gui.status.items"),
                    sel_count,
                    t(self.lang, "gui.status.selected")
                )
            } else {
                format!("{} {}", visible_count, t(self.lang, "gui.status.items"))
            };
            let status_bar = container(
                row![
                    image(status_icon).width(15).height(15),
                    text(self.status.clone())
                        .size(11)
                        .style(theme::text_secondary)
                        .width(Length::Fill),
                    text(counts).size(11).style(theme::text_muted),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([6, 12])
            .width(Length::Fill)
            .style(theme::status_bar);
            outer = outer.push(status_bar);
        }

        container(outer)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_toolbar(&self, busy: bool) -> Element<'_, Message> {
        let tk = theme::tokens(&self.ui_theme);
        let small = matches!(self.prefs.toolbar, ToolbarSize::Small);
        let label_size = if small { 11.0 } else { 12.0 };
        let icon_size = if small { 13.0 } else { 14.0 };
        let pad = if small { [4, 8] } else { [6, 10] };

        let make_btn = |icon: icons::UiIcon,
                        key: &'static str,
                        msg: Message,
                        primary: bool,
                        enabled: bool|
         -> Element<'_, Message> {
            let fg = if !enabled {
                tk.text_3
            } else if primary {
                Color::WHITE
            } else {
                tk.text
            };
            let content = row![
                icons::ui_svg(icon, icon_size, fg),
                text(t(self.lang, key)).size(label_size),
            ]
            .spacing(5)
            .align_y(Alignment::Center);
            let mut btn = button(content).padding(pad).style(if primary {
                rounded_primary
            } else {
                rounded_secondary
            });
            if enabled {
                btn = btn.on_press(msg);
            }
            btn.into()
        };

        let tb = &self.prefs.toolbar_buttons;
        type ToolbarGroup<'a> = &'a [(bool, icons::UiIcon, &'static str, Message, bool)];
        let groups: [ToolbarGroup<'_>; 3] = [
            &[
                (
                    tb.open,
                    icons::UiIcon::Open,
                    "gui.open_archive",
                    Message::OpenClicked,
                    false,
                ),
                (
                    tb.close,
                    icons::UiIcon::Close,
                    "gui.close_archive",
                    Message::CloseArchive,
                    false,
                ),
            ],
            &[
                (
                    tb.extract,
                    icons::UiIcon::Extract,
                    "gui.extract",
                    Message::ExtractClicked,
                    true,
                ),
                (
                    tb.extract_here,
                    icons::UiIcon::ExtractHere,
                    "gui.extract_here",
                    Message::ExtractHereClicked,
                    false,
                ),
                (
                    tb.extract_smart,
                    icons::UiIcon::Smart,
                    "gui.extract_smart",
                    Message::ExtractSmartClicked,
                    false,
                ),
                (
                    tb.test,
                    icons::UiIcon::Test,
                    "gui.test",
                    Message::TestClicked,
                    false,
                ),
            ],
            &[
                (
                    tb.add,
                    icons::UiIcon::Add,
                    "gui.add",
                    Message::AddClicked,
                    false,
                ),
                (
                    tb.create,
                    icons::UiIcon::Create,
                    "gui.create",
                    Message::CreateClicked,
                    false,
                ),
            ],
        ];

        let mut tools = row![].spacing(5).align_y(Alignment::Center);
        let mut any_prev = false;
        for group in groups {
            let visible_items: Vec<_> = group.iter().filter(|s| s.0).collect();
            if visible_items.is_empty() {
                continue;
            }
            if any_prev {
                tools = tools.push(
                    container(Space::with_width(1))
                        .height(20)
                        .style(theme::divider),
                );
            }
            for (visible, icon, key, msg, primary) in visible_items {
                if *visible {
                    tools = tools.push(make_btn(*icon, key, msg.clone(), *primary, !busy));
                }
            }
            any_prev = true;
        }
        if busy {
            tools = tools.push(Space::with_width(Length::Fill));
            tools = tools.push(make_btn(
                icons::UiIcon::Close,
                "gui.cancel",
                Message::CancelClicked,
                false,
                true,
            ));
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
        let list_box = container(
            scrollable(list)
                .style(theme::scrollable_style)
                .height(Length::Fill),
        )
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
        let tk = theme::tokens(&self.ui_theme);
        let mut folders = BTreeSet::new();
        for entry in &self.entries {
            if let Some(folder) = top_folder(&entry.name, entry.is_dir) {
                folders.insert(folder);
            }
        }
        let all_active = self.tree_prefix.is_none();
        let mut col = column![button(
            row![
                icons::ui_svg(
                    icons::UiIcon::Folder,
                    14.0,
                    if all_active { tk.accent } else { tk.text_2 }
                ),
                text(t(self.lang, "gui.tree.all")).size(12),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([5, 8])
        .style(move |th, st| {
            let mut s = theme::btn_ghost(th, st);
            if all_active {
                let t = theme::tokens(th);
                s.background = Some(Background::Color(t.accent_soft));
                s.text_color = t.accent;
            }
            s
        })
        .on_press(Message::SetTreePrefix(None))]
        .spacing(2)
        .padding(6);
        for folder in folders {
            let label = folder.clone();
            let active = self.tree_prefix.as_deref() == Some(label.as_str())
                || self.tree_prefix.as_deref() == Some(&format!("{label}/"));
            col = col.push(
                button(
                    row![
                        icons::ui_svg(
                            icons::UiIcon::Folder,
                            14.0,
                            if active { tk.accent } else { tk.text_2 }
                        ),
                        text(label.clone()).size(12),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([5, 8])
                .style(move |th, st| {
                    let mut s = theme::btn_ghost(th, st);
                    if active {
                        let t = theme::tokens(th);
                        s.background = Some(Background::Color(t.accent_soft));
                        s.text_color = t.accent;
                    }
                    s
                })
                .on_press(Message::SetTreePrefix(Some(label))),
            );
        }
        container(
            scrollable(col)
                .style(theme::scrollable_style)
                .height(Length::Fill),
        )
        .width(Length::Fixed(152.0))
        .height(Length::Fill)
        .style(list_box_style)
        .into()
    }

    fn view_details(&self, busy: bool, indices: &[usize]) -> Element<'_, Message> {
        let mut header = row![
            checkbox(
                "",
                indices.iter().all(|i| self.selected.contains(i)) && !indices.is_empty()
            )
            .size(15)
            .style(theme::checkbox_style)
            .on_toggle(|_| Message::ToggleSelectAll),
            text(t(self.lang, "gui.col.name"))
                .size(11)
                .style(theme::text_secondary)
                .width(Length::Fill),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if self.prefs.columns.size {
            header = header.push(
                text(t(self.lang, "gui.col.size"))
                    .size(11)
                    .style(theme::text_secondary)
                    .align_x(Alignment::End)
                    .width(Length::Fixed(72.0)),
            );
        }
        if self.prefs.columns.compressed {
            header = header.push(
                text(t(self.lang, "gui.col.compressed"))
                    .size(11)
                    .style(theme::text_secondary)
                    .align_x(Alignment::End)
                    .width(Length::Fixed(72.0)),
            );
        }
        if self.prefs.columns.method {
            header = header.push(
                text(t(self.lang, "gui.col.method"))
                    .size(11)
                    .style(theme::text_secondary)
                    .width(Length::Fixed(60.0)),
            );
        }
        if self.prefs.columns.modified {
            header = header.push(
                text(t(self.lang, "gui.col.modified"))
                    .size(11)
                    .style(theme::text_secondary)
                    .width(Length::Fixed(116.0)),
            );
        }
        if self.prefs.columns.kind {
            header = header.push(
                text(t(self.lang, "gui.col.kind"))
                    .size(11)
                    .style(theme::text_secondary)
                    .width(Length::Fixed(50.0)),
            );
        }
        header = header.push(Space::with_width(Length::Fixed(76.0)));
        let mut col = column![
            container(header)
                .padding([5, 6])
                .width(Length::Fill)
                .style(list_header_style),
            container(Space::with_height(1))
                .width(Length::Fill)
                .style(theme::divider),
        ]
        .spacing(2);
        if self.prefs.show_dotdot && self.tree_prefix.is_some() {
            let up = button(text("[..]").size(12))
                .padding([3, 6])
                .style(rounded_text_btn)
                .on_press(Message::SetTreePrefix(None));
            col = col.push(container(up).padding([2, 4]).width(Length::Fill));
        }
        for index in indices {
            if let Some(entry) = self.entries.get(*index) {
                col = col.push(self.detail_row(*index, entry, busy));
            }
        }
        col.into()
    }

    fn detail_row(&self, index: usize, entry: &EntryMeta, busy: bool) -> Element<'_, Message> {
        let tk = theme::tokens(&self.ui_theme);
        let selected = self.selected.contains(&index);
        let display_name = entry
            .name
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&entry.name)
            .to_string();
        let entry_icon: Element<'_, Message> = if entry.is_dir {
            icons::ui_svg(icons::UiIcon::Folder, 15.0, tk.accent)
        } else {
            image(self.icons.for_path_name(&entry.name))
                .width(16)
                .height(16)
                .into()
        };
        let name_msg = if entry.is_dir {
            let prefix = if entry.name.ends_with('/') {
                entry.name.clone()
            } else {
                format!("{}/", entry.name)
            };
            Message::SetTreePrefix(Some(prefix))
        } else {
            Message::OpenEntry(index)
        };
        let name_widget: Element<'_, Message> = if self.prefs.single_click_open && !busy {
            button(text(display_name).size(12))
                .padding([2, 4])
                .style(rounded_text_btn)
                .on_press(name_msg)
                .width(Length::Fill)
                .into()
        } else {
            text(display_name)
                .size(12)
                .style(theme::text_primary)
                .width(Length::Fill)
                .into()
        };
        let mut line = row![
            checkbox("", selected)
                .size(15)
                .style(theme::checkbox_style)
                .on_toggle(move |_| Message::ToggleSelect(index)),
            entry_icon,
            name_widget,
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if self.prefs.columns.size {
            let size = entry
                .uncompressed_size
                .map(human_size)
                .unwrap_or_else(|| "—".into());
            line = line.push(
                text(size)
                    .size(12)
                    .style(theme::text_secondary)
                    .align_x(Alignment::End)
                    .width(Length::Fixed(72.0)),
            );
        }
        if self.prefs.columns.compressed {
            let size = entry
                .compressed_size
                .map(human_size)
                .unwrap_or_else(|| "—".into());
            line = line.push(
                text(size)
                    .size(12)
                    .style(theme::text_secondary)
                    .align_x(Alignment::End)
                    .width(Length::Fixed(72.0)),
            );
        }
        if self.prefs.columns.method {
            let method = entry.method.clone().unwrap_or_else(|| "—".into());
            line = line.push(
                text(method)
                    .size(11)
                    .style(theme::text_muted)
                    .width(Length::Fixed(60.0)),
            );
        }
        if self.prefs.columns.modified {
            line = line.push(
                text(fmt_modified(entry.modified))
                    .size(11)
                    .style(theme::text_muted)
                    .width(Length::Fixed(116.0)),
            );
        }
        if self.prefs.columns.kind {
            let kind = if entry.is_dir {
                "DIR".to_string()
            } else {
                std::path::Path::new(&entry.name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_uppercase())
                    .unwrap_or_else(|| "—".into())
            };
            line = line.push(
                text(kind)
                    .size(11)
                    .style(theme::text_muted)
                    .width(Length::Fixed(50.0)),
            );
        }
        let open_btn = {
            let b = button(text(t(self.lang, "gui.open_entry")).size(11))
                .padding([2, 6])
                .style(rounded_text_btn);
            if busy || entry.is_dir {
                b
            } else {
                b.on_press(Message::OpenEntry(index))
            }
        };
        let del_btn = {
            let b = button(text(t(self.lang, "gui.delete_entry")).size(11))
                .padding([2, 6])
                .style(rounded_text_btn);
            if busy {
                b
            } else {
                b.on_press(Message::DeleteEntry(index))
            }
        };
        line = line.push(open_btn).push(del_btn);
        let show_grid = self.prefs.show_gridlines;
        let full_row = self.prefs.full_row_select;
        let row_body: Element<'_, Message> = if full_row {
            mouse_area(line)
                .on_press(Message::ToggleSelect(index))
                .into()
        } else {
            line.into()
        };
        let row_container = container(row_body)
            .padding([3, 6])
            .width(Length::Fill)
            .style(move |theme: &Theme| row_style(theme, selected));

        if show_grid {
            column![
                row_container,
                container(Space::with_height(1))
                    .width(Length::Fill)
                    .style(theme::divider),
            ]
            .into()
        } else {
            row_container.into()
        }
    }

    fn view_name_rows(
        &self,
        busy: bool,
        indices: &[usize],
        icon_px: f32,
        show_icon: bool,
    ) -> Element<'_, Message> {
        let _ = busy;
        let tk = theme::tokens(&self.ui_theme);
        let mut col = column![].spacing(2);
        for index in indices {
            let Some(entry) = self.entries.get(*index) else {
                continue;
            };
            let selected = self.selected.contains(index);
            let mut line = row![checkbox("", selected)
                .size(15)
                .style(theme::checkbox_style)
                .on_toggle({
                    let index = *index;
                    move |_| Message::ToggleSelect(index)
                })]
            .spacing(6)
            .align_y(Alignment::Center);
            if show_icon {
                let icon_el: Element<'_, Message> = if entry.is_dir {
                    icons::ui_svg(icons::UiIcon::Folder, icon_px, tk.accent)
                } else {
                    image(self.icons.for_path_name(&entry.name))
                        .width(icon_px)
                        .height(icon_px)
                        .into()
                };
                line = line.push(icon_el);
            }
            line = line.push(text(entry.name.clone()).size(12));
            col = col.push(
                container(line)
                    .padding([3, 6])
                    .width(Length::Fill)
                    .style(move |theme: &Theme| row_style(theme, selected)),
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
        let tk = theme::tokens(&self.ui_theme);
        let mut col = column![].spacing(8);
        let mut line = row![].spacing(8);
        let mut count = 0usize;
        for index in indices {
            let Some(entry) = self.entries.get(*index) else {
                continue;
            };
            let selected = self.selected.contains(index);
            let icon_el: Element<'_, Message> = if entry.is_dir {
                icons::ui_svg(icons::UiIcon::Folder, icon_px, tk.accent)
            } else {
                image(self.icons.for_path_name(&entry.name))
                    .width(icon_px)
                    .height(icon_px)
                    .into()
            };
            let cell = column![icon_el, text(entry.name.clone()).size(11)]
                .spacing(6)
                .align_x(Alignment::Center)
                .width(Length::Fixed(icon_px + 42.0));
            let index = *index;
            line = line.push(
                button(cell)
                    .padding([8, 6])
                    .style(move |theme, status| {
                        let mut style = rounded_text_btn(theme, status);
                        if selected {
                            style.background =
                                Some(Background::Color(theme::tokens(theme).accent_soft));
                        }
                        style
                    })
                    .on_press(Message::ToggleSelect(index)),
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
        settings::view_settings(SettingsView {
            lang: self.lang,
            tab: self.settings_tab,
            prefs: &self.settings_draft,
            setup_exts: &self.settings_draft_exts,
            status: &self.settings_status,
        })
        .map(Message::Settings)
    }

    fn view_overlay(&self) -> Element<'_, Message> {
        let color_swatch = |hex: u32, label_key: &'static str, accent: ColorAccent| {
            let active = self.prefs.color_accent == accent;
            let swatch = container(Space::new(14, 14)).style(move |_th: &Theme| {
                iced::widget::container::Style {
                    background: Some(Background::Color(Color::from_rgb8(
                        (hex >> 16) as u8,
                        (hex >> 8) as u8,
                        hex as u8,
                    ))),
                    border: Border {
                        radius: 7.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            });
            button(
                row![
                    swatch,
                    text(t(self.lang, label_key)).size(12).width(Length::Fill),
                    if active {
                        text("✓").size(12).style(theme::text_primary)
                    } else {
                        text("").size(12)
                    },
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([8, 12])
            .style(rounded_secondary)
            .on_press(Message::ColorPicked(accent))
        };

        let card = match self.overlay {
            Overlay::None => return Space::with_height(0).into(),
            Overlay::About => self.overlay_card(
                t(self.lang, "gui.about.title"),
                column![
                    row![
                        image(self.icons.oxizip()).width(36).height(36),
                        column![
                            text(format!("Oxizip {}", env!("CARGO_PKG_VERSION")))
                                .size(15)
                                .style(theme::text_primary),
                            text("AGPL-3.0-only").size(11).style(theme::text_muted),
                        ]
                        .spacing(2),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                    text(t(self.lang, "gui.about.body"))
                        .size(12)
                        .style(theme::text_secondary),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.dialog.close")).size(12))
                            .padding([6, 16])
                            .style(rounded_primary)
                            .on_press(Message::DismissOverlay),
                    ],
                ]
                .spacing(12),
            ),
            Overlay::Comment => self.overlay_card(
                t(self.lang, "gui.comment.title"),
                column![
                    text_input("", &self.dialog_text)
                        .size(12)
                        .padding([6, 8])
                        .style(theme::input_style)
                        .on_input(Message::DialogTextChanged)
                        .width(Length::Fill),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.dialog.cancel")).size(12))
                            .padding([6, 14])
                            .style(rounded_secondary)
                            .on_press(Message::DismissOverlay),
                        button(text(t(self.lang, "gui.comment.save")).size(12))
                            .padding([6, 14])
                            .style(rounded_primary)
                            .on_press(Message::CommentSave),
                    ]
                    .spacing(8),
                ]
                .spacing(12),
            ),
            Overlay::Rename => self.overlay_card(
                t(self.lang, "gui.rename.title"),
                column![
                    text(t(self.lang, "gui.rename.name"))
                        .size(12)
                        .style(theme::text_secondary),
                    text_input("", &self.dialog_text)
                        .size(12)
                        .padding([6, 8])
                        .style(theme::input_style)
                        .on_input(Message::DialogTextChanged)
                        .on_submit(Message::RenameConfirm)
                        .width(Length::Fill),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.dialog.cancel")).size(12))
                            .padding([6, 14])
                            .style(rounded_secondary)
                            .on_press(Message::DismissOverlay),
                        button(text(t(self.lang, "gui.dialog.ok")).size(12))
                            .padding([6, 14])
                            .style(rounded_primary)
                            .on_press(Message::RenameConfirm),
                    ]
                    .spacing(8),
                ]
                .spacing(10),
            ),
            Overlay::PasswordManager => self.overlay_card(
                t(self.lang, "gui.passman.title"),
                column![
                    text(t(self.lang, "gui.passman.hint"))
                        .size(11)
                        .style(theme::text_secondary),
                    row![
                        text_input("", &self.dialog_text)
                            .size(12)
                            .padding([6, 8])
                            .style(theme::input_style)
                            .on_input(Message::DialogTextChanged)
                            .width(Length::Fill),
                        button(text(t(self.lang, "gui.passman.add")).size(12))
                            .padding([6, 12])
                            .style(rounded_secondary)
                            .on_press(Message::PassmanAdd),
                    ]
                    .spacing(8),
                    self.view_password_list(),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.dialog.close")).size(12))
                            .padding([6, 16])
                            .style(rounded_primary)
                            .on_press(Message::DismissOverlay),
                    ],
                ]
                .spacing(10),
            ),
            Overlay::Recover => self.overlay_card(
                t(self.lang, "gui.recover.title"),
                column![
                    text(t(self.lang, "gui.recover.hint"))
                        .size(11)
                        .style(theme::text_secondary),
                    text(t(self.lang, "gui.recover.wordlist"))
                        .size(12)
                        .style(theme::text_primary),
                    row![
                        text(if self.recover_wordlist.is_empty() {
                            "—".to_string()
                        } else {
                            self.recover_wordlist.clone()
                        })
                        .size(11)
                        .style(theme::text_secondary)
                        .width(Length::Fill),
                        button(text(t(self.lang, "gui.recover.browse")).size(12))
                            .padding([5, 12])
                            .style(rounded_secondary)
                            .on_press(Message::PickWordlist),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text(t(self.lang, "gui.recover.maxlen"))
                            .size(12)
                            .style(theme::text_primary),
                        text_input("", &self.recover_max_len)
                            .size(12)
                            .padding([5, 8])
                            .style(theme::input_style)
                            .on_input(Message::RecoverMaxChanged)
                            .width(Length::Fixed(52.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(self.dialog_result.as_str())
                        .size(12)
                        .style(theme::text_secondary),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.dialog.close")).size(12))
                            .padding([6, 14])
                            .style(rounded_secondary)
                            .on_press(Message::DismissOverlay),
                        button(text(t(self.lang, "gui.recover.start")).size(12))
                            .padding([6, 14])
                            .style(rounded_primary)
                            .on_press(Message::RecoverStart),
                    ]
                    .spacing(8),
                ]
                .spacing(10),
            ),
            Overlay::Checksum => self.overlay_card(
                t(self.lang, "gui.checksum.title"),
                column![
                    container(
                        scrollable(text(self.dialog_result.as_str()).size(11))
                            .style(theme::scrollable_style)
                            .height(Length::Fixed(180.0))
                    )
                    .style(list_box_style)
                    .padding(8)
                    .width(Length::Fill),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.dialog.close")).size(12))
                            .padding([6, 16])
                            .style(rounded_primary)
                            .on_press(Message::DismissOverlay),
                    ],
                ]
                .spacing(10),
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
                    Space::with_height(4),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.dialog.close")).size(12))
                            .padding([6, 16])
                            .style(rounded_primary)
                            .on_press(Message::DismissOverlay),
                    ],
                ]
                .spacing(6),
            ),
            Overlay::Color => self.overlay_card(
                t(self.lang, "gui.color.title"),
                column![
                    color_swatch(0x2F6FD6, "gui.color.brand", ColorAccent::BrandBlue),
                    color_swatch(0x1F4E8C, "gui.color.navy", ColorAccent::DeepNavy),
                    color_swatch(0x1783A3, "gui.color.teal", ColorAccent::Teal),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.dialog.cancel")).size(12))
                            .padding([6, 12])
                            .style(rounded_text_btn)
                            .on_press(Message::DismissOverlay),
                    ],
                ]
                .spacing(8),
            ),
            Overlay::Find => self.overlay_card(
                t(self.lang, "gui.find.title"),
                column![
                    text(t(self.lang, "gui.find.hint"))
                        .size(12)
                        .style(theme::text_secondary),
                    text_input("", &self.dialog_text)
                        .size(12)
                        .padding([6, 8])
                        .style(theme::input_style)
                        .on_input(Message::DialogTextChanged)
                        .on_submit(Message::FindApply)
                        .width(Length::Fill),
                    row![
                        Space::with_width(Length::Fill),
                        button(text(t(self.lang, "gui.find.clear")).size(12))
                            .padding([6, 14])
                            .style(rounded_secondary)
                            .on_press(Message::FindClear),
                        button(text(t(self.lang, "gui.find.apply")).size(12))
                            .padding([6, 14])
                            .style(rounded_primary)
                            .on_press(Message::FindApply),
                    ]
                    .spacing(8),
                ]
                .spacing(10),
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
                        text(hint).size(12).style(theme::text_secondary),
                        row![
                            Space::with_width(Length::Fill),
                            button(text(t(self.lang, "gui.dialog.cancel")).size(12))
                                .padding([6, 14])
                                .style(rounded_secondary)
                                .on_press(Message::DismissOverlay),
                            button(text(t(self.lang, "gui.dialog.ok")).size(12))
                                .padding([6, 14])
                                .style(theme::btn_danger)
                                .on_press(Message::ConfirmDeleteArchive),
                        ]
                        .spacing(8),
                    ]
                    .spacing(12),
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
            column![
                text(title).size(16).style(theme::text_primary),
                container(Space::with_height(1))
                    .width(Length::Fill)
                    .style(theme::divider),
                body.into(),
            ]
            .spacing(10)
            .padding(18)
            .width(Length::Fixed(380.0)),
        )
        .style(dropdown_panel_style)
        .into()
    }

    fn view_password_list(&self) -> Element<'_, Message> {
        if self.prefs.passwords.is_empty() {
            return text(t(self.lang, "gui.passman.empty"))
                .size(12)
                .style(theme::text_muted)
                .into();
        }
        let mut col = column![].spacing(4);
        for (index, password) in self.prefs.passwords.iter().enumerate() {
            col = col.push(
                row![
                    text(password.clone()).size(12).width(Length::Fill),
                    button(text(t(self.lang, "gui.passman.use")).size(11))
                        .padding([2, 8])
                        .style(rounded_text_btn)
                        .on_press(Message::PassmanUse(index)),
                    button(text(t(self.lang, "gui.passman.remove")).size(11))
                        .padding([2, 8])
                        .style(rounded_text_btn)
                        .on_press(Message::PassmanRemove(index)),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        }
        container(
            scrollable(col)
                .style(theme::scrollable_style)
                .height(Length::Fixed(120.0)),
        )
        .style(list_box_style)
        .padding(8)
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
            .size(15)
            .text_size(12)
            .spacing(8)
            .style(theme::checkbox_style)
            .on_toggle(move |value| Message::ToolbarFlagToggled(flag, value))
            .into()
    }
}

fn rounded_primary(theme: &Theme, status: button::Status) -> button::Style {
    theme::btn_primary(theme, status)
}

fn rounded_secondary(theme: &Theme, status: button::Status) -> button::Style {
    theme::btn_secondary(theme, status)
}

fn rounded_text_btn(theme: &Theme, status: button::Status) -> button::Style {
    theme::btn_ghost(theme, status)
}

fn list_header_style(theme: &Theme) -> iced::widget::container::Style {
    theme::list_header(theme)
}

fn list_box_style(theme: &Theme) -> iced::widget::container::Style {
    theme::panel(theme)
}

pub(crate) fn home_menu_bar_style(theme: &Theme) -> iced::widget::container::Style {
    theme::menu_bar(theme)
}

pub(crate) fn home_menu_btn_style(theme: &Theme, status: button::Status) -> button::Style {
    theme::btn_menu(theme, status)
}

pub(crate) fn dropdown_panel_style(theme: &Theme) -> iced::widget::container::Style {
    theme::elevated(theme)
}

pub(crate) fn dropdown_item_style(theme: &Theme, status: button::Status) -> button::Style {
    theme::btn_menu(theme, status)
}

fn home_glass_tile<'a>(
    icon: iced::widget::image::Handle,
    label: &'a str,
    desc: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let body = column![
        container(image(icon).width(34).height(34))
            .padding(14)
            .style(theme::icon_badge),
        Space::with_height(14),
        text(label).size(14).style(theme::text_primary),
        Space::with_height(4),
        text(desc).size(11).style(theme::text_secondary),
    ]
    .spacing(0)
    .align_x(Alignment::Center);

    let mut btn = button(
        container(body)
            .width(Length::Fill)
            .padding([24, 18])
            .center_x(Length::Fill),
    )
    .width(236)
    .style(theme::home_tile);

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

async fn pick_create_output(format: FormatId, initial_dir: Option<PathBuf>) -> Option<PathBuf> {
    let ext = format.as_str();
    let default_name = format!("archive.{ext}");
    let mut dlg = rfd::AsyncFileDialog::new()
        .set_file_name(&default_name)
        .add_filter(ext, &[ext]);
    if let Some(dir) = initial_dir {
        dlg = dlg.set_directory(dir);
    }
    dlg.save_file().await.map(|f| f.path().to_path_buf())
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

fn row_style(theme: &Theme, selected: bool) -> iced::widget::container::Style {
    if selected {
        theme::row_selected(theme)
    } else {
        Default::default()
    }
}

fn dim_style(theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: theme::scrim(theme).background,
        text_color: Color::WHITE,
        border: Border::default(),
        shadow: iced::Shadow::default(),
    }
}
