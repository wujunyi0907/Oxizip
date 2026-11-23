//! Multi-tab Settings dialog (left-nav layout, clean-room).
//! Opened as a dedicated `--settings` window; edits stay in a draft until OK.

use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_input, Column,
    Space,
};
use iced::{Alignment, Element, Length};
use oxizip_engine::{FormatId, MethodId};
use oxizip_i18n::{t, Lang};

use crate::prefs::{ColorAccent, DoubleClickAction, Prefs, ShellMenuPrefs};
use crate::setup;

/// Left-nav tabs (language stays on Settings → Language menu; no Security page).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Assoc,
    Shell,
    Extract,
    Archive,
    Viewer,
    Advanced,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 7] = [
        Self::General,
        Self::Assoc,
        Self::Shell,
        Self::Extract,
        Self::Archive,
        Self::Viewer,
        Self::Advanced,
    ];

    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::General => "gui.settings.tab.general",
            Self::Assoc => "gui.settings.tab.assoc",
            Self::Shell => "gui.settings.tab.shell",
            Self::Extract => "gui.settings.tab.extract",
            Self::Archive => "gui.settings.tab.archive",
            Self::Viewer => "gui.settings.tab.viewer",
            Self::Advanced => "gui.settings.tab.advanced",
        }
    }

    pub fn parse_cli(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "general" => Some(Self::General),
            "assoc" | "association" | "associations" => Some(Self::Assoc),
            "shell" | "menu" => Some(Self::Shell),
            "extract" => Some(Self::Extract),
            "archive" | "compress" => Some(Self::Archive),
            "viewer" | "view" => Some(Self::Viewer),
            "advanced" | "etc" => Some(Self::Advanced),
            _ => None,
        }
    }

    pub fn cli_name(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Assoc => "assoc",
            Self::Shell => "shell",
            Self::Extract => "extract",
            Self::Archive => "archive",
            Self::Viewer => "viewer",
            Self::Advanced => "advanced",
        }
    }
}

