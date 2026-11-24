//! Settings dialog layout: sidebar navigation plus grouped, token-styled pages.

use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_input, Column,
    Space,
};
use iced::{Alignment, Element, Length};
use oxizip_engine::FormatId;
use oxizip_i18n::{t, Lang};

use super::{AccentLabel, FormatLabel, LevelLabel, MethodLabel, SettingsMsg, SettingsTab};
use crate::prefs::{ColorAccent, DoubleClickAction, Prefs, ShellMenuPrefs};
use crate::setup;
use crate::theme;

const LABEL_W: f32 = 150.0;
const PICK_W: f32 = 160.0;

pub struct SettingsView<'a> {
    pub lang: Lang,
    pub tab: SettingsTab,
    pub prefs: &'a Prefs,
    pub setup_exts: &'a std::collections::BTreeMap<&'static str, bool>,
    pub status: &'a str,
}

type El<'a> = Element<'a, SettingsMsg>;

fn page_title<'a>(title: &'a str) -> El<'a> {
    text(title).size(16).style(theme::text_primary).into()
}

fn note<'a>(s: &'a str) -> El<'a> {
    text(s).size(11).style(theme::text_secondary).into()
}

/// Titled group: a quiet caption above an inset panel whose rows are divided by hairlines.
fn section<'a>(title: &'a str, rows: Vec<El<'a>>) -> El<'a> {
    let mut body = Column::new();
    let last = rows.len().saturating_sub(1);
    for (i, r) in rows.into_iter().enumerate() {
        body = body.push(r);
        if i != last {
            body = body.push(
                container(Space::with_height(1))
                    .width(Length::Fill)
                    .style(theme::divider),
            );
        }
    }
    column![
        text(title).size(11).style(theme::text_secondary),
        container(body)
            .padding([2, 12])
            .width(Length::Fill)
            .style(theme::panel),
    ]
    .spacing(6)
    .into()
}

fn toggle<'a>(label: &'a str, on: bool, msg: impl Fn(bool) -> SettingsMsg + 'a) -> El<'a> {
    container(
        checkbox(label, on)
            .on_toggle(msg)
            .size(16)
            .text_size(12)
            .spacing(10)
            .width(Length::Fill),
    )
    .padding([8, 0])
    .into()
}

fn field<'a>(label: &'a str, control: impl Into<El<'a>>) -> El<'a> {
    row![
        text(label)
            .size(12)
            .style(theme::text_primary)
            .width(Length::Fixed(LABEL_W)),
        Space::with_width(Length::Fill),
        control.into(),
    ]
    .align_y(Alignment::Center)
    .padding([6, 0])
    .into()
}

fn pad<'a>(e: El<'a>) -> El<'a> {
    container(e).padding([8, 0]).width(Length::Fill).into()
}

fn small_btn<'a>(label: &'a str, msg: SettingsMsg) -> El<'a> {
    button(text(label).size(12))
        .padding([6, 12])
        .style(theme::btn_secondary)
        .on_press(msg)
        .into()
}

fn page<'a>(title: &'a str, intro: Option<&'a str>, sections: Vec<El<'a>>) -> El<'a> {
    let mut col = Column::new().spacing(16).push(page_title(title));
    if let Some(s) = intro {
        col = col.push(note(s));
    }
    for s in sections {
        col = col.push(s);
    }
    col.into()
}

