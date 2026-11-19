//! Oxizip custom installer / uninstaller (clean-room UI).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod install;

use iced::widget::{
    button, column, container, mouse_area, pick_list, progress_bar, row, scrollable, text,
    text_input, Space,
};
use iced::{window, Alignment, Element, Length, Point, Size, Subscription, Task};
use install::{
    append_window_pos, default_install_dir, encode_install_args, encode_uninstall_args,
    is_elevated, launch_app, parse_cli, perform_install, perform_uninstall, relaunch_elevated,
    resolve_install_dir, CliMode, InstallOptions, APP_NAME, VERSION,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

enum JobEvent {
    Progress { fraction: f32, detail: String },
    Finished(Result<(), String>),
}

fn main() -> iced::Result {
    match parse_cli() {
        CliMode::ElevatedInstall(opts) => {
            match perform_install(&opts, &mut |_, _| {}) {
                Ok(()) => {
                    let _ = launch_app(&opts.dir);
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("install failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        CliMode::ElevatedUninstall { dir } => {
            match perform_uninstall(dir, &mut |_, _| {}) {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("uninstall failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        CliMode::UninstallUi {
            dir,
            auto_start,
            window_pos,
        } => run_uninstall_ui(dir, auto_start, window_pos),
        CliMode::InstallerUi {
            auto_install,
            window_pos,
        } => run_installer_ui(auto_install, window_pos),
    }
}

fn run_installer_ui(
    auto_install: Option<InstallOptions>,
    window_pos: Option<(f32, f32)>,
) -> iced::Result {
    iced::application(SetupApp::title, SetupApp::update, SetupApp::view)
        .subscription(SetupApp::subscription)
        .window(undecorated_window(Size::new(600.0, 440.0), window_pos))
        .default_font(ui_font())
        .run_with(move || SetupApp::new(auto_install))
}

fn run_uninstall_ui(
    dir: Option<PathBuf>,
    auto_start: bool,
    window_pos: Option<(f32, f32)>,
) -> iced::Result {
    iced::application(
        UninstallApp::title,
        UninstallApp::update,
        UninstallApp::view,
    )
    .subscription(UninstallApp::subscription)
    // ≈ golden ratio: 486 / 300 = 1.62
    .window(undecorated_window(Size::new(486.0, 300.0), window_pos))
    .default_font(ui_font())
    .run_with(move || UninstallApp::new(dir, auto_start))
}

fn progress_label(lang: Lang, key: &str) -> String {
    match (lang, key) {
        (Lang::Zh, "prepare") => "准备中…".into(),
        (Lang::En, "prepare") => "Preparing…".into(),
        (Lang::Zh, "extract") => "正在解压文件…".into(),
        (Lang::En, "extract") => "Extracting files…".into(),
        (Lang::Zh, "copy") => "正在复制文件…".into(),
        (Lang::En, "copy") => "Copying files…".into(),
        (Lang::Zh, "uninstall_stub") => "正在写入卸载程序…".into(),
        (Lang::En, "uninstall_stub") => "Writing uninstaller…".into(),
        (Lang::Zh, "registry") => "正在写入注册表…".into(),
        (Lang::En, "registry") => "Updating registry…".into(),
        (Lang::Zh, "shortcut") => "正在创建快捷方式…".into(),
        (Lang::En, "shortcut") => "Creating shortcuts…".into(),
        (Lang::Zh, "associate") => "正在配置文件关联…".into(),
        (Lang::En, "associate") => "Configuring associations…".into(),
        (Lang::Zh, "shell") => "正在移除右键菜单…".into(),
        (Lang::En, "shell") => "Removing shell integration…".into(),
        (Lang::Zh, "stop") => "正在结束进程…".into(),
        (Lang::En, "stop") => "Stopping processes…".into(),
        (Lang::Zh, "start_menu") => "正在清理开始菜单…".into(),
        (Lang::En, "start_menu") => "Cleaning Start Menu…".into(),
        (Lang::Zh, "schedule_delete" | "delete") => "正在删除文件…".into(),
        (Lang::En, "schedule_delete" | "delete") => "Deleting files…".into(),
        (Lang::Zh, "done") => "即将完成…".into(),
        (Lang::En, "done") => "Finishing…".into(),
        (Lang::Zh, _) => "处理中…".into(),
        (Lang::En, _) => "Working…".into(),
    }
}

type JobDrain = (Option<(f32, String)>, Option<Result<(), String>>);

fn drain_job(rx: &Receiver<JobEvent>) -> JobDrain {
    let mut progress = None;
    let mut finished = None;
    loop {
        match rx.try_recv() {
            Ok(JobEvent::Progress { fraction, detail }) => {
                progress = Some((fraction, detail));
            }
            Ok(JobEvent::Finished(r)) => {
                finished = Some(r);
                break;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                if finished.is_none() {
                    finished = Some(Err("worker disconnected".into()));
                }
                break;
            }
        }
    }
    (progress, finished)
}

const WINDOW_RADIUS: f32 = 12.0;
const CONTROL_RADIUS: f32 = 8.0;

fn center_on_monitor(size: Size, monitor: Size) -> Point {
    Point::new(
        ((monitor.width - size.width) / 2.0).max(0.0),
        ((monitor.height - size.height) / 2.0).max(0.0),
    )
}

fn undecorated_window(size: Size, window_pos: Option<(f32, f32)>) -> iced::window::Settings {
    let mut settings = iced::window::Settings {
        size,
        resizable: false,
        decorations: false,
        transparent: true,
        // Prefer explicit monitor centering; `Centered` is unreliable with undecorated windows.
        position: match window_pos {
            Some((x, y)) => window::Position::Specific(Point::new(x, y)),
            None => window::Position::SpecificWith(center_on_monitor),
        },
        ..Default::default()
    };
    settings.icon = window_icon();
    settings
}

#[cfg(windows)]
fn apply_native_window_rounding(raw_id: u64) {
    // Win11 DWM rounded corners for the HWND backing this iced window.
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
    unsafe {
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            raw_id as *mut core::ffi::c_void,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&pref as *const i32).cast(),
            std::mem::size_of_val(&pref) as u32,
        );
    }
}

#[cfg(not(windows))]
fn apply_native_window_rounding(_raw_id: u64) {}

fn rounded_primary(theme: &iced::Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let mut style = iced::widget::button::primary(theme, status);
    style.border.radius = CONTROL_RADIUS.into();
    style
}

fn rounded_danger(theme: &iced::Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let mut style = iced::widget::button::danger(theme, status);
    style.border.radius = CONTROL_RADIUS.into();
    style
}

fn rounded_secondary(theme: &iced::Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let mut style = iced::widget::button::secondary(theme, status);
    style.border.radius = CONTROL_RADIUS.into();
    style
}

fn rounded_text_btn(theme: &iced::Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let mut style = iced::widget::button::text(theme, status);
    style.border.radius = 6.0.into();
    style
}

/// Custom caption bar: drag region + language dropdown immediately left of min/close.
fn caption_bar<'a, Message: Clone + 'a>(
    title: String,
    lang: Lang,
    on_lang: impl Fn(Lang) -> Message + 'a,
    on_drag: Message,
    on_minimize: Message,
    on_close: Message,
) -> Element<'a, Message> {
    let drag_zone = mouse_area(
        row![
            text(title).size(12),
            Space::with_width(Length::Fill),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left: 10.0,
        }),
    )
    .on_press(on_drag);

    let caption_btn = |label: &'static str, msg: Message| {
        button(container(text(label).size(12)).center_x(Length::Fill).center_y(Length::Fill))
            .width(40)
            .height(28)
            .padding(0)
            .style(rounded_text_btn)
            .on_press(msg)
    };

    container(
        row![
            drag_zone,
            pick_list(Lang::ALL, Some(lang), on_lang)
                .text_size(12)
                .width(78)
                .padding(4),
            Space::with_width(4),
            caption_btn("—", on_minimize),
            caption_btn("×", on_close),
        ]
        .spacing(2)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(32),
    )
    .width(Length::Fill)
    .style(|theme: &iced::Theme| {
        let palette = theme.extended_palette();
        iced::widget::container::Style {
            background: Some(iced::Background::Color(palette.background.weak.color)),
            border: iced::Border {
                color: palette.background.strong.color,
                width: 0.0,
                radius: iced::border::Radius {
                    top_left: WINDOW_RADIUS,
                    top_right: WINDOW_RADIUS,
                    bottom_right: 0.0,
                    bottom_left: 0.0,
                },
            },
            ..Default::default()
        }
    })
    .into()
}

fn frame_border(theme: &iced::Theme) -> iced::widget::container::Style {
    let palette = theme.extended_palette();
    iced::widget::container::Style {
        background: Some(iced::Background::Color(palette.background.base.color)),
        border: iced::Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: WINDOW_RADIUS.into(),
        },
        ..Default::default()
    }
}