/// Messages produced by the settings UI (forwarded into the app `Message`).
#[derive(Debug, Clone)]
pub enum SettingsMsg {
    Tab(SettingsTab),
    Reset,
    /// Commit draft and close the settings window.
    Ok,
    ExtToggled(&'static str, bool),
    ToggleSelectAllAssoc,
    SelectAllAssocDefaults,
    DoubleClick(DoubleClickAction),
    DeleteAfterDblExtract(bool),
    ShellExtractHere(bool),
    ShellExtractSmart(bool),
    ShellExtractToFolder(bool),
    ShellExtractHereDel(bool),
    ShellExtractSmartDel(bool),
    ShellExtractToFolderDel(bool),
    ShellExtractDialog(bool),
    ShellOpenWith(bool),
    ShellTest(bool),
    ShellChecksum(bool),
    ShellCompressZip(bool),
    ShellCompress7z(bool),
    ShellCompressZipx(bool),
    ShellCompressTar(bool),
    ShellCompressTgz(bool),
    ShellCompressGz(bool),
    ShellCompressXz(bool),
    ShellCompressZpaq(bool),
    ShellCompressEachZip(bool),
    ShellCompressEach7z(bool),
    ShellCascaded(bool),
    ShellDisabled(bool),
    ShellHideIcon(bool),
    EscToExit(bool),
    ShowDotDot(bool),
    ShowGrid(bool),
    FullRowSelect(bool),
    SingleClickOpen(bool),
    AlwaysOnTop(bool),
    ShowStatusBar(bool),
    TestAfterModify(bool),
    ShowTree(bool),
    ShowComment(bool),
    OpenDest(bool),
    JobKeepOpenExtract(bool),
    JobKeepOpenCreate(bool),
    EnterSingleRoot(bool),
    SmartSingleFileFolder(bool),
    DefaultFormat(FormatId),
    DefaultMethodAuto,
    DefaultMethod(MethodId),
    CompressionLevel(Option<u8>),
    Utf8Zip(bool),
    ExcludeThumbsDb(bool),
    TestAfterCompress(bool),
    NoRootSingleFolder(bool),
    NewArchiveUseLast(bool),
    NewArchiveFolderChanged(String),
    BrowseNewArchiveFolder,
    ExcludeFilterChanged(String),
    ColSize(bool),
    ColCompressed(bool),
    ColMethod(bool),
    ColModified(bool),
    ColKind(bool),
    ColorAccent(ColorAccent),
    CopyZoneId(bool),
    ShowRecent(bool),
    UseLongPaths(bool),
    DeleteOnCancel(bool),
    TitleFullPath(bool),
    AutoExpandTree(bool),
    DeleteToTrash(bool),
    ShowStartScreen(bool),
    DeleteTempAsap(bool),
    TempFolderChanged(String),
    BrowseTempFolder,
    ExportPrefs,
    ImportPrefs,
    ClearRecent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatLabel(pub FormatId);

impl std::fmt::Display for FormatLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodLabel {
    Auto,
    Method(MethodId),
}

impl MethodLabel {
    pub fn all() -> &'static [MethodLabel] {
        &[
            Self::Auto,
            Self::Method(MethodId::Store),
            Self::Method(MethodId::Deflate),
            Self::Method(MethodId::Bzip2),
            Self::Method(MethodId::Lzma),
            Self::Method(MethodId::Xz),
            Self::Method(MethodId::Zstd),
            Self::Method(MethodId::Lzma2),
            Self::Method(MethodId::Ppmd),
        ]
    }

    pub fn from_prefs(prefs: &Prefs) -> Self {
        match prefs.default_method_id() {
            None => Self::Auto,
            Some(m) => Self::Method(m),
        }
    }

    pub fn to_method(self) -> Option<MethodId> {
        match self {
            Self::Auto => None,
            Self::Method(m) => Some(m),
        }
    }
}

impl std::fmt::Display for MethodLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Method(m) => f.write_str(m.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccentLabel(pub ColorAccent);

impl std::fmt::Display for AccentLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.0 {
            ColorAccent::BrandBlue => "brand",
            ColorAccent::DeepNavy => "navy",
            ColorAccent::Teal => "teal",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelLabel(pub Option<u8>);

impl LevelLabel {
    pub fn all() -> &'static [LevelLabel] {
        &[
            LevelLabel(None),
            LevelLabel(Some(0)),
            LevelLabel(Some(1)),
            LevelLabel(Some(3)),
            LevelLabel(Some(5)),
            LevelLabel(Some(7)),
            LevelLabel(Some(9)),
        ]
    }
}

impl std::fmt::Display for LevelLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            None => f.write_str("auto"),
            Some(n) => write!(f, "{n}"),
        }
    }
}

pub struct SettingsView<'a> {
    pub lang: Lang,
    pub tab: SettingsTab,
    pub prefs: &'a Prefs,
    pub setup_exts: &'a std::collections::BTreeMap<&'static str, bool>,
    pub status: &'a str,
}

