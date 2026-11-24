//! Multi-tab Settings dialog (left-nav layout, clean-room).
//! Opened as a dedicated `--settings` window; edits stay in a draft until OK.

use oxizip_engine::{FormatId, MethodId};
mod pages;
pub use pages::{view_settings, SettingsView};

use crate::prefs::{ColorAccent, DoubleClickAction, Prefs};

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