fn ui_font() -> iced::Font {
    #[cfg(windows)]
    {
        iced::Font::with_name("Microsoft YaHei UI")
    }
    #[cfg(not(windows))]
    {
        iced::Font::DEFAULT
    }
}

fn window_icon() -> Option<iced::window::Icon> {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../oxizip-gui/assets/icons/oxizip.ico"
    ));
    iced::window::icon::from_file_data(bytes, Some(image::ImageFormat::Ico)).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Zh,
    En,
}

impl Lang {
    const ALL: &'static [Lang] = &[Lang::Zh, Lang::En];

    fn label(self) -> &'static str {
        match self {
            Self::Zh => "简体中文",
            Self::En => "English",
        }
    }
    fn code(self) -> &'static str {
        match self {
            Self::Zh => "zh-cn",
            Self::En => "en",
        }
    }
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

struct SetupApp {
    lang: Lang,
    dir: String,
    status: String,
    progress: f32,
    busy: bool,
    done: bool,
    window_id: Option<window::Id>,
    job_rx: Option<Receiver<JobEvent>>,
    pending_elevate: Option<InstallOptions>,
}

#[derive(Debug, Clone)]
enum Message {
    WindowId(Option<window::Id>),
    RawWindowId(u64),
    TitleDrag,
    Minimize,
    Close,
    LangSelected(Lang),
    DirChanged(String),
    Browse,
    BrowseDone(Option<PathBuf>),
    Install,
    ElevateWithPosition(Option<Point>),
    PollJob,
    InstallFinished(Result<(), String>),
}

