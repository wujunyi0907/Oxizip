//! Persistent GUI preferences (recent files, language, view options).

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
}

impl Default for ColumnVisibility {
    fn default() -> Self {
        Self {
            size: true,
            compressed: false,
            method: true,
            modified: false,
        }
    }
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

    pub fn lang_enum(&self) -> Lang {
        if self.lang.eq_ignore_ascii_case("en") {
            Lang::En
        } else {
            Lang::ZhCn
        }
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = match lang {
            Lang::ZhCn => "zh-CN".into(),
            Lang::En => "en".into(),
        };
    }

    pub fn push_recent(&mut self, path: &Path) {
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
}

fn prefs_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("Oxizip").join("prefs.json");
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("oxizip")
                .join("prefs.json");
        }
    }
    PathBuf::from("oxizip-prefs.json")
}