pub fn view_settings(state: SettingsView<'_>) -> Element<'_, SettingsMsg> {
    let lang = state.lang;

    let nav = SettingsTab::ALL.iter().fold(
        Column::new().spacing(2).width(Length::Fixed(124.0)),
        |col, tab| {
            let selected = *tab == state.tab;
            col.push(
                button(text(t(lang, tab.i18n_key())).size(12))
                    .width(Length::Fill)
                    .padding([8, 12])
                    .style(move |th, st| {
                        let mut s = theme::btn_ghost(th, st);
                        if selected {
                            let tk = theme::tokens(th);
                            s.background = Some(iced::Background::Color(tk.accent_soft));
                            s.text_color = tk.accent;
                        }
                        s
                    })
                    .on_press(SettingsMsg::Tab(*tab)),
            )
        },
    );

    let content: El<'_> = match state.tab {
        SettingsTab::General => page_general(lang, state.prefs),
        SettingsTab::Assoc => page_assoc(lang, state.setup_exts),
        SettingsTab::Shell => page_shell(lang, &state.prefs.shell),
        SettingsTab::Extract => page_extract(lang, state.prefs),
        SettingsTab::Archive => page_archive(lang, state.prefs),
        SettingsTab::Viewer => page_viewer(lang, state.prefs),
        SettingsTab::Advanced => page_advanced(lang, state.prefs),
    };

    let footer = row![
        button(text(t(lang, "gui.settings.reset")).size(12))
            .padding([8, 14])
            .style(theme::btn_secondary)
            .on_press(SettingsMsg::Reset),
        text(if state.status.is_empty() {
            " "
        } else {
            state.status
        })
        .size(11)
        .style(theme::text_secondary)
        .width(Length::Fill)
        .align_x(Alignment::Center),
        button(text(t(lang, "gui.settings.ok")).size(13))
            .padding([8, 28])
            .style(theme::btn_primary)
            .on_press(SettingsMsg::Ok),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let body = column![
        row![
            container(scrollable(nav).height(Length::Fill)).padding([4, 0]),
            container(scrollable(container(content).padding(iced::Padding {
                top: 4.0,
                right: 14.0,
                bottom: 4.0,
                left: 4.0,
            })))
            .width(Length::Fill)
            .height(Length::Fill),
        ]
        .spacing(12)
        .height(Length::Fill),
        footer,
    ]
    .spacing(10)
    .padding(14)
    .width(Length::Fill)
    .height(Length::Fill);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn page_general(lang: Lang, prefs: &Prefs) -> El<'_> {
    page(
        t(lang, "gui.settings.general.heading"),
        None,
        vec![
            section(
                t(lang, "gui.settings.tab.general"),
                vec![
                    toggle(
                        t(lang, "gui.settings.general.esc"),
                        prefs.esc_to_exit,
                        SettingsMsg::EscToExit,
                    ),
                    toggle(
                        t(lang, "gui.settings.general.test_modify"),
                        prefs.test_after_modify,
                        SettingsMsg::TestAfterModify,
                    ),
                    toggle(
                        t(lang, "gui.menu.view.top"),
                        prefs.always_on_top,
                        SettingsMsg::AlwaysOnTop,
                    ),
                    toggle(
                        t(lang, "gui.menu.view.status"),
                        prefs.show_status_bar,
                        SettingsMsg::ShowStatusBar,
                    ),
                ],
            ),
            section(
                t(lang, "gui.settings.tab.viewer"),
                vec![
                    toggle(
                        t(lang, "gui.settings.general.dotdot"),
                        prefs.show_dotdot,
                        SettingsMsg::ShowDotDot,
                    ),
                    toggle(
                        t(lang, "gui.settings.general.grid"),
                        prefs.show_gridlines,
                        SettingsMsg::ShowGrid,
                    ),
                    toggle(
                        t(lang, "gui.settings.general.full_row"),
                        prefs.full_row_select,
                        SettingsMsg::FullRowSelect,
                    ),
                    toggle(
                        t(lang, "gui.settings.general.single_click"),
                        prefs.single_click_open,
                        SettingsMsg::SingleClickOpen,
                    ),
                ],
            ),
        ],
    )
}

fn page_assoc<'a>(
    lang: Lang,
    setup_exts: &'a std::collections::BTreeMap<&'static str, bool>,
) -> El<'a> {
    const PER_ROW: usize = 4;
    let mut sections: Vec<El<'a>> = Vec::new();
    for (group, exts) in setup::assoc_catalog() {
        let mut rows: Vec<El<'a>> = Vec::new();
        for chunk in exts.chunks(PER_ROW) {
            let mut line = row![].spacing(8);
            for ext in chunk {
                let e = *ext;
                let checked = setup_exts.get(e).copied().unwrap_or(false);
                line = line.push(
                    checkbox(format!(".{e}"), checked)
                        .on_toggle(move |v| SettingsMsg::ExtToggled(e, v))
                        .size(16)
                        .text_size(12)
                        .spacing(8)
                        .width(Length::FillPortion(1)),
                );
            }
            for _ in chunk.len()..PER_ROW {
                line = line.push(Space::with_width(Length::FillPortion(1)));
            }
            rows.push(pad(line.into()));
        }
        sections.push(section(t(lang, group.i18n_key()), rows));
    }
    let all_on = !setup_exts.is_empty() && setup_exts.values().all(|v| *v);
    let toggle_label = if all_on {
        t(lang, "gui.setup.clear_all")
    } else {
        t(lang, "gui.setup.select_all")
    };
    sections.push(
        row![
            small_btn(toggle_label, SettingsMsg::ToggleSelectAllAssoc),
            small_btn(
                t(lang, "gui.settings.assoc.all"),
                SettingsMsg::SelectAllAssocDefaults
            ),
        ]
        .spacing(8)
        .into(),
    );
    page(
        t(lang, "gui.settings.assoc.heading"),
        Some(t(lang, "gui.settings.assoc.intro_ok")),
        sections,
    )
}