impl SetupApp {
    fn new(auto_install: Option<InstallOptions>) -> (Self, Task<Message>) {
        let mut lang = Lang::Zh;
        let mut dir = default_install_dir().display().to_string();
        if let Some(ref opts) = auto_install {
            dir = opts.dir.display().to_string();
            lang = if opts.lang.to_ascii_lowercase().starts_with("en") {
                Lang::En
            } else {
                Lang::Zh
            };
        }
        let app = Self {
            lang,
            dir,
            status: String::new(),
            progress: 0.0,
            busy: false,
            done: false,
            window_id: None,
            job_rx: None,
            pending_elevate: None,
        };
        let boot = window::get_oldest().map(Message::WindowId);
        if auto_install.is_some() {
            (app, Task::batch([boot, Task::done(Message::Install)]))
        } else {
            (app, boot)
        }
    }

    fn title(&self) -> String {
        match self.lang {
            Lang::Zh => format!("安装 {APP_NAME} {VERSION}"),
            Lang::En => format!("Install {APP_NAME} {VERSION}"),
        }
    }

    fn tr(&self, zh: &str, en: &str) -> String {
        match self.lang {
            Lang::Zh => zh.to_string(),
            Lang::En => en.to_string(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.job_rx.is_some() {
            iced::time::every(Duration::from_millis(50)).map(|_| Message::PollJob)
        } else {
            Subscription::none()
        }
    }

    fn spawn_install(&mut self, opts: InstallOptions) {
        let (tx, rx) = mpsc::channel();
        self.job_rx = Some(rx);
        self.busy = true;
        self.progress = 0.0;
        self.status = progress_label(self.lang, "prepare");
        std::thread::spawn(move || {
            let tx_progress = tx.clone();
            let result = perform_install(&opts, &mut |fraction, detail| {
                let _ = tx_progress.send(JobEvent::Progress {
                    fraction,
                    detail: detail.to_string(),
                });
            })
            .map(|_| {
                let _ = launch_app(&opts.dir);
            })
            .map_err(|e| e.to_string());
            let _ = tx.send(JobEvent::Finished(result));
        });
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowId(id) => {
                self.window_id = id;
                match id {
                    Some(id) => window::get_raw_id::<Message>(id).map(Message::RawWindowId),
                    None => Task::none(),
                }
            }
            Message::RawWindowId(raw) => {
                apply_native_window_rounding(raw);
                Task::none()
            }
            Message::TitleDrag => self
                .window_id
                .map(window::drag)
                .unwrap_or_else(Task::none),
            Message::Minimize => self
                .window_id
                .map(|id| window::minimize(id, true))
                .unwrap_or_else(Task::none),
            Message::Close => {
                if self.busy {
                    return Task::none();
                }
                self.window_id
                    .map(window::close)
                    .unwrap_or_else(Task::none)
            }
            Message::LangSelected(lang) => {
                self.lang = lang;
                Task::none()
            }
            Message::DirChanged(v) => {
                if !self.busy {
                    self.dir = v;
                }
                Task::none()
            }
            Message::Browse => {
                if self.busy {
                    return Task::none();
                }
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_directory(default_install_dir())
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::BrowseDone,
                )
            }
            Message::BrowseDone(Some(p)) => {
                self.dir = p.display().to_string();
                Task::none()
            }
            Message::BrowseDone(None) => Task::none(),
            Message::Install => {
                if self.busy || self.done {
                    return Task::none();
                }
                // File associations are configured on first GUI launch, not here.
                let opts = InstallOptions {
                    dir: PathBuf::from(self.dir.trim()),
                    associate: false,
                    lang: self.lang.code().to_string(),
                };
                if opts.dir.as_os_str().is_empty() {
                    self.status = self.tr("请选择安装目录", "Choose an install folder");
                    return Task::none();
                }

                #[cfg(windows)]
                {
                    if !is_elevated() {
                        self.pending_elevate = Some(opts);
                        return if let Some(id) = self.window_id {
                            window::get_position(id).map(Message::ElevateWithPosition)
                        } else {
                            Task::done(Message::ElevateWithPosition(None))
                        };
                    }
                }

                self.spawn_install(opts);
                Task::none()
            }
            Message::ElevateWithPosition(pos) => {
                let Some(opts) = self.pending_elevate.take() else {
                    return Task::none();
                };
                let mut args = encode_install_args(&opts);
                append_window_pos(&mut args, pos.map(|p| (p.x, p.y)));
                match relaunch_elevated(&args) {
                    Ok(()) => std::process::exit(0),
                    Err(e) => {
                        self.status = format!("{e}");
                        Task::none()
                    }
                }
            }
            Message::PollJob => {
                let Some(rx) = self.job_rx.as_ref() else {
                    return Task::none();
                };
                let (progress, finished) = drain_job(rx);
                if let Some((fraction, detail)) = progress {
                    self.progress = fraction.clamp(0.0, 1.0);
                    self.status = progress_label(self.lang, &detail);
                }
                if let Some(result) = finished {
                    self.job_rx = None;
                    return Task::done(Message::InstallFinished(result));
                }
                Task::none()
            }
            Message::InstallFinished(Ok(())) => {
                // GUI already launched with --setup; exit installer reliably.
                std::process::exit(0);
            }
            Message::InstallFinished(Err(e)) => {
                self.busy = false;
                self.status = e;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        const CONTENT_WIDTH: f32 = 520.0;

        let window_title = self.title();
        let caption = caption_bar(
            window_title.clone(),
            self.lang,
            Message::LangSelected,
            Message::TitleDrag,
            Message::Minimize,
            Message::Close,
        );

        let heading = text(window_title)
            .size(16)
            .width(Length::Fixed(CONTENT_WIDTH));

        let license_body = text(self.tr(LICENSE_TEXT_ZH, LICENSE_TEXT_EN)).size(11);
        let license_box = container(
            scrollable(license_body)
                .height(Length::Fill)
                .width(Length::Fill),
        )
        .padding(8)
        .width(Length::Fixed(CONTENT_WIDTH))
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

        let path_label = text(self.tr("目标文件夹", "Destination folder")).size(12);
        let mut path_input = text_input("", &self.dir).size(12).width(Length::Fill);
        if !self.busy && !self.done {
            path_input = path_input.on_input(Message::DirChanged);
        }
        let mut browse = button(text("...").size(12)).style(rounded_secondary);
        if !self.busy && !self.done {
            browse = browse.on_press(Message::Browse);
        }
        let path_row = row![path_input, Space::with_width(8), browse].align_y(Alignment::Center);

        let path_block = column![path_label, path_row]
            .spacing(4)
            .width(Length::Fixed(CONTENT_WIDTH));

        // Fixed-height slot so swapping button ↔ progress does not shift layout.
        const ACTION_H: f32 = 44.0;
        let action_inner: Element<'_, Message> = if self.busy || self.done {
            let status = text(if self.status.is_empty() {
                " "
            } else {
                self.status.as_str()
            })
            .size(11)
            .width(Length::Fill);
            column![
                progress_bar(0.0..=1.0, self.progress)
                    .width(Length::Fill)
                    .height(8),
                status,
            ]
            .spacing(4)
            .width(Length::Fill)
            .into()
        } else {
            let label = container(text(self.tr("同意并安装", "Agree and Install")).size(14))
                .width(Length::Fill)
                .center_x(Length::Fill);
            button(label)
                .width(Length::Fill)
                .padding(10)
                .style(rounded_primary)
                .on_press(Message::Install)
                .into()
        };
        let action_area = container(action_inner)
            .width(Length::Fixed(CONTENT_WIDTH))
            .height(ACTION_H)
            .center_y(ACTION_H);

        let content = column![
            heading,
            Space::with_height(8),
            license_box,
            Space::with_height(8),
            path_block,
            Space::with_height(8),
            action_area,
        ]
        .spacing(4)
        .padding(iced::Padding {
            top: 10.0,
            right: 16.0,
            bottom: 12.0,
            left: 16.0,
        })
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fill);

        container(column![caption, content].width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(frame_border)
            .into()
    }
}

const LICENSE_TEXT_ZH: &str = "\
Oxizip 最终用户许可协议

Oxizip 是自由软件，采用 GNU Affero General Public License v3.0 only（AGPL-3.0-only）授权。

在遵守 AGPL-3.0 条款的前提下，你可以再分发和/或修改本软件。
完整许可文本见本软件附带的 LICENSE 文件，也可访问：
https://www.gnu.org/licenses/agpl-3.0.html

点击「同意并安装」即表示你已阅读并同意上述许可协议。
";

const LICENSE_TEXT_EN: &str = "\
Oxizip End User License Agreement

Oxizip is free software licensed under the GNU Affero General Public License
v3.0 only (AGPL-3.0-only).

You may redistribute and/or modify it under the terms of the AGPL-3.0.
See the LICENSE file shipped with this software, or:
https://www.gnu.org/licenses/agpl-3.0.html

By clicking \"Agree and Install\" you acknowledge that you have read and
agree to this license.
";

struct UninstallApp {
    lang: Lang,
    dir: PathBuf,
    status: String,
    progress: f32,
    busy: bool,
    done: bool,
    window_id: Option<window::Id>,
    job_rx: Option<Receiver<JobEvent>>,
    pending_elevate: bool,
}

#[derive(Debug, Clone)]
enum UMessage {
    WindowId(Option<window::Id>),
    RawWindowId(u64),
    TitleDrag,
    Minimize,
    Close,
    LangSelected(Lang),
    Confirm,
    Cancel,
    ElevateWithPosition(Option<Point>),
    PollJob,
    Finished(Result<(), String>),
}

impl UninstallApp {
    fn new(dir: Option<PathBuf>, auto_start: bool) -> (Self, Task<UMessage>) {
        let dir = resolve_install_dir(dir);
        let app = Self {
            lang: Lang::Zh,
            dir,
            status: String::new(),
            progress: 0.0,
            busy: false,
            done: false,
            window_id: None,
            job_rx: None,
            pending_elevate: false,
        };
        let boot = window::get_oldest().map(UMessage::WindowId);
        if auto_start {
            (app, Task::batch([boot, Task::done(UMessage::Confirm)]))
        } else {
            (app, boot)
        }
    }