pub fn view_settings(state: SettingsView<'_>) -> Element<'_, SettingsMsg> {
    let nav = SettingsTab::ALL.iter().fold(
        Column::new().spacing(4).width(Length::Fixed(120.0)),
        |col, tab| {
            let label = t(state.lang, tab.i18n_key());
            let btn = if *tab == state.tab {
                button(text(label).size(12))
                    .width(Length::Fill)
                    .padding(8)
                    .style(iced::widget::button::primary)
                    .on_press(SettingsMsg::Tab(*tab))
            } else {
                button(text(label).size(12))
                    .width(Length::Fill)
                    .padding(8)
                    .style(iced::widget::button::secondary)
                    .on_press(SettingsMsg::Tab(*tab))
            };
            col.push(btn)
        },
    );

    let content: Element<'_, SettingsMsg> = match state.tab {
        SettingsTab::General => page_general(state.lang, state.prefs),
        SettingsTab::Assoc => page_assoc(state.lang, state.setup_exts),
        SettingsTab::Shell => page_shell(state.lang, &state.prefs.shell),
        SettingsTab::Extract => page_extract(state.lang, state.prefs),
        SettingsTab::Archive => page_archive(state.lang, state.prefs),
        SettingsTab::Viewer => page_viewer(state.lang, state.prefs),
        SettingsTab::Advanced => page_advanced(state.lang, state.prefs),
    };

    let footer = row![
        button(text(t(state.lang, "gui.settings.reset")).size(12))
            .padding(8)
            .style(iced::widget::button::secondary)
            .on_press(SettingsMsg::Reset),
        Space::with_width(Length::Fill),
        button(text(t(state.lang, "gui.settings.ok")).size(13))
            .padding(10)
            .style(iced::widget::button::primary)
            .on_press(SettingsMsg::Ok),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let status_line: Element<'_, SettingsMsg> = container(
        text(if state.status.is_empty() {
            " "
        } else {
            state.status
        })
        .size(11),
    )
    .width(Length::Fill)
    .height(Length::Fixed(18.0))
    .into();

    let body = column![
        text(t(state.lang, "gui.settings.title")).size(16),
        row![
            scrollable(nav).height(Length::Fill),
            container(scrollable(content))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(8),
        ]
        .spacing(8)
        .height(Length::Fill),
        status_line,
        footer,
    ]
    .spacing(8)
    .padding(12)
    .width(Length::Fill)
    .height(Length::Fill);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn page_general(lang: Lang, prefs: &Prefs) -> Element<'_, SettingsMsg> {
    column![
        text(t(lang, "gui.settings.general.heading")).size(14),
        checkbox(t(lang, "gui.settings.general.esc"), prefs.esc_to_exit)
            .on_toggle(SettingsMsg::EscToExit),
        checkbox(
            t(lang, "gui.settings.general.test_modify"),
            prefs.test_after_modify
        )
        .on_toggle(SettingsMsg::TestAfterModify),
        checkbox(t(lang, "gui.settings.general.dotdot"), prefs.show_dotdot)
            .on_toggle(SettingsMsg::ShowDotDot),
        checkbox(t(lang, "gui.settings.general.grid"), prefs.show_gridlines)
            .on_toggle(SettingsMsg::ShowGrid),
        checkbox(
            t(lang, "gui.settings.general.full_row"),
            prefs.full_row_select
        )
        .on_toggle(SettingsMsg::FullRowSelect),
        checkbox(
            t(lang, "gui.settings.general.single_click"),
            prefs.single_click_open
        )
        .on_toggle(SettingsMsg::SingleClickOpen),
        checkbox(t(lang, "gui.menu.view.top"), prefs.always_on_top)
            .on_toggle(SettingsMsg::AlwaysOnTop),
        checkbox(t(lang, "gui.menu.view.status"), prefs.show_status_bar)
            .on_toggle(SettingsMsg::ShowStatusBar),
    ]
    .spacing(8)
    .into()
}