fn page_shell(lang: Lang, shell: &ShellMenuPrefs) -> El<'_> {
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
    let mut dbl_rows: Vec<El<'_>> = dbl
        .into_iter()
        .map(|(action, key)| {
            let on = shell.double_click == action;
            toggle(t(lang, key), on, move |v| {
                if v {
                    SettingsMsg::DoubleClick(action)
                } else {
                    SettingsMsg::DoubleClick(DoubleClickAction::Open)
                }
            })
        })
        .collect();
    dbl_rows.push(toggle(
        t(lang, "gui.settings.shell.dbl.delete"),
        shell.delete_after_dbl_extract,
        SettingsMsg::DeleteAfterDblExtract,
    ));

    let sh = |key: &'static str, on: bool, f: fn(bool) -> SettingsMsg| -> El<'_> {
        toggle(t(lang, key), on, f)
    };

    page(
        t(lang, "gui.settings.shell.heading"),
        Some(t(lang, "gui.settings.shell.note_ok")),
        vec![
            section(t(lang, "gui.settings.shell.dbl"), dbl_rows),
            section(
                t(lang, "gui.settings.shell.extract"),
                vec![
                    sh(
                        "gui.settings.shell.extract_here",
                        shell.extract_here,
                        SettingsMsg::ShellExtractHere,
                    ),
                    sh(
                        "gui.settings.shell.extract_smart",
                        shell.extract_smart,
                        SettingsMsg::ShellExtractSmart,
                    ),
                    sh(
                        "gui.settings.shell.extract_folder",
                        shell.extract_to_folder,
                        SettingsMsg::ShellExtractToFolder,
                    ),
                    sh(
                        "gui.settings.shell.extract_here_del",
                        shell.extract_here_del,
                        SettingsMsg::ShellExtractHereDel,
                    ),
                    sh(
                        "gui.settings.shell.extract_smart_del",
                        shell.extract_smart_del,
                        SettingsMsg::ShellExtractSmartDel,
                    ),
                    sh(
                        "gui.settings.shell.extract_folder_del",
                        shell.extract_to_folder_del,
                        SettingsMsg::ShellExtractToFolderDel,
                    ),
                    sh(
                        "gui.settings.shell.extract_dialog",
                        shell.extract_dialog,
                        SettingsMsg::ShellExtractDialog,
                    ),
                    sh(
                        "gui.settings.shell.open_with",
                        shell.open_with,
                        SettingsMsg::ShellOpenWith,
                    ),
                    sh(
                        "gui.settings.shell.test",
                        shell.test,
                        SettingsMsg::ShellTest,
                    ),
                    sh(
                        "gui.settings.shell.checksum",
                        shell.checksum,
                        SettingsMsg::ShellChecksum,
                    ),
                ],
            ),
            section(
                t(lang, "gui.settings.shell.compress"),
                vec![
                    sh(
                        "gui.settings.shell.zip",
                        shell.compress_zip,
                        SettingsMsg::ShellCompressZip,
                    ),
                    sh(
                        "gui.settings.shell.sevenz",
                        shell.compress_7z,
                        SettingsMsg::ShellCompress7z,
                    ),
                    sh(
                        "gui.settings.shell.zipx",
                        shell.compress_zipx,
                        SettingsMsg::ShellCompressZipx,
                    ),
                    sh(
                        "gui.settings.shell.tar",
                        shell.compress_tar,
                        SettingsMsg::ShellCompressTar,
                    ),
                    sh(
                        "gui.settings.shell.tgz",
                        shell.compress_tgz,
                        SettingsMsg::ShellCompressTgz,
                    ),
                    sh(
                        "gui.settings.shell.gz",
                        shell.compress_gz,
                        SettingsMsg::ShellCompressGz,
                    ),
                    sh(
                        "gui.settings.shell.xz",
                        shell.compress_xz,
                        SettingsMsg::ShellCompressXz,
                    ),
                    sh(
                        "gui.settings.shell.zpaq",
                        shell.compress_zpaq,
                        SettingsMsg::ShellCompressZpaq,
                    ),
                    sh(
                        "gui.settings.shell.each_zip",
                        shell.compress_each_zip,
                        SettingsMsg::ShellCompressEachZip,
                    ),
                    sh(
                        "gui.settings.shell.each_7z",
                        shell.compress_each_7z,
                        SettingsMsg::ShellCompressEach7z,
                    ),
                ],
            ),
            section(
                t(lang, "gui.settings.shell.other"),
                vec![
                    sh(
                        "gui.settings.shell.cascade",
                        shell.cascaded,
                        SettingsMsg::ShellCascaded,
                    ),
                    sh(
                        "gui.settings.shell.disable",
                        shell.disabled,
                        SettingsMsg::ShellDisabled,
                    ),
                    sh(
                        "gui.settings.shell.hide_icon",
                        shell.hide_icon,
                        SettingsMsg::ShellHideIcon,
                    ),
                ],
            ),
        ],
    )
}