    fn title(&self) -> String {
        match self.lang {
            Lang::Zh => format!("卸载 {APP_NAME}"),
            Lang::En => format!("Uninstall {APP_NAME}"),
        }
    }

    fn tr(&self, zh: &str, en: &str) -> String {
        match self.lang {
            Lang::Zh => zh.to_string(),
            Lang::En => en.to_string(),
        }
    }

    fn prompt(&self) -> String {
        self.tr(
            "确定要从本机移除 Oxizip 吗？将删除程序文件、开始菜单项与文件关联。",
            "Remove Oxizip from this computer? Program files, Start Menu entries, and file associations will be deleted.",
        )
    }

    fn subscription(&self) -> Subscription<UMessage> {
        if self.job_rx.is_some() {
            iced::time::every(Duration::from_millis(50)).map(|_| UMessage::PollJob)
        } else {
            Subscription::none()
        }
    }

    fn spawn_uninstall(&mut self) {
        let dir = Some(self.dir.clone());
        let (tx, rx) = mpsc::channel();
        self.job_rx = Some(rx);
        self.busy = true;
        self.progress = 0.0;
        self.status = progress_label(self.lang, "prepare");
        std::thread::spawn(move || {
            let tx_progress = tx.clone();
            let result = perform_uninstall(dir, &mut |fraction, detail| {
                let _ = tx_progress.send(JobEvent::Progress {
                    fraction,
                    detail: detail.to_string(),
                });
            })
            .map_err(|e| e.to_string());
            let _ = tx.send(JobEvent::Finished(result));
        });
    }