fn page_assoc<'a>(
    lang: Lang,
    setup_exts: &'a std::collections::BTreeMap<&'static str, bool>,
) -> Element<'a, SettingsMsg> {
    let mut list = Column::new().spacing(6);
    for (group, exts) in setup::assoc_catalog() {
        list = list.push(text(t(lang, group.i18n_key())).size(12));
        for ext in *exts {
            let checked = setup_exts.get(ext).copied().unwrap_or(false);
            let e = *ext;
            list = list.push(
                checkbox(format!(".{ext}"), checked)
                    .on_toggle(move |v| SettingsMsg::ExtToggled(e, v)),
            );
        }
    }
    let all_on = !setup_exts.is_empty() && setup_exts.values().all(|v| *v);
    let toggle_label = if all_on {
        t(lang, "gui.setup.clear_all")
    } else {
        t(lang, "gui.setup.select_all")
    };
    column![
        text(t(lang, "gui.settings.assoc.heading")).size(14),
        text(t(lang, "gui.settings.assoc.intro_ok")).size(11),
        list,
        row![
            button(text(toggle_label).size(12))
                .padding(8)
                .style(iced::widget::button::secondary)
                .on_press(SettingsMsg::ToggleSelectAllAssoc),
            button(text(t(lang, "gui.settings.assoc.all")).size(12))
                .padding(8)
                .style(iced::widget::button::secondary)
                .on_press(SettingsMsg::SelectAllAssocDefaults),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .into()
}

fn page_shell(lang: Lang, shell: &ShellMenuPrefs) -> Element<'_, SettingsMsg> {
    let dbl = [
        (DoubleClickAction::Open, "gui.settings.shell.dbl.open"),
        (
            DoubleClickAction::ExtractSmart,
            "gui.settings.shell.dbl.smart",
        ),
        (
            DoubleClickAction::ExtractToFolder,
            "gui.settings.shell.dbl.folder",
        ),
    ];
    let mut dbl_col = Column::new().spacing(4);
    dbl_col = dbl_col.push(text(t(lang, "gui.settings.shell.dbl")).size(12));
    for (action, key) in dbl {
        let on = shell.double_click == action;
        dbl_col = dbl_col.push(checkbox(t(lang, key), on).on_toggle(move |v| {
            if v {
                SettingsMsg::DoubleClick(action)
            } else {
                SettingsMsg::DoubleClick(DoubleClickAction::Open)
            }
        }));
    }
    dbl_col = dbl_col.push(
        checkbox(
            t(lang, "gui.settings.shell.dbl.delete"),
            shell.delete_after_dbl_extract,
        )
        .on_toggle(SettingsMsg::DeleteAfterDblExtract),
    );

    column![
        text(t(lang, "gui.settings.shell.heading")).size(14),
        text(t(lang, "gui.settings.shell.note_ok")).size(11),
        dbl_col,
        text(t(lang, "gui.settings.shell.extract")).size(12),
        checkbox(
            t(lang, "gui.settings.shell.extract_here"),
            shell.extract_here
        )
        .on_toggle(SettingsMsg::ShellExtractHere),
        checkbox(
            t(lang, "gui.settings.shell.extract_smart"),
            shell.extract_smart
        )
        .on_toggle(SettingsMsg::ShellExtractSmart),
        checkbox(
            t(lang, "gui.settings.shell.extract_folder"),
            shell.extract_to_folder
        )
        .on_toggle(SettingsMsg::ShellExtractToFolder),
        checkbox(
            t(lang, "gui.settings.shell.extract_here_del"),
            shell.extract_here_del
        )
        .on_toggle(SettingsMsg::ShellExtractHereDel),
        checkbox(
            t(lang, "gui.settings.shell.extract_smart_del"),
            shell.extract_smart_del
        )
        .on_toggle(SettingsMsg::ShellExtractSmartDel),
        checkbox(
            t(lang, "gui.settings.shell.extract_folder_del"),
            shell.extract_to_folder_del
        )
        .on_toggle(SettingsMsg::ShellExtractToFolderDel),
        checkbox(
            t(lang, "gui.settings.shell.extract_dialog"),
            shell.extract_dialog
        )
        .on_toggle(SettingsMsg::ShellExtractDialog),
        checkbox(t(lang, "gui.settings.shell.open_with"), shell.open_with)
            .on_toggle(SettingsMsg::ShellOpenWith),
        checkbox(t(lang, "gui.settings.shell.test"), shell.test)
            .on_toggle(SettingsMsg::ShellTest),
        checkbox(t(lang, "gui.settings.shell.checksum"), shell.checksum)
            .on_toggle(SettingsMsg::ShellChecksum),
        text(t(lang, "gui.settings.shell.compress")).size(12),
        checkbox(t(lang, "gui.settings.shell.zip"), shell.compress_zip)
            .on_toggle(SettingsMsg::ShellCompressZip),
        checkbox(t(lang, "gui.settings.shell.sevenz"), shell.compress_7z)
            .on_toggle(SettingsMsg::ShellCompress7z),
        checkbox(t(lang, "gui.settings.shell.zipx"), shell.compress_zipx)
            .on_toggle(SettingsMsg::ShellCompressZipx),
        checkbox(t(lang, "gui.settings.shell.tar"), shell.compress_tar)
            .on_toggle(SettingsMsg::ShellCompressTar),
        checkbox(t(lang, "gui.settings.shell.tgz"), shell.compress_tgz)
            .on_toggle(SettingsMsg::ShellCompressTgz),
        checkbox(t(lang, "gui.settings.shell.gz"), shell.compress_gz)
            .on_toggle(SettingsMsg::ShellCompressGz),
        checkbox(t(lang, "gui.settings.shell.xz"), shell.compress_xz)
            .on_toggle(SettingsMsg::ShellCompressXz),
        checkbox(t(lang, "gui.settings.shell.zpaq"), shell.compress_zpaq)
            .on_toggle(SettingsMsg::ShellCompressZpaq),
        checkbox(
            t(lang, "gui.settings.shell.each_zip"),
            shell.compress_each_zip
        )
        .on_toggle(SettingsMsg::ShellCompressEachZip),
        checkbox(
            t(lang, "gui.settings.shell.each_7z"),
            shell.compress_each_7z
        )
        .on_toggle(SettingsMsg::ShellCompressEach7z),
        text(t(lang, "gui.settings.shell.other")).size(12),
        checkbox(t(lang, "gui.settings.shell.cascade"), shell.cascaded)
            .on_toggle(SettingsMsg::ShellCascaded),
        checkbox(t(lang, "gui.settings.shell.disable"), shell.disabled)
            .on_toggle(SettingsMsg::ShellDisabled),
        checkbox(t(lang, "gui.settings.shell.hide_icon"), shell.hide_icon)
            .on_toggle(SettingsMsg::ShellHideIcon),
    ]
    .spacing(6)
    .into()
}

