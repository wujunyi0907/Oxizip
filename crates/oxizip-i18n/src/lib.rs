//! User-facing strings for Oxizip.
//!
//! Language packs are plain TOML files. At startup the catalog is built from:
//! 1. Embedded defaults (built from `locales/*.toml` in this crate)
//! 2. Install / portable folder: `<exe_dir>/locales/`
//! 3. System share (Linux): `/usr/share/oxizip/locales/`
//! 4. User overrides: `%APPDATA%\Oxizip\locales\` or `~/.config/oxizip/locales/`
//! 5. Optional `OXIZIP_LOCALES` directory (highest priority for that layer order —
//!    applied last so it wins)
//!
//! Later files with the same `code` replace earlier ones; new `code`s are added.
//! Restart the app after adding or editing language files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use oxizip_core::{CoreError, ErrorCode};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/locales_generated.rs"));
}

use generated::LOCALES;

/// Active UI / CLI language (identity = locale `code` from the TOML file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lang {
    code: &'static str,
}

impl Default for Lang {
    fn default() -> Self {
        Self::ZhCn
    }
}

struct RuntimeLocale {
    code: &'static str,
    cli: &'static str,
    native: &'static str,
    messages: BTreeMap<&'static str, &'static str>,
}

static CATALOG: OnceLock<Vec<RuntimeLocale>> = OnceLock::new();

/// Load / merge language packs. Safe to call multiple times; only the first call
/// builds the catalog (restart the process to pick up file changes).
pub fn init() {
    let _ = catalog();
}

fn catalog() -> &'static [RuntimeLocale] {
    CATALOG.get_or_init(build_catalog).as_slice()
}

fn build_catalog() -> Vec<RuntimeLocale> {
    // code → locale (BTreeMap keeps a stable menu order by code).
    let mut by_code: BTreeMap<&'static str, RuntimeLocale> = BTreeMap::new();

    for loc in LOCALES {
        insert_locale(
            &mut by_code,
            loc.code,
            loc.cli,
            loc.native,
            loc.messages.iter().copied(),
        );
    }

    for dir in locale_search_dirs() {
        merge_dir(&mut by_code, &dir);
    }

    by_code.into_values().collect()
}

fn insert_locale(
    map: &mut BTreeMap<&'static str, RuntimeLocale>,
    code: &'static str,
    cli: &'static str,
    native: &'static str,
    messages: impl IntoIterator<Item = (&'static str, &'static str)>,
) {
    let mut msgs = BTreeMap::new();
    for (k, v) in messages {
        msgs.insert(k, v);
    }
    map.insert(
        code,
        RuntimeLocale {
            code,
            cli,
            native,
            messages: msgs,
        },
    );
}

fn merge_dir(map: &mut BTreeMap<&'static str, RuntimeLocale>, dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        })
        .collect();
    files.sort();
    for path in files {
        if let Some(parsed) = parse_locale_file(&path) {
            insert_locale(
                map,
                parsed.code,
                parsed.cli,
                parsed.native,
                parsed.messages,
            );
        }
    }
}

struct ParsedLocale {
    code: &'static str,
    cli: &'static str,
    native: &'static str,
    messages: Vec<(&'static str, &'static str)>,
}

fn parse_locale_file(path: &Path) -> Option<ParsedLocale> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: toml::Value = text.parse().ok()?;
    let table = value.as_table()?;
    let code = intern(table.get("code")?.as_str()?.to_string());
    let cli = table
        .get("cli")
        .and_then(|v| v.as_str())
        .map(|s| intern(s.to_string()))
        .unwrap_or(code);
    let native = intern(table.get("native")?.as_str()?.to_string());
    let messages_table = table.get("messages")?.as_table()?;
    let mut messages = Vec::with_capacity(messages_table.len());
    for (k, v) in messages_table {
        let s = v.as_str()?;
        messages.push((intern(k.clone()), intern(s.to_string())));
    }
    if messages.is_empty() {
        return None;
    }
    Some(ParsedLocale {
        code,
        cli,
        native,
        messages,
    })
}

fn intern(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Directories scanned after embedded defaults (later wins on same `code`).
fn locale_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // Dev tree: crates/oxizip-i18n/locales (debug builds only).
    #[cfg(debug_assertions)]
    {
        dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("locales"));
    }

    if let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        dirs.push(exe.join("locales"));
        // AppImage / FHS: <prefix>/bin/oxizip → <prefix>/share/oxizip/locales
        if let Some(prefix) = exe.parent() {
            dirs.push(prefix.join("share").join("oxizip").join("locales"));
        }
    }

    #[cfg(unix)]
    {
        dirs.push(PathBuf::from("/usr/share/oxizip/locales"));
        dirs.push(PathBuf::from("/usr/local/share/oxizip/locales"));
    }

    if let Some(user) = user_locales_dir() {
        dirs.push(user);
    }

    if let Ok(extra) = std::env::var("OXIZIP_LOCALES") {
        let p = PathBuf::from(extra);
        if p.is_dir() {
            dirs.push(p);
        }
    }

    dirs
}

