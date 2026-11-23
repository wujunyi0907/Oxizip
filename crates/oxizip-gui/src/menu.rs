//! Menu bar definitions and dropdown helpers.

use iced::widget::{button, column, container, row, svg, text, Space};
use iced::{Alignment, Element, Length, Padding};
use oxizip_i18n::{t, Lang};

use crate::prefs::Prefs;
use crate::{
    dropdown_item_style, dropdown_panel_style, home_menu_bar_style, home_menu_btn_style, Message,
};

/// Compact dropdown panel width (px).
pub const MENU_WIDTH: f32 = 156.0;
const PANEL_PAD: f32 = 4.0;
const ITEM_GAP: f32 = 1.0;
/// Fixed row height so submenu top offset matches the triggering item.
const ITEM_H: f32 = 22.0;
const SEP_H: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuPopup {
    #[default]
    None,
    File,
    FileRecent,
    FileExtract,
    Edit,
    Find,
    Settings,
    SettingsLang,
    View,
    ViewToolbar,
    ViewList,
    ViewColumn,
    Tools,
    Help,
}

impl MenuPopup {
    /// Top-level menu this popup belongs under (for bar highlight / left offset).
    pub fn root(self) -> Self {
        match self {
            Self::FileRecent | Self::FileExtract => Self::File,
            Self::SettingsLang => Self::Settings,
            Self::ViewToolbar | Self::ViewList | Self::ViewColumn => Self::View,
            other => other,
        }
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Open,
    Recent(usize),
    ClearRecent,
    Close,
    Create,
    NewWindow,
    NewWindowAdmin,
    SaveAs,
    DeleteArchive,
    DeleteArchivePermanent,
    OpenArchiveFolder,
    Test,
    ExtractTo,
    ExtractHere,
    ExtractSmart,
    Exit,
    Rename,
    Add,
    DeleteEntries,
    EditComment,
    CopyFilesClipboard,
    PasteFilesClipboard,
    CopyFullPaths,
    Find,
    ShowSettings,
    SetLang(oxizip_i18n::Lang),
    StartScreen,
    ToggleTree,
    ToggleComment,
    ToggleStatusBar,
    ToolbarLarge,
    ToolbarSmall,
    ToolbarHide,
    ToolbarCustomize,
    ListIcons,
    ListList,
    ListDetails,
    ListLargeIcons,
    ToggleColSize,
    ToggleColCompressed,
    ToggleColMethod,
    ToggleColModified,
    ToggleColKind,
    ToggleAlwaysOnTop,
    ColorSettings,
    Repair,
    RecoverPassword,
    Checksum,
    About,
    OpenHelp,
    OpenInstallFolder,
    Licenses,
    PasswordManager,
    Associations,
    OpenSettingsLang,
    OpenFileRecent,
    OpenFileExtract,
    OpenViewToolbar,
    OpenViewList,
    OpenViewColumn,
}

pub struct MenuCtx<'a> {
    pub lang: Lang,
    pub has_archive: bool,
    pub busy: bool,
    pub selection_count: usize,
    pub is_zip_comment: bool,
    pub prefs: &'a Prefs,
}

impl MenuCtx<'_> {
    fn can_archive(&self) -> bool {
        self.has_archive && !self.busy
    }

    fn can_edit_entries(&self) -> bool {
        self.can_archive() && self.selection_count > 0
    }
}

/// Horizontal padding shared by bar buttons and overlay width anchors.
pub const MENU_BAR_PAD_X: f32 = 6.0;
pub const MENU_BAR_GAP: f32 = 0.0;
pub const MENU_BAR_PAD: Padding = Padding {
    top: 0.0,
    right: 4.0,
    bottom: 0.0,
    left: 4.0,
};
pub const MENU_LABEL_SIZE: u16 = 11;