fn page_extract(lang: Lang, prefs: &Prefs) -> Element<'_, SettingsMsg> {
    column![
        text(t(lang, "gui.settings.extract.heading")).size(14),
        checkbox(
            t(lang, "gui.settings.open_dest"),
            prefs.open_dest_after_extract
        )
        .on_toggle(SettingsMsg::OpenDest),
        checkbox(
            t(lang, "gui.settings.extract.keep_job"),
            prefs.job_keep_open_after_extract
        )
        .on_toggle(SettingsMsg::JobKeepOpenExtract),
        checkbox(
            t(lang, "gui.settings.extract.enter_root"),
            prefs.enter_single_root_folder
        )
        .on_toggle(SettingsMsg::EnterSingleRoot),
        checkbox(
            t(lang, "gui.settings.extract.smart_single"),
            prefs.smart_single_file_folder
        )
        .on_toggle(SettingsMsg::SmartSingleFileFolder),
    ]
    .spacing(8)
    .into()
}

fn page_archive(lang: Lang, prefs: &Prefs) -> Element<'_, SettingsMsg> {
    let formats: Vec<FormatLabel> = FormatId::create_choices()
        .iter()
        .copied()
        .map(FormatLabel)
        .collect();
    let methods: Vec<MethodLabel> = MethodLabel::all().to_vec();
    let current_method = MethodLabel::from_prefs(prefs);
    let levels: Vec<LevelLabel> = LevelLabel::all().to_vec();

    column![
        text(t(lang, "gui.settings.archive.heading")).size(14),
        row![
            text(t(lang, "gui.settings.default_format"))
                .size(12)
                .width(Length::Fixed(140.0)),
            pick_list(
                formats,
                Some(FormatLabel(prefs.default_format_id())),
                |f| SettingsMsg::DefaultFormat(f.0),
            )
            .width(160),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        row![
            text(t(lang, "gui.settings.default_method"))
                .size(12)
                .width(Length::Fixed(140.0)),
            pick_list(methods, Some(current_method), |m| {
                match m.to_method() {
                    None => SettingsMsg::DefaultMethodAuto,
                    Some(id) => SettingsMsg::DefaultMethod(id),
                }
            })
            .width(160),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        row![
            text(t(lang, "gui.settings.archive.level"))
                .size(12)
                .width(Length::Fixed(140.0)),
            pick_list(
                levels,
                Some(LevelLabel(prefs.compression_level)),
                |l| SettingsMsg::CompressionLevel(l.0),
            )
            .width(160),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        checkbox(t(lang, "gui.settings.archive.utf8"), prefs.utf8_zip_names)
            .on_toggle(SettingsMsg::Utf8Zip),
        checkbox(
            t(lang, "gui.settings.archive.thumbs"),
            prefs.exclude_thumbs_db
        )
        .on_toggle(SettingsMsg::ExcludeThumbsDb),
        checkbox(
            t(lang, "gui.settings.archive.test_after"),
            prefs.test_after_compress
        )
        .on_toggle(SettingsMsg::TestAfterCompress),
        checkbox(
            t(lang, "gui.settings.archive.no_root"),
            prefs.no_root_for_single_folder
        )
        .on_toggle(SettingsMsg::NoRootSingleFolder),
        checkbox(
            t(lang, "gui.settings.archive.keep_job"),
            prefs.job_keep_open_after_create
        )
        .on_toggle(SettingsMsg::JobKeepOpenCreate),
        checkbox(
            t(lang, "gui.settings.archive.use_last"),
            prefs.new_archive_use_last
        )
        .on_toggle(SettingsMsg::NewArchiveUseLast),
        text(t(lang, "gui.settings.archive.default_folder")).size(12),
        row![
            text_input(
                t(lang, "gui.settings.archive.default_folder_ph"),
                &prefs.new_archive_folder
            )
            .on_input(SettingsMsg::NewArchiveFolderChanged)
            .width(Length::Fill),
            button(text("…").size(12))
                .padding(8)
                .style(iced::widget::button::secondary)
                .on_press(SettingsMsg::BrowseNewArchiveFolder),
        ]
        .spacing(8),
        text(t(lang, "gui.settings.archive.exclude")).size(12),
        text_input(
            t(lang, "gui.settings.archive.exclude_ph"),
            &prefs.exclude_filter
        )
        .on_input(SettingsMsg::ExcludeFilterChanged),
    ]
    .spacing(8)
    .into()
}

fn page_viewer(lang: Lang, prefs: &Prefs) -> Element<'_, SettingsMsg> {
    let cols = &prefs.columns;
    let accents = [
        ColorAccent::BrandBlue,
        ColorAccent::DeepNavy,
        ColorAccent::Teal,
    ]
    .map(AccentLabel)
    .to_vec();

    column![
        text(t(lang, "gui.settings.viewer.heading")).size(14),
        text(t(lang, "gui.settings.viewer.preview_note")).size(11),
        checkbox(t(lang, "gui.menu.view.tree"), prefs.show_tree)
            .on_toggle(SettingsMsg::ShowTree),
        checkbox(t(lang, "gui.menu.view.comment"), prefs.show_comment)
            .on_toggle(SettingsMsg::ShowComment),
        text(t(lang, "gui.settings.viewer.columns")).size(12),
        checkbox(t(lang, "gui.settings.viewer.col.size"), cols.size)
            .on_toggle(SettingsMsg::ColSize),
        checkbox(
            t(lang, "gui.settings.viewer.col.compressed"),
            cols.compressed
        )
        .on_toggle(SettingsMsg::ColCompressed),
        checkbox(t(lang, "gui.settings.viewer.col.method"), cols.method)
            .on_toggle(SettingsMsg::ColMethod),
        checkbox(
            t(lang, "gui.settings.viewer.col.modified"),
            cols.modified
        )
        .on_toggle(SettingsMsg::ColModified),
        checkbox(t(lang, "gui.settings.viewer.col.kind"), cols.kind)
            .on_toggle(SettingsMsg::ColKind),
        row![
            text(t(lang, "gui.settings.viewer.accent"))
                .size(12)
                .width(Length::Fixed(140.0)),
            pick_list(
                accents,
                Some(AccentLabel(prefs.color_accent)),
                |a| SettingsMsg::ColorAccent(a.0),
            )
            .width(160),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

fn page_advanced(lang: Lang, prefs: &Prefs) -> Element<'_, SettingsMsg> {
    column![
        text(t(lang, "gui.settings.advanced.heading")).size(14),
        checkbox(
            t(lang, "gui.settings.security.zone"),
            prefs.copy_zone_identifier
        )
        .on_toggle(SettingsMsg::CopyZoneId),
        checkbox(t(lang, "gui.settings.advanced.recent"), prefs.show_recent)
            .on_toggle(SettingsMsg::ShowRecent),
        checkbox(
            t(lang, "gui.settings.advanced.long_path"),
            prefs.use_long_paths
        )
        .on_toggle(SettingsMsg::UseLongPaths),
        checkbox(
            t(lang, "gui.settings.advanced.del_cancel"),
            prefs.delete_on_extract_cancel
        )
        .on_toggle(SettingsMsg::DeleteOnCancel),
        checkbox(
            t(lang, "gui.settings.advanced.title_path"),
            prefs.title_full_path
        )
        .on_toggle(SettingsMsg::TitleFullPath),
        checkbox(
            t(lang, "gui.settings.advanced.expand_tree"),
            prefs.auto_expand_tree
        )
        .on_toggle(SettingsMsg::AutoExpandTree),
        checkbox(
            t(lang, "gui.settings.advanced.trash"),
            prefs.delete_to_trash
        )
        .on_toggle(SettingsMsg::DeleteToTrash),
        checkbox(
            t(lang, "gui.settings.advanced.start_screen"),
            prefs.show_start_screen
        )
        .on_toggle(SettingsMsg::ShowStartScreen),
        checkbox(
            t(lang, "gui.settings.advanced.temp_asap"),
            prefs.delete_temp_asap
        )
        .on_toggle(SettingsMsg::DeleteTempAsap),
        text(t(lang, "gui.settings.advanced.temp")).size(12),
        row![
            text_input(
                t(lang, "gui.settings.advanced.temp_ph"),
                &prefs.temp_folder
            )
            .on_input(SettingsMsg::TempFolderChanged)
            .width(Length::Fill),
            button(text("…").size(12))
                .padding(8)
                .style(iced::widget::button::secondary)
                .on_press(SettingsMsg::BrowseTempFolder),
        ]
        .spacing(8),
        row![
            button(text(t(lang, "gui.settings.advanced.export")).size(12))
                .padding(8)
                .style(iced::widget::button::secondary)
                .on_press(SettingsMsg::ExportPrefs),
            button(text(t(lang, "gui.settings.advanced.import")).size(12))
                .padding(8)
                .style(iced::widget::button::secondary)
                .on_press(SettingsMsg::ImportPrefs),
            button(text(t(lang, "gui.settings.advanced.clear_recent")).size(12))
                .padding(8)
                .style(iced::widget::button::secondary)
                .on_press(SettingsMsg::ClearRecent),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .into()
}