fn page_extract(lang: Lang, prefs: &Prefs) -> El<'_> {
    page(
        t(lang, "gui.settings.extract.heading"),
        None,
        vec![section(
            t(lang, "gui.settings.tab.extract"),
            vec![
                toggle(
                    t(lang, "gui.settings.open_dest"),
                    prefs.open_dest_after_extract,
                    SettingsMsg::OpenDest,
                ),
                toggle(
                    t(lang, "gui.settings.extract.keep_job"),
                    prefs.job_keep_open_after_extract,
                    SettingsMsg::JobKeepOpenExtract,
                ),
                toggle(
                    t(lang, "gui.settings.extract.enter_root"),
                    prefs.enter_single_root_folder,
                    SettingsMsg::EnterSingleRoot,
                ),
                toggle(
                    t(lang, "gui.settings.extract.smart_single"),
                    prefs.smart_single_file_folder,
                    SettingsMsg::SmartSingleFileFolder,
                ),
            ],
        )],
    )
}

fn page_archive(lang: Lang, prefs: &Prefs) -> El<'_> {
    let formats: Vec<FormatLabel> = FormatId::create_choices()
        .iter()
        .copied()
        .map(FormatLabel)
        .collect();
    let methods: Vec<MethodLabel> = MethodLabel::all().to_vec();
    let current_method = MethodLabel::from_prefs(prefs);
    let levels: Vec<LevelLabel> = LevelLabel::all().to_vec();

    let defaults = section(
        t(lang, "gui.settings.tab.archive"),
        vec![
            field(
                t(lang, "gui.settings.default_format"),
                pick_list(formats, Some(FormatLabel(prefs.default_format_id())), |f| {
                    SettingsMsg::DefaultFormat(f.0)
                })
                .width(PICK_W),
            ),
            field(
                t(lang, "gui.settings.default_method"),
                pick_list(methods, Some(current_method), |m| match m.to_method() {
                    None => SettingsMsg::DefaultMethodAuto,
                    Some(id) => SettingsMsg::DefaultMethod(id),
                })
                .width(PICK_W),
            ),
            field(
                t(lang, "gui.settings.archive.level"),
                pick_list(levels, Some(LevelLabel(prefs.compression_level)), |l| {
                    SettingsMsg::CompressionLevel(l.0)
                })
                .width(PICK_W),
            ),
        ],
    );

    let options = section(
        t(lang, "gui.settings.archive.heading"),
        vec![
            toggle(
                t(lang, "gui.settings.archive.utf8"),
                prefs.utf8_zip_names,
                SettingsMsg::Utf8Zip,
            ),
            toggle(
                t(lang, "gui.settings.archive.thumbs"),
                prefs.exclude_thumbs_db,
                SettingsMsg::ExcludeThumbsDb,
            ),
            toggle(
                t(lang, "gui.settings.archive.test_after"),
                prefs.test_after_compress,
                SettingsMsg::TestAfterCompress,
            ),
            toggle(
                t(lang, "gui.settings.archive.no_root"),
                prefs.no_root_for_single_folder,
                SettingsMsg::NoRootSingleFolder,
            ),
            toggle(
                t(lang, "gui.settings.archive.keep_job"),
                prefs.job_keep_open_after_create,
                SettingsMsg::JobKeepOpenCreate,
            ),
            toggle(
                t(lang, "gui.settings.archive.use_last"),
                prefs.new_archive_use_last,
                SettingsMsg::NewArchiveUseLast,
            ),
        ],
    );

    let paths = section(
        t(lang, "gui.settings.archive.default_folder"),
        vec![
            pad(row![
                text_input(
                    t(lang, "gui.settings.archive.default_folder_ph"),
                    &prefs.new_archive_folder
                )
                .size(12)
                .on_input(SettingsMsg::NewArchiveFolderChanged)
                .width(Length::Fill),
                small_btn("…", SettingsMsg::BrowseNewArchiveFolder),
            ]
            .spacing(8)
            .into()),
            pad(column![
                text(t(lang, "gui.settings.archive.exclude"))
                    .size(12)
                    .style(theme::text_primary),
                text_input(
                    t(lang, "gui.settings.archive.exclude_ph"),
                    &prefs.exclude_filter
                )
                .size(12)
                .on_input(SettingsMsg::ExcludeFilterChanged),
            ]
            .spacing(6)
            .into()),
        ],
    );

    page(
        t(lang, "gui.settings.archive.heading"),
        None,
        vec![defaults, options, paths],
    )
}

