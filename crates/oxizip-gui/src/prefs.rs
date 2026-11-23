//! Persistent GUI preferences (recent files, language, view options, settings tabs).

use std::fs;
use std::path::{Path, PathBuf};

use oxizip_engine::{FormatId, MethodId};
use oxizip_i18n::Lang;
use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarSize {
    #[default]
    Large,
    Small,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ListViewMode {
    Icons,
    List,
    #[default]
    Details,
    LargeIcons,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColorAccent {
    #[default]
    BrandBlue,
    DeepNavy,
    Teal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoubleClickAction {
    #[default]
    Open,
    ExtractSmart,
    ExtractToFolder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolbarButtons {
    pub close: bool,
    pub open: bool,
    pub extract: bool,
    pub extract_here: bool,
    pub extract_smart: bool,
    pub test: bool,
    pub add: bool,
    pub create: bool,
}

impl Default for ToolbarButtons {
    fn default() -> Self {
        Self {
            close: true,
            open: true,
            extract: true,
            extract_here: true,
            extract_smart: true,
            test: true,
            add: true,
            create: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnVisibility {
    pub size: bool,
    pub compressed: bool,
    pub method: bool,
    pub modified: bool,
    #[serde(default)]
    pub kind: bool,
}

impl Default for ColumnVisibility {
    fn default() -> Self {
        Self {
            size: true,
            compressed: false,
            method: true,
            modified: false,
            kind: false,
        }
    }
}

/// Explorer context-menu toggles (applied via `oxizip-cli shell install`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellMenuPrefs {
    #[serde(default)]
    pub double_click: DoubleClickAction,
    /// Delete the archive after a double-click extract action.
    #[serde(default)]
    pub delete_after_dbl_extract: bool,
    #[serde(default = "default_true")]
    pub extract_here: bool,
    #[serde(default = "default_true")]
    pub extract_smart: bool,
    #[serde(default = "default_true")]
    pub extract_to_folder: bool,
    #[serde(default)]
    pub extract_here_del: bool,
    #[serde(default)]
    pub extract_smart_del: bool,
    #[serde(default)]
    pub extract_to_folder_del: bool,
    #[serde(default = "default_true")]
    pub extract_dialog: bool,
    #[serde(default = "default_true")]
    pub open_with: bool,
    #[serde(default = "default_true")]
    pub test: bool,
    #[serde(default = "default_true")]
    pub checksum: bool,
    #[serde(default = "default_true")]
    pub compress_zip: bool,
    #[serde(default = "default_true")]
    pub compress_7z: bool,
    #[serde(default)]
    pub compress_zipx: bool,
    #[serde(default)]
    pub compress_tar: bool,
    #[serde(default)]
    pub compress_tgz: bool,
    #[serde(default)]
    pub compress_gz: bool,
    #[serde(default)]
    pub compress_xz: bool,
    #[serde(default)]
    pub compress_zpaq: bool,
    #[serde(default)]
    pub compress_each_zip: bool,
    #[serde(default)]
    pub compress_each_7z: bool,
    /// Prefer Win11 Sparse cascade when available (classic stays flat).
    #[serde(default = "default_true")]
    pub cascaded: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub hide_icon: bool,
}

impl Default for ShellMenuPrefs {
    fn default() -> Self {
        Self {
            double_click: DoubleClickAction::Open,
            delete_after_dbl_extract: false,
            extract_here: true,
            extract_smart: true,
            extract_to_folder: true,
            extract_here_del: false,
            extract_smart_del: false,
            extract_to_folder_del: false,
            extract_dialog: true,
            open_with: true,
            test: true,
            checksum: true,
            compress_zip: true,
            compress_7z: true,
            compress_zipx: false,
            compress_tar: false,
            compress_tgz: false,
            compress_gz: false,
            compress_xz: false,
            compress_zpaq: false,
            compress_each_zip: false,
            compress_each_7z: false,
            cascaded: true,
            disabled: false,
            hide_icon: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
    pub lang: String,
    pub recent: Vec<String>,
    pub show_tree: bool,
    pub show_comment: bool,
    pub show_status_bar: bool,
    pub toolbar: ToolbarSize,
    pub toolbar_buttons: ToolbarButtons,
    pub list_view: ListViewMode,
    pub columns: ColumnVisibility,
    pub always_on_top: bool,
    pub color_accent: ColorAccent,
    pub default_format: String,
    pub default_method: String,
    pub open_dest_after_extract: bool,
    pub passwords: Vec<String>,
    // --- settings tabs (serde defaults for older prefs.json) ---
    #[serde(default)]
    pub esc_to_exit: bool,
    #[serde(default)]
    pub show_dotdot: bool,
    #[serde(default)]
    pub show_gridlines: bool,
    #[serde(default = "default_true")]
    pub full_row_select: bool,
    #[serde(default)]
    pub single_click_open: bool,
    #[serde(default)]
    pub job_keep_open_after_extract: bool,
    #[serde(default)]
    pub job_keep_open_after_create: bool,
    #[serde(default = "default_true")]
    pub enter_single_root_folder: bool,
    #[serde(default)]
    pub smart_single_file_folder: bool,
    /// Reserved (engine has no parallel extract yet).
    #[serde(default = "default_true")]
    pub parallel_extract: bool,
    #[serde(default = "default_true")]
    pub utf8_zip_names: bool,
    #[serde(default = "default_true")]
    pub exclude_thumbs_db: bool,
    #[serde(default)]
    pub test_after_compress: bool,
    #[serde(default)]
    pub test_after_modify: bool,
    #[serde(default)]
    pub no_root_for_single_folder: bool,
    /// Compression level 0–9; `None` = format default.
    #[serde(default)]
    pub compression_level: Option<u8>,
    #[serde(default = "default_true")]
    pub new_archive_use_last: bool,
    #[serde(default)]
    pub new_archive_folder: String,
    /// Semicolon-separated patterns (`*.tmp;Thumbs.db`).
    #[serde(default)]
    pub exclude_filter: String,
    #[serde(default = "default_true")]
    pub copy_zone_identifier: bool,
    #[serde(default)]
    pub amsi_before_extract: bool,
    #[serde(default = "default_true")]
    pub show_recent: bool,
    #[serde(default = "default_true")]
    pub use_long_paths: bool,
    #[serde(default)]
    pub delete_on_extract_cancel: bool,
    #[serde(default)]
    pub title_full_path: bool,
    #[serde(default)]
    pub auto_expand_tree: bool,
    #[serde(default)]
    pub temp_folder: String,
    #[serde(default = "default_true")]
    pub delete_to_trash: bool,
    #[serde(default = "default_true")]
    pub show_start_screen: bool,
    #[serde(default = "default_true")]
    pub delete_temp_asap: bool,
    #[serde(default)]
    pub last_new_archive_dir: String,
    #[serde(default)]
    pub shell: ShellMenuPrefs,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            lang: "zh-CN".into(),
            recent: Vec::new(),
            show_tree: false,
            show_comment: false,
            show_status_bar: true,
            toolbar: ToolbarSize::default(),
            toolbar_buttons: ToolbarButtons::default(),
            list_view: ListViewMode::default(),
            columns: ColumnVisibility::default(),
            always_on_top: false,
            color_accent: ColorAccent::default(),
            default_format: "zip".into(),
            default_method: "auto".into(),
            open_dest_after_extract: false,
            passwords: Vec::new(),
            esc_to_exit: false,
            show_dotdot: false,
            show_gridlines: false,
            full_row_select: true,
            single_click_open: false,
            job_keep_open_after_extract: false,
            job_keep_open_after_create: false,
            enter_single_root_folder: true,
            smart_single_file_folder: false,
            parallel_extract: true,
            utf8_zip_names: true,
            exclude_thumbs_db: true,
            test_after_compress: false,
            test_after_modify: false,
            no_root_for_single_folder: false,
            compression_level: None,
            new_archive_use_last: true,
            new_archive_folder: String::new(),
            exclude_filter: String::new(),
            copy_zone_identifier: true,
            amsi_before_extract: false,
            show_recent: true,
            use_long_paths: true,
            delete_on_extract_cancel: false,
            title_full_path: false,
            auto_expand_tree: false,
            temp_folder: String::new(),
            delete_to_trash: true,
            show_start_screen: true,
            delete_temp_asap: true,
            last_new_archive_dir: String::new(),
            shell: ShellMenuPrefs::default(),
        }
    }
}

impl Prefs {
    pub fn load() -> Self {
        let path = prefs_path();
        let Ok(bytes) = fs::read(&path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = prefs_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = fs::write(path, bytes);
        }
    }

    pub fn export_to(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(path, bytes).map_err(|e| e.to_string())
    }

    pub fn import_from(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }

    /// Write shell menu prefs for CLI consumers (same directory as prefs.json).
    pub fn write_shell_menu_sidecar(&self) {
        let path = shell_menu_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.shell) {
            let _ = fs::write(path, bytes);
        }
    }

    pub fn lang_enum(&self) -> Lang {
        Lang::parse(&self.lang).unwrap_or_default()
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang.code().into();
    }

    pub fn push_recent(&mut self, path: &Path) {
        if !self.show_recent {
            return;
        }
        let s = path.to_string_lossy().into_owned();
        self.recent.retain(|p| p != &s);
        self.recent.insert(0, s);
        self.recent.truncate(MAX_RECENT);
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }

    pub fn default_format_id(&self) -> FormatId {
        FormatId::parse_create_name(&self.default_format).unwrap_or(FormatId::Zip)
    }

    pub fn set_default_format(&mut self, id: FormatId) {
        self.default_format = id.as_str().to_string();
    }

    pub fn default_method_id(&self) -> Option<MethodId> {
        if self.default_method == "auto" {
            None
        } else {
            MethodId::parse(&self.default_method)
        }
    }

    pub fn set_default_method_auto(&mut self) {
        self.default_method = "auto".into();
    }

    pub fn set_default_method(&mut self, id: MethodId) {
        self.default_method = id.as_str().to_string();
    }

    /// Directory suggested for “New Archive” save dialogs.
    pub fn new_archive_dir(&self) -> Option<PathBuf> {
        if self.new_archive_use_last {
            if !self.last_new_archive_dir.is_empty() {
                return Some(PathBuf::from(&self.last_new_archive_dir));
            }
        } else if !self.new_archive_folder.is_empty() {
            return Some(PathBuf::from(&self.new_archive_folder));
        }
        None
    }

    pub fn remember_new_archive_dir(&mut self, path: &Path) {
        if let Some(parent) = path.parent() {
            self.last_new_archive_dir = parent.to_string_lossy().into_owned();
        }
    }
}

pub fn prefs_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("Oxizip").join("prefs.json");
        }
    }
    #[cfg(not(windows))]
    {
        // Honor XDG_CONFIG_HOME so prefs / shell_menu.json match CLI + i18n.
        if let Ok(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("oxizip").join("prefs.json");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("oxizip")
                .join("prefs.json");
        }
    }
    PathBuf::from("oxizip-prefs.json")
}

pub fn shell_menu_path() -> PathBuf {
    prefs_path()
        .parent()
        .map(|p| p.join("shell_menu.json"))
        .unwrap_or_else(|| PathBuf::from("shell_menu.json"))
}