const MENU_TOPS: &[(MenuPopup, &str)] = &[
    (MenuPopup::File, "gui.home.menu.file"),
    (MenuPopup::Edit, "gui.home.menu.edit"),
    (MenuPopup::Find, "gui.home.menu.search"),
    (MenuPopup::Settings, "gui.home.menu.settings"),
    (MenuPopup::View, "gui.home.menu.view"),
    (MenuPopup::Tools, "gui.home.menu.tools"),
    (MenuPopup::Help, "gui.home.menu.help"),
];

pub fn menu_bar_row<'a>(lang: Lang, _popup: MenuPopup) -> Element<'a, Message> {
    let mut items = row![].spacing(MENU_BAR_GAP).padding(MENU_BAR_PAD);
    for &(target, key) in MENU_TOPS {
        items = items.push(menu_top(lang, key, target));
    }
    container(items)
        .width(Length::Fill)
        .style(home_menu_bar_style)
        .into()
}

/// Overlay under the bar. Spacers use the **same fixed widths** as the top buttons
/// (`menu_item_width`), so the active flyout's left edge matches the menu item.
pub fn menu_dropdown_overlay<'a>(popup: MenuPopup, ctx: &MenuCtx<'_>) -> Element<'a, Message> {
    if popup == MenuPopup::None {
        return Space::with_height(0).into();
    }
    let mut items = row![].spacing(MENU_BAR_GAP).padding(MENU_BAR_PAD);
    for &(target, key) in MENU_TOPS {
        let w = menu_item_width(t(ctx.lang, key));
        if popup.root() == target {
            items = items.push(
                column![view_dropdown(popup, ctx)]
                    .width(Length::Shrink)
                    .align_x(Alignment::Start),
            );
        } else {
            items = items.push(Space::with_width(Length::Fixed(w)));
        }
    }
    items = items.push(Space::with_width(Length::Fill));
    items.into()
}

fn menu_top<'a>(lang: Lang, key: &'static str, popup: MenuPopup) -> Element<'a, Message> {
    let label = t(lang, key);
    button(text(label).size(MENU_LABEL_SIZE))
        .padding(menu_btn_padding())
        .width(Length::Fixed(menu_item_width(label)))
        .style(home_menu_btn_style)
        .on_press(Message::ToggleMenu(popup))
        .into()
}

fn menu_btn_padding() -> Padding {
    Padding {
        top: 1.0,
        right: MENU_BAR_PAD_X,
        bottom: 1.0,
        left: MENU_BAR_PAD_X,
    }
}

/// Shared width for a top-level menu button and its overlay spacer.
fn menu_item_width(label: &str) -> f32 {
    let text_w: f32 = label
        .chars()
        .map(|c| {
            if !c.is_ascii() {
                f32::from(MENU_LABEL_SIZE)
            } else if c.is_ascii_alphanumeric() {
                6.6
            } else {
                4.8
            }
        })
        .sum();
    // padding L+R; small fudge so text is not clipped inside the fixed button
    text_w + MENU_BAR_PAD_X * 2.0 + 2.0
}

fn item_btn<'a>(label: String, enabled: bool, action: MenuAction) -> Element<'a, Message> {
    let mut b = button(text(label).size(12))
        .padding(Padding {
            top: 2.0,
            right: 6.0,
            bottom: 2.0,
            left: 6.0,
        })
        .height(Length::Fixed(ITEM_H))
        .style(dropdown_item_style)
        .width(Length::Fill);
    if enabled {
        b = b.on_press(Message::MenuAction(action));
    }
    b.into()
}

fn sep<'a>() -> Element<'a, Message> {
    container(
        container(Space::with_height(1))
            .width(Length::Fill)
            .height(1)
            .style(|_theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb8(
                    0xD0, 0xD0, 0xD0,
                ))),
                ..Default::default()
            }),
    )
    .center_y(Length::Fixed(SEP_H))
    .width(Length::Fill)
    .into()
}

fn submenu_chevron<'a>(open: bool) -> Element<'a, Message> {
    const RIGHT: &[u8] = include_bytes!("../assets/icons/chevron_right.svg");
    const DOWN: &[u8] = include_bytes!("../assets/icons/chevron_down.svg");
    let handle = svg::Handle::from_memory(if open { DOWN } else { RIGHT });
    svg(handle)
        .width(10)
        .height(10)
        .style(|_theme, _status| svg::Style {
            color: Some(iced::Color::from_rgb8(0x33, 0x33, 0x33)),
        })
        .into()
}