fn page_viewer(lang: Lang, prefs: &Prefs) -> El<'_> {
    let cols = &prefs.columns;
    let accents = [
        ColorAccent::BrandBlue,
        ColorAccent::DeepNavy,
        ColorAccent::Teal,
    ]
    .map(AccentLabel)
    .to_vec();

    page(
        t(lang, "gui.settings.viewer.heading"),
        Some(t(lang, "gui.settings.viewer.preview_note")),
        vec![
            section(
                t(lang, "gui.settings.tab.viewer"),
                vec![
                    toggle(
                        t(lang, "gui.menu.view.tree"),
                        prefs.show_tree,
                        SettingsMsg::ShowTree,
                    ),
                    toggle(
                        t(lang, "gui.menu.view.comment"),
                        prefs.show_comment,
                        SettingsMsg::ShowComment,
                    ),
                    field(
                        t(lang, "gui.settings.viewer.accent"),
                        pick_list(accents, Some(AccentLabel(prefs.color_accent)), |a| {
                            SettingsMsg::ColorAccent(a.0)
                        })
                        .width(PICK_W),
                    ),
                ],
            ),
            section(
                t(lang, "gui.settings.viewer.columns"),
                vec![
                    toggle(
                        t(lang, "gui.settings.viewer.col.size"),
                        cols.size,
                        SettingsMsg::ColSize,
                    ),
                    toggle(
                        t(lang, "gui.settings.viewer.col.compressed"),
                        cols.compressed,
                        SettingsMsg::ColCompressed,
                    ),
                    toggle(
                        t(lang, "gui.settings.viewer.col.method"),
                        cols.method,
                        SettingsMsg::ColMethod,
                    ),
                    toggle(
                        t(lang, "gui.settings.viewer.col.modified"),
                        cols.modified,
                        SettingsMsg::ColModified,
                    ),
                    toggle(
                        t(lang, "gui.settings.viewer.col.kind"),
                        cols.kind,
                        SettingsMsg::ColKind,
                    ),
                ],
            ),
        ],
    )
}