    fn update(&mut self, message: UMessage) -> Task<UMessage> {
        match message {
            UMessage::WindowId(id) => {
                self.window_id = id;
                match id {
                    Some(id) => window::get_raw_id::<UMessage>(id).map(UMessage::RawWindowId),
                    None => Task::none(),
                }
            }
            UMessage::RawWindowId(raw) => {
                apply_native_window_rounding(raw);
                Task::none()
            }
            UMessage::TitleDrag => self
                .window_id
                .map(window::drag)
                .unwrap_or_else(Task::none),
            UMessage::Minimize => self
                .window_id
                .map(|id| window::minimize(id, true))
                .unwrap_or_else(Task::none),
            UMessage::Close => {
                if self.busy {
                    return Task::none();
                }
                self.window_id
                    .map(window::close)
                    .unwrap_or_else(Task::none)
            }
            UMessage::LangSelected(lang) => {
                self.lang = lang;
                Task::none()
            }
            UMessage::Cancel => {
                if !self.busy {
                    if let Some(id) = self.window_id {
                        return window::close(id);
                    }
                    std::process::exit(0);
                }
                Task::none()
            }
            UMessage::Confirm => {
                if self.busy || self.done {
                    return Task::none();
                }
                #[cfg(windows)]
                {
                    if !is_elevated() {
                        self.pending_elevate = true;
                        return if let Some(id) = self.window_id {
                            window::get_position(id).map(UMessage::ElevateWithPosition)
                        } else {
                            Task::done(UMessage::ElevateWithPosition(None))
                        };
                    }
                }
                self.spawn_uninstall();
                Task::none()
            }
            UMessage::ElevateWithPosition(pos) => {
                if !self.pending_elevate {
                    return Task::none();
                }
                self.pending_elevate = false;
                let mut args = encode_uninstall_args(Some(&self.dir));
                append_window_pos(&mut args, pos.map(|p| (p.x, p.y)));
                match relaunch_elevated(&args) {
                    Ok(()) => std::process::exit(0),
                    Err(e) => {
                        self.status = format!("{e}");
                        Task::none()
                    }
                }
            }
            UMessage::PollJob => {
                let Some(rx) = self.job_rx.as_ref() else {
                    return Task::none();
                };
                let (progress, finished) = drain_job(rx);
                if let Some((fraction, detail)) = progress {
                    self.progress = fraction.clamp(0.0, 1.0);
                    self.status = progress_label(self.lang, &detail);
                }
                if let Some(result) = finished {
                    self.job_rx = None;
                    return Task::done(UMessage::Finished(result));
                }
                Task::none()
            }
            UMessage::Finished(Ok(())) => {
                self.busy = false;
                self.done = true;
                self.progress = 1.0;
                self.status = self.tr("Oxizip 已卸载。", "Oxizip has been removed.");
                Task::none()
            }
            UMessage::Finished(Err(e)) => {
                self.busy = false;
                self.status = e;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, UMessage> {
        let window_title = self.title();
        let caption = caption_bar(
            window_title.clone(),
            self.lang,
            UMessage::LangSelected,
            UMessage::TitleDrag,
            UMessage::Minimize,
            UMessage::Close,
        );

        let path_line = text(format!(
            "{} {}",
            self.tr("安装目录：", "Install folder:"),
            self.dir.display()
        ))
        .size(12);

        let status = if self.status.is_empty() {
            text(self.prompt()).size(12)
        } else {
            text(&self.status).size(12)
        };

        let action_area: Element<'_, UMessage> = if self.busy {
            progress_bar(0.0..=1.0, self.progress)
                .width(Length::Fill)
                .height(8)
                .into()
        } else if self.done {
            column![
                progress_bar(0.0..=1.0, self.progress)
                    .width(Length::Fill)
                    .height(8),
                Space::with_height(8),
                button(
                    container(text(self.tr("完成", "Done")).size(14))
                        .width(Length::Fill)
                        .center_x(Length::Fill)
                )
                .width(Length::Fill)
                .padding(10)
                .on_press(UMessage::Cancel)
                .style(rounded_primary),
            ]
            .into()
        } else {
            let uninstall = button(
                container(text(self.tr("卸载", "Uninstall")).size(14))
                    .width(Length::Fill)
                    .center_x(Length::Fill),
            )
            .width(Length::Fill)
            .padding(10)
            .style(rounded_danger)
            .on_press(UMessage::Confirm);
            let cancel = button(
                container(text(self.tr("取消", "Cancel")).size(14))
                    .width(Length::Fill)
                    .center_x(Length::Fill),
            )
            .width(Length::Fill)
            .padding(10)
            .style(rounded_secondary)
            .on_press(UMessage::Cancel);
            row![uninstall, Space::with_width(12), cancel].into()
        };

        let content = column![
            text(window_title).size(16),
            Space::with_height(6),
            path_line,
            Space::with_height(6),
            status,
            Space::with_height(Length::Fill),
            action_area,
        ]
        .spacing(6)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center);

        container(column![caption, content].width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(frame_border)
            .into()
    }
}