fn submenu_btn<'a>(label: String, open: bool, action: MenuAction) -> Element<'a, Message> {
    button(
        row![
            text(label).size(12).width(Length::Fill),
            submenu_chevron(open),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding(Padding {
        top: 2.0,
        right: 6.0,
        bottom: 2.0,
        left: 6.0,
    })
    .height(Length::Fixed(ITEM_H))
    .style(dropdown_item_style)
    .width(Length::Fill)
    .on_press(Message::MenuAction(action))
    .into()
}

fn panel<'a>(items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut col = column![]
        .spacing(ITEM_GAP)
        .padding(PANEL_PAD)
        .width(MENU_WIDTH);
    for e in items {
        col = col.push(e);
    }
    container(col).style(dropdown_panel_style).into()
}

/// Vertical offset from panel top to the start of row `index` (0-based).
/// `sep_at` lists indices that are separators rather than `ITEM_H` rows.
fn item_top_offset(index: usize, sep_at: &[usize]) -> f32 {
    let mut y = PANEL_PAD;
    for i in 0..index {
        y += if sep_at.contains(&i) { SEP_H } else { ITEM_H };
        y += ITEM_GAP;
    }
    y
}

fn submenu_offset(popup: MenuPopup) -> f32 {
    match popup {
        // File: open, recent*, close, sep, new, win, admin, sep, save, del, perm, folder, sep, test, extract*, …
        MenuPopup::FileRecent => item_top_offset(1, &[3, 7, 12, 15]),
        MenuPopup::FileExtract => item_top_offset(14, &[3, 7, 12, 15]),
        // Settings: page, lang*, passman, assoc
        MenuPopup::SettingsLang => item_top_offset(1, &[]),
        // View: start, tree, comment, status, toolbar*, pick, list*, column*, top, color
        MenuPopup::ViewToolbar => item_top_offset(4, &[]),
        MenuPopup::ViewList => item_top_offset(6, &[]),
        MenuPopup::ViewColumn => item_top_offset(7, &[]),
        _ => 0.0,
    }
}

fn flyout<'a>(
    parent: Element<'a, Message>,
    child: Option<Element<'a, Message>>,
    top: f32,
) -> Element<'a, Message> {
    match child {
        Some(c) => row![
            parent,
            Space::with_width(2),
            column![Space::with_height(Length::Fixed(top)), c],
        ]
        .align_y(Alignment::Start)
        .into(),
        None => parent,
    }
}