fn page_advanced(lang: Lang, prefs: &Prefs) -> El<'_> {
    let behavior = section(
        t(lang, "gui.settings.advanced.heading"),
        vec![
            toggle(
                t(lang, "gui.settings.security.zone"),
                prefs.copy_zone_identifier,
                SettingsMsg::CopyZoneId,
            ),
            toggle(
                t(lang, "gui.settings.advanced.recent"),
                prefs.show_recent,
                SettingsMsg::ShowRecent,
            ),
            toggle(
                t(lang, "gui.settings.advanced.long_path"),
                prefs.use_long_paths,
                SettingsMsg::UseLongPaths,
            ),
            toggle(
                t(lang, "gui.settings.advanced.del_cancel"),
                prefs.delete_on_extract_cancel,
                SettingsMsg::DeleteOnCancel,
            ),
            toggle(
                t(lang, "gui.settings.advanced.title_path"),
                prefs.title_full_path,
                SettingsMsg::TitleFullPath,
            ),
            toggle(
                t(lang, "gui.settings.advanced.expand_tree"),
                prefs.auto_expand_tree,
                SettingsMsg::AutoExpandTree,
            ),
            toggle(
                t(lang, "gui.settings.advanced.trash"),
                prefs.delete_to_trash,
                SettingsMsg::DeleteToTrash,
            ),
            toggle(
                t(lang, "gui.settings.advanced.start_screen"),
                prefs.show_start_screen,
                SettingsMsg::ShowStartScreen,
            ),
        ],
    );

    let temp = section(
        t(lang, "gui.settings.advanced.temp"),
        vec![
            toggle(
                t(lang, "gui.settings.advanced.temp_asap"),
                prefs.delete_temp_asap,
                SettingsMsg::DeleteTempAsap,
            ),
            pad(row![
                text_input(t(lang, "gui.settings.advanced.temp_ph"), &prefs.temp_folder)
                    .size(12)
                    .on_input(SettingsMsg::TempFolderChanged)
                    .width(Length::Fill),
                small_btn("…", SettingsMsg::BrowseTempFolder),
            ]
            .spacing(8)
            .into()),
        ],
    );

    let actions: El<'_> = row![
        small_btn(
            t(lang, "gui.settings.advanced.export"),
            SettingsMsg::ExportPrefs
        ),
        small_btn(
            t(lang, "gui.settings.advanced.import"),
            SettingsMsg::ImportPrefs
        ),
        small_btn(
            t(lang, "gui.settings.advanced.clear_recent"),
            SettingsMsg::ClearRecent
        ),
    ]
    .spacing(8)
    .into();

    page(
        t(lang, "gui.settings.advanced.heading"),
        None,
        vec![behavior, temp, actions],
    )
}
