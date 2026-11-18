//! Oxizip custom installer / uninstaller (clean-room UI).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod install;

use iced::widget::{button, checkbox, column, container, row, text, text_input, Space};
use iced::{Element, Length, Size, Task};
use install::{
    default_install_dir, encode_install_args, encode_uninstall_args, is_elevated, launch_app,
    parse_cli, perform_install, perform_uninstall, relaunch_elevated, resolve_install_dir,
    CliMode, InstallOptions, APP_NAME, VERSION,
};
use std::path::PathBuf;

fn main() -> iced::Result {
    match parse_cli() {
        CliMode::ElevatedInstall(opts) => match perform_install(&opts) {
            Ok(()) => {
                let _ = launch_app(&opts.dir);
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("install failed: {e}");
                std::process::exit(1);
            }
        },
        CliMode::ElevatedUninstall { dir } => match perform_uninstall(dir) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("uninstall failed: {e}");
                std::process::exit(1);
            }
        },
        CliMode::UninstallUi { dir } => return run_uninstall_ui(dir),
        CliMode::InstallerUi => {}
    }

    let mut window = iced::window::Settings {
        size: Size::new(560.0, 420.0),
        resizable: false,
        ..Default::default()
    };
    window.icon = window_icon();

    iced::application(SetupApp::title, SetupApp::update, SetupApp::view)
        .window(window)
        .default_font(ui_font())
        .run_with(SetupApp::new)
}