pub fn view_dropdown<'a>(popup: MenuPopup, ctx: &MenuCtx<'_>) -> Element<'a, Message> {
    let lang = ctx.lang;
    let a = ctx.can_archive();
    let e = ctx.can_edit_entries();
    let busy = ctx.busy;

    match popup {
        MenuPopup::None => Space::with_height(0).into(),

        MenuPopup::File
        | MenuPopup::FileRecent
        | MenuPopup::FileExtract => {
            let child = match popup {
                MenuPopup::FileRecent => Some(panel_recent(ctx, busy)),
                MenuPopup::FileExtract => Some(panel_extract(lang, a)),
                _ => None,
            };
            let top = submenu_offset(popup);
            flyout(panel_file(lang, a, busy, popup), child, top)
        }

        MenuPopup::Edit => panel_edit(lang, a, e, ctx),
        MenuPopup::Find => panel(vec![item_btn(
            t(lang, "gui.menu.find.files").into(),
            a,
            MenuAction::Find,
        )]),

        MenuPopup::Settings | MenuPopup::SettingsLang => {
            let child = if popup == MenuPopup::SettingsLang {
                Some(panel(
                    oxizip_i18n::Lang::all()
                        .map(|l| item_btn(l.native_name().into(), true, MenuAction::SetLang(l)))
                        .collect(),
                ))
            } else {
                None
            };
            flyout(
                panel_settings(lang, popup),
                child,
                submenu_offset(popup),
            )
        }

        MenuPopup::View
        | MenuPopup::ViewToolbar
        | MenuPopup::ViewList
        | MenuPopup::ViewColumn => {
            let child = match popup {
                MenuPopup::ViewToolbar => Some(panel_view_toolbar(lang, ctx)),
                MenuPopup::ViewList => Some(panel_view_list(lang, ctx)),
                MenuPopup::ViewColumn => Some(panel_view_column(lang, ctx)),
                _ => None,
            };
            flyout(
                panel_view(lang, a, busy, ctx, popup),
                child,
                submenu_offset(popup),
            )
        }

        MenuPopup::Tools => panel(vec![
            item_btn(
                t(lang, "gui.menu.tools.repair").into(),
                !busy,
                MenuAction::Repair,
            ),
            item_btn(
                t(lang, "gui.menu.tools.recover").into(),
                a,
                MenuAction::RecoverPassword,
            ),
            item_btn(
                t(lang, "gui.menu.tools.checksum").into(),
                a,
                MenuAction::Checksum,
            ),
        ]),
        MenuPopup::Help => panel(vec![
            item_btn(
                t(lang, "gui.menu.help.about").into(),
                true,
                MenuAction::About,
            ),
            item_btn(
                t(lang, "gui.menu.help.page").into(),
                true,
                MenuAction::OpenHelp,
            ),
            item_btn(
                t(lang, "gui.menu.help.folder").into(),
                true,
                MenuAction::OpenInstallFolder,
            ),
            item_btn(
                t(lang, "gui.home.menu.help.licenses").into(),
                true,
                MenuAction::Licenses,
            ),
        ]),
    }
}

fn panel_file<'a>(lang: Lang, a: bool, busy: bool, popup: MenuPopup) -> Element<'a, Message> {
    panel(vec![
        item_btn(
            t(lang, "gui.menu.file.open").into(),
            !busy,
            MenuAction::Open,
        ),
        submenu_btn(
            t(lang, "gui.menu.file.recent").into(),
            popup == MenuPopup::FileRecent,
            MenuAction::OpenFileRecent,
        ),
        item_btn(t(lang, "gui.menu.file.close").into(), a, MenuAction::Close),
        sep(),
        item_btn(
            t(lang, "gui.menu.file.new").into(),
            !busy,
            MenuAction::Create,
        ),
        item_btn(
            t(lang, "gui.menu.file.new_window").into(),
            true,
            MenuAction::NewWindow,
        ),
        item_btn(
            t(lang, "gui.menu.file.new_window_admin").into(),
            true,
            MenuAction::NewWindowAdmin,
        ),
        sep(),
        item_btn(
            t(lang, "gui.menu.file.save_as").into(),
            a,
            MenuAction::SaveAs,
        ),
        item_btn(
            t(lang, "gui.menu.file.delete").into(),
            a,
            MenuAction::DeleteArchive,
        ),
        item_btn(
            t(lang, "gui.menu.file.delete_perm").into(),
            a,
            MenuAction::DeleteArchivePermanent,
        ),
        item_btn(
            t(lang, "gui.menu.file.open_folder").into(),
            a,
            MenuAction::OpenArchiveFolder,
        ),
        sep(),
        item_btn(t(lang, "gui.menu.file.test").into(), a, MenuAction::Test),
        submenu_btn(
            t(lang, "gui.menu.file.extract").into(),
            popup == MenuPopup::FileExtract,
            MenuAction::OpenFileExtract,
        ),
        sep(),
        item_btn(t(lang, "gui.menu.file.exit").into(), true, MenuAction::Exit),
    ])
}