/// Ensure the user `locales` folder exists and has a short README so end users
/// know where to drop new `*.toml` packs without editing Program Files.
pub fn ensure_user_locales_scaffold() {
    let Some(dir) = user_locales_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let readme = dir.join("README.txt");
    if readme.is_file() {
        return;
    }
    if let Some(src) = install_locales_dir().map(|d| d.join("README.txt")) {
        if src.is_file() {
            let _ = std::fs::copy(src, &readme);
            return;
        }
    }
    let _ = std::fs::write(
        &readme,
        "Put additional Oxizip language *.toml files in this folder, then restart Oxizip.\n\
         Copy en.toml / zh-CN.toml from the install locales folder as a template.\n",
    );
}

/// Writable user locale folder (created on demand by callers if desired).
pub fn user_locales_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("Oxizip").join("locales"))
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME")?;
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".config"));
        Some(base.join("oxizip").join("locales"))
    }
}

/// Install / portable locales next to the GUI/CLI binary.
pub fn install_locales_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("locales"))
}

impl Lang {
    /// Built-in aliases for call sites that still name Chinese / English directly.
    #[allow(non_upper_case_globals)]
    pub const ZhCn: Lang = Lang { code: "zh-CN" };
    #[allow(non_upper_case_globals)]
    pub const En: Lang = Lang { code: "en" };

    /// Every loaded locale (embedded + on-disk), ordered by `code`.
    pub fn all() -> impl Iterator<Item = Lang> {
        catalog().iter().map(|l| Lang { code: l.code })
    }

    /// Canonical prefs code (`zh-CN`, `en`, …).
    pub fn code(self) -> &'static str {
        self.meta().map(|m| m.code).unwrap_or(self.code)
    }

    /// CLI `--lang` token (`zh-cn`, `en`, …).
    pub fn cli_code(self) -> &'static str {
        self.meta().map(|m| m.cli).unwrap_or(self.code)
    }

    /// Native label for language menus.
    pub fn native_name(self) -> &'static str {
        self.meta().map(|m| m.native).unwrap_or(self.code)
    }

    /// Parse prefs / CLI / MIME lang tags (case-insensitive). Unknown → `None`.
    pub fn parse(s: &str) -> Option<Lang> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        for loc in catalog() {
            if eq_tag(s, loc.code) || eq_tag(s, loc.cli) {
                return Some(Lang { code: loc.code });
            }
        }
        if eq_tag(s, "zh") || eq_tag(s, "zh-hans") || eq_tag(s, "chinese") {
            return catalog()
                .iter()
                .find(|l| eq_tag(l.code, "zh-CN"))
                .map(|l| Lang { code: l.code });
        }
        if eq_tag(s, "english") || eq_tag(s, "en-us") || eq_tag(s, "en-gb") {
            return catalog()
                .iter()
                .find(|l| eq_tag(l.code, "en"))
                .map(|l| Lang { code: l.code });
        }
        None
    }

    fn meta(self) -> Option<&'static RuntimeLocale> {
        catalog()
            .iter()
            .find(|l| eq_tag(l.code, self.code) || eq_tag(l.cli, self.code))
    }

    fn lookup_in(self, key: &str) -> Option<&'static str> {
        self.meta()?.messages.get(key).copied()
    }
}

fn eq_tag(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn fallback_lang() -> Lang {
    Lang::parse("en")
        .or_else(|| Lang::parse("zh-CN"))
        .or_else(|| catalog().first().map(|l| Lang { code: l.code }))
        .unwrap_or(Lang::En)
}

fn lookup(lang: Lang, key: &str) -> Option<&'static str> {
    lang.lookup_in(key)
        .or_else(|| Lang::En.lookup_in(key))
        .or_else(|| Lang::ZhCn.lookup_in(key))
        .or_else(|| fallback_lang().lookup_in(key))
}

/// Translate a known key. Unknown keys fall back to the key string (owned).
pub fn t_owned(lang: Lang, key: &str) -> String {
    lookup(lang, key)
        .map(str::to_string)
        .unwrap_or_else(|| key.to_string())
}

/// Translate a known static key.
pub fn t(lang: Lang, key: &'static str) -> &'static str {
    lookup(lang, key).unwrap_or(key)
}

pub fn error_message(lang: Lang, err: &CoreError) -> String {
    let base = t_owned(lang, &err.i18n_key());
    match err {
        CoreError::Io(e) => format!("{base}: {e}"),
        CoreError::PathUnsafe(detail)
        | CoreError::UnrarFailed(detail)
        | CoreError::WimlibFailed(detail)
        | CoreError::Other(detail) => format!("{base}: {detail}"),
        CoreError::EntryNotFound(name) => format!("{base}: {name}"),
        _ => base,
    }
}

pub fn code_message(lang: Lang, code: ErrorCode) -> String {
    t_owned(lang, &format!("error.{}", code.as_str()))
}