fn run_uninstall_ui(dir: Option<PathBuf>) -> iced::Result {
    let mut window = iced::window::Settings {
        size: Size::new(520.0, 300.0),
        resizable: false,
        ..Default::default()
    };
    window.icon = window_icon();
    iced::application(
        UninstallApp::title,
        UninstallApp::update,
        UninstallApp::view,
    )
    .window(window)
    .default_font(ui_font())
    .run_with(move || UninstallApp::new(dir))
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

struct SetupApp {
    lang: Lang,
    dir: String,
    associate: bool,
    status: String,
    busy: bool,
    done: bool,
}

#[derive(Debug, Clone)]
enum Message {
    LangZh,
    LangEn,
    DirChanged(String),
    Browse,
    BrowseDone(Option<PathBuf>),
    AssociateToggled(bool),
    Install,
    InstallFinished(Result<(), String>),
}

impl SetupApp {
    fn new() -> (Self, Task<Message>) {
        (
            Self {
                lang: Lang::Zh,
                dir: default_install_dir().display().to_string(),
                associate: true,
                status: String::new(),
                busy: false,
                done: false,
            },
            Task::none(),
        )
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

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LangZh => {
                self.lang = Lang::Zh;
                Task::none()
            }
            Message::LangEn => {
                self.lang = Lang::En;
                Task::none()
            }
            Message::DirChanged(v) => {
                self.dir = v;
                Task::none()
            }
            Message::Browse => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_directory(default_install_dir())
                        .pick_folder()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::BrowseDone,
            ),
            Message::BrowseDone(Some(p)) => {
                self.dir = p.display().to_string();
                Task::none()
            }
            Message::BrowseDone(None) => Task::none(),
            Message::AssociateToggled(v) => {
                self.associate = v;
                Task::none()
            }
            Message::Install => {
                if self.busy || self.done {
                    return Task::none();
                }
                let opts = InstallOptions {
                    dir: PathBuf::from(self.dir.trim()),
                    associate: self.associate,
                    lang: self.lang.code().to_string(),
                };
                if opts.dir.as_os_str().is_empty() {
                    self.status = self.tr("请选择安装目录", "Choose an install folder");
                    return Task::none();
                }
                self.busy = true;
                self.status = self.tr("正在安装…", "Installing…");

                #[cfg(windows)]
                {
                    if !is_elevated() {
                        let args = encode_install_args(&opts);
                        match relaunch_elevated(&args) {
                            Ok(()) => {
                                // Parent exits; elevated child performs install.
                                std::process::exit(0);
                            }
                            Err(e) => {
                                self.busy = false;
                                self.status = format!("{e}");
                                return Task::none();
                            }
                        }
                    }
                }

                Task::perform(
                    async move {
                        perform_install(&opts).map_err(|e| e.to_string())?;
                        let _ = launch_app(&opts.dir);
                        Ok(())
                    },
                    Message::InstallFinished,
                )
            }
            Message::InstallFinished(Ok(())) => {
                self.busy = false;
                self.done = true;
                self.status = self.tr("安装完成", "Installation complete");
                Task::none()
            }
            Message::InstallFinished(Err(e)) => {
                self.busy = false;
                self.status = e;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let title = text(self.title()).size(22);
        let lang_row = row![
            Space::with_width(Length::Fill),
            button(text(Lang::Zh.label()))
                .on_press(Message::LangZh)
                .style(if self.lang == Lang::Zh {
                    iced::widget::button::primary
                } else {
                    iced::widget::button::secondary
                }),
            Space::with_width(8),
            button(text(Lang::En.label()))
                .on_press(Message::LangEn)
                .style(if self.lang == Lang::En {
                    iced::widget::button::primary
                } else {
                    iced::widget::button::secondary
                }),
        ]
        .align_y(iced::Alignment::Center);

        let path_label = text(self.tr("目标文件夹", "Destination folder"));
        let path_row = row![
            text_input("", &self.dir)
                .on_input(Message::DirChanged)
                .width(Length::Fill),
            Space::with_width(8),
            button(text("...")).on_press(Message::Browse),
        ]
        .align_y(iced::Alignment::Center);

        let associate = checkbox(
            self.tr(
                "关联常见压缩格式，并安装资源管理器右键菜单",
                "Associate archive formats and install Explorer context menus",
            ),
            self.associate,
        )
        .on_toggle(Message::AssociateToggled);

        let license_note = text(self.tr(
            "点击下方按钮即表示同意 GNU AGPL-3.0 许可协议",
            "By continuing you agree to the GNU AGPL-3.0 license",
        ))
        .size(12);

        let install_btn = {
            let label = if self.done {
                self.tr("已安装", "Installed")
            } else if self.busy {
                self.tr("安装中…", "Installing…")
            } else {
                self.tr("同意并安装", "Agree and Install")
            };
            let b = button(text(label).size(18))
                .width(Length::Fill)
                .padding(14);
            if self.busy || self.done {
                b
            } else {
                b.on_press(Message::Install)
                    .style(iced::widget::button::primary)
            }
        };

        // Reserve a fixed line so the install button is never clipped.
        let status = text(if self.status.is_empty() {
            " "
        } else {
            self.status.as_str()
        })
        .size(13);

        let body = column![
            lang_row,
            title,
            Space::with_height(8),
            path_label,
            path_row,
            Space::with_height(4),
            associate,
            license_note,
            Space::with_height(Length::Fill),
            install_btn,
            status,
        ]
        .spacing(6)
        .padding(16);

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

struct UninstallApp {
    lang: Lang,
    dir: PathBuf,
    status: String,
    busy: bool,
    done: bool,
}

#[derive(Debug, Clone)]
enum UMessage {
    LangZh,
    LangEn,
    Confirm,
    Cancel,
    Finished(Result<(), String>),
}

impl UninstallApp {
    fn new(dir: Option<PathBuf>) -> (Self, Task<UMessage>) {
        let dir = resolve_install_dir(dir);
        (
            Self {
                lang: Lang::Zh,
                dir,
                status: String::new(),
                busy: false,
                done: false,
            },
            Task::none(),
        )
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

    fn update(&mut self, message: UMessage) -> Task<UMessage> {
        match message {
            UMessage::LangZh => {
                self.lang = Lang::Zh;
                Task::none()
            }
            UMessage::LangEn => {
                self.lang = Lang::En;
                Task::none()
            }
            UMessage::Cancel => {
                if !self.busy {
                    std::process::exit(0);
                }
                Task::none()
            }
            UMessage::Confirm => {
                if self.busy || self.done {
                    return Task::none();
                }
                self.busy = true;
                self.status = self.tr("正在卸载…", "Uninstalling…");
                let dir = Some(self.dir.clone());
                #[cfg(windows)]
                {
                    if !is_elevated() {
                        let args = encode_uninstall_args(dir.as_deref());
                        match relaunch_elevated(&args) {
                            Ok(()) => std::process::exit(0),
                            Err(e) => {
                                self.busy = false;
                                self.status = format!("{e}");
                                return Task::none();
                            }
                        }
                    }
                }
                Task::perform(
                    async move { perform_uninstall(dir).map_err(|e| e.to_string()) },
                    UMessage::Finished,
                )
            }
            UMessage::Finished(Ok(())) => {
                self.busy = false;
                self.done = true;
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
        let lang_row = row![
            Space::with_width(Length::Fill),
            button(text(Lang::Zh.label()))
                .on_press(UMessage::LangZh)
                .style(if self.lang == Lang::Zh {
                    iced::widget::button::primary
                } else {
                    iced::widget::button::secondary
                }),
            Space::with_width(8),
            button(text(Lang::En.label()))
                .on_press(UMessage::LangEn)
                .style(if self.lang == Lang::En {
                    iced::widget::button::primary
                } else {
                    iced::widget::button::secondary
                }),
        ]
        .align_y(iced::Alignment::Center);

        let path_line = text(format!(
            "{} {}",
            self.tr("安装目录：", "Install folder:"),
            self.dir.display()
        ))
        .size(13);

        let actions = if self.done {
            row![button(text(self.tr("完成", "Done")).size(16))
                .width(Length::Fill)
                .padding(12)
                .on_press(UMessage::Cancel)
                .style(iced::widget::button::primary)]
        } else {
            let uninstall = {
                let label = if self.busy {
                    self.tr("请稍候…", "Please wait…")
                } else {
                    self.tr("卸载", "Uninstall")
                };
                let b = button(text(label).size(16))
                    .width(Length::Fill)
                    .padding(12);
                if self.busy {
                    b
                } else {
                    b.on_press(UMessage::Confirm)
                        .style(iced::widget::button::danger)
                }
            };
            let cancel = {
                let b = button(text(self.tr("取消", "Cancel")).size(16))
                    .width(Length::Fill)
                    .padding(12);
                if self.busy {
                    b
                } else {
                    b.on_press(UMessage::Cancel)
                }
            };
            row![uninstall, Space::with_width(12), cancel]
        };

        let status = if self.status.is_empty() {
            text(self.prompt()).size(14)
        } else {
            text(&self.status).size(14)
        };

        container(
            column![
                lang_row,
                text(self.title()).size(22),
                Space::with_height(8),
                path_line,
                Space::with_height(8),
                status,
                Space::with_height(Length::Fill),
                actions,
            ]
            .spacing(8)
            .padding(20),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