fn panel_recent<'a>(ctx: &MenuCtx<'_>, busy: bool) -> Element<'a, Message> {
    let lang = ctx.lang;
    let mut items = Vec::new();
    if ctx.prefs.recent.is_empty() {
        items.push(item_btn(
            t(lang, "gui.menu.file.recent_empty").into(),
            false,
            MenuAction::ClearRecent,
        ));
    } else {
        for (i, path) in ctx.prefs.recent.iter().enumerate() {
            let name = std::path::Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            items.push(item_btn(name, !busy, MenuAction::Recent(i)));
        }
        items.push(sep());
        items.push(item_btn(
            t(lang, "gui.menu.file.recent_clear").into(),
            true,
            MenuAction::ClearRecent,
        ));
    }
    panel(items)
}

fn panel_extract<'a>(lang: Lang, a: bool) -> Element<'a, Message> {
    panel(vec![
        item_btn(t(lang, "gui.extract").into(), a, MenuAction::ExtractTo),
        item_btn(
            t(lang, "gui.extract_here").into(),
            a,
            MenuAction::ExtractHere,
        ),
        item_btn(
            t(lang, "gui.extract_smart").into(),
            a,
            MenuAction::ExtractSmart,
        ),
    ])
}

fn panel_edit<'a>(lang: Lang, a: bool, e: bool, ctx: &MenuCtx<'_>) -> Element<'a, Message> {
    panel(vec![
        item_btn(
            t(lang, "gui.menu.edit.rename").into(),
            e && ctx.selection_count == 1,
            MenuAction::Rename,
        ),
        item_btn(t(lang, "gui.menu.edit.add").into(), a, MenuAction::Add),
        item_btn(
            t(lang, "gui.menu.edit.delete").into(),
            e,
            MenuAction::DeleteEntries,
        ),
        item_btn(
            t(lang, "gui.menu.edit.comment").into(),
            a && ctx.is_zip_comment,
            MenuAction::EditComment,
        ),
        sep(),
        item_btn(
            t(lang, "gui.menu.edit.copy_files").into(),
            e,
            MenuAction::CopyFilesClipboard,
        ),
        item_btn(
            t(lang, "gui.menu.edit.paste_files").into(),
            a,
            MenuAction::PasteFilesClipboard,
        ),
        item_btn(
            t(lang, "gui.menu.edit.copy_paths").into(),
            e,
            MenuAction::CopyFullPaths,
        ),
    ])
}

fn panel_settings<'a>(lang: Lang, popup: MenuPopup) -> Element<'a, Message> {
    panel(vec![
        item_btn(
            t(lang, "gui.menu.settings.page").into(),
            true,
            MenuAction::ShowSettings,
        ),
        submenu_btn(
            t(lang, "gui.home.menu.settings.lang").into(),
            popup == MenuPopup::SettingsLang,
            MenuAction::OpenSettingsLang,
        ),
        item_btn(
            t(lang, "gui.menu.settings.passman").into(),
            true,
            MenuAction::PasswordManager,
        ),
        item_btn(
            t(lang, "gui.home.menu.settings.assoc").into(),
            true,
            MenuAction::Associations,
        ),
    ])
}

fn panel_view<'a>(
    lang: Lang,
    a: bool,
    busy: bool,
    ctx: &MenuCtx<'_>,
    popup: MenuPopup,
) -> Element<'a, Message> {
    panel(vec![
        item_btn(
            t(lang, "gui.menu.view.start").into(),
            !busy,
            MenuAction::StartScreen,
        ),
        item_btn(
            check_label(t(lang, "gui.menu.view.tree"), ctx.prefs.show_tree),
            a,
            MenuAction::ToggleTree,
        ),
        item_btn(
            check_label(t(lang, "gui.menu.view.comment"), ctx.prefs.show_comment),
            a && ctx.is_zip_comment,
            MenuAction::ToggleComment,
        ),
        item_btn(
            check_label(t(lang, "gui.menu.view.status"), ctx.prefs.show_status_bar),
            true,
            MenuAction::ToggleStatusBar,
        ),
        submenu_btn(
            t(lang, "gui.menu.view.toolbar").into(),
            popup == MenuPopup::ViewToolbar,
            MenuAction::OpenViewToolbar,
        ),
        item_btn(
            t(lang, "gui.menu.view.toolbar_pick").into(),
            true,
            MenuAction::ToolbarCustomize,
        ),
        submenu_btn(
            t(lang, "gui.menu.view.list").into(),
            popup == MenuPopup::ViewList,
            MenuAction::OpenViewList,
        ),
        submenu_btn(
            t(lang, "gui.menu.view.column").into(),
            popup == MenuPopup::ViewColumn,
            MenuAction::OpenViewColumn,
        ),
        item_btn(
            check_label(t(lang, "gui.menu.view.top"), ctx.prefs.always_on_top),
            true,
            MenuAction::ToggleAlwaysOnTop,
        ),
        item_btn(
            t(lang, "gui.menu.view.color").into(),
            true,
            MenuAction::ColorSettings,
        ),
    ])
}

fn panel_view_toolbar<'a>(lang: Lang, ctx: &MenuCtx<'_>) -> Element<'a, Message> {
    panel(vec![
        item_btn(
            check_label(
                t(lang, "gui.menu.view.toolbar_large"),
                matches!(ctx.prefs.toolbar, crate::prefs::ToolbarSize::Large),
            ),
            true,
            MenuAction::ToolbarLarge,
        ),
        item_btn(
            check_label(
                t(lang, "gui.menu.view.toolbar_small"),
                matches!(ctx.prefs.toolbar, crate::prefs::ToolbarSize::Small),
            ),
            true,
            MenuAction::ToolbarSmall,
        ),
        item_btn(
            check_label(
                t(lang, "gui.menu.view.toolbar_hide"),
                matches!(ctx.prefs.toolbar, crate::prefs::ToolbarSize::Hidden),
            ),
            true,
            MenuAction::ToolbarHide,
        ),
    ])
}

fn panel_view_list<'a>(lang: Lang, ctx: &MenuCtx<'_>) -> Element<'a, Message> {
    panel(vec![
        item_btn(
            check_label(
                t(lang, "gui.menu.view.list_icons"),
                matches!(ctx.prefs.list_view, crate::prefs::ListViewMode::Icons),
            ),
            true,
            MenuAction::ListIcons,
        ),
        item_btn(
            check_label(
                t(lang, "gui.menu.view.list_list"),
                matches!(ctx.prefs.list_view, crate::prefs::ListViewMode::List),
            ),
            true,
            MenuAction::ListList,
        ),
        item_btn(
            check_label(
                t(lang, "gui.menu.view.list_details"),
                matches!(ctx.prefs.list_view, crate::prefs::ListViewMode::Details),
            ),
            true,
            MenuAction::ListDetails,
        ),
        item_btn(
            check_label(
                t(lang, "gui.menu.view.list_large"),
                matches!(ctx.prefs.list_view, crate::prefs::ListViewMode::LargeIcons),
            ),
            true,
            MenuAction::ListLargeIcons,
        ),
    ])
}

fn panel_view_column<'a>(lang: Lang, ctx: &MenuCtx<'_>) -> Element<'a, Message> {
    panel(vec![
        item_btn(
            check_label(t(lang, "gui.col.size"), ctx.prefs.columns.size),
            true,
            MenuAction::ToggleColSize,
        ),
        item_btn(
            check_label(t(lang, "gui.col.compressed"), ctx.prefs.columns.compressed),
            true,
            MenuAction::ToggleColCompressed,
        ),
        item_btn(
            check_label(t(lang, "gui.col.method"), ctx.prefs.columns.method),
            true,
            MenuAction::ToggleColMethod,
        ),
        item_btn(
            check_label(t(lang, "gui.col.modified"), ctx.prefs.columns.modified),
            true,
            MenuAction::ToggleColModified,
        ),
        item_btn(
            check_label(t(lang, "gui.col.kind"), ctx.prefs.columns.kind),
            true,
            MenuAction::ToggleColKind,
        ),
    ])
}

fn check_label(label: &str, on: bool) -> String {
    // ASCII markers — YaHei often fails to draw ✓ / ▸ consistently.
    if on {
        format!("* {label}")
    } else {
        format!("  {label}")
    }
}
