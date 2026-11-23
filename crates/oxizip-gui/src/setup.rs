//! First-run / post-install setup helpers (file associations via CLI).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::prefs;

/// App config directory (same tree as `prefs.json`: `%APPDATA%\Oxizip` / `~/.config/oxizip`).
pub fn config_dir() -> Option<PathBuf> {
    prefs::prefs_path().parent().map(PathBuf::from)
}

/// Legacy marker dir from older builds (`cn.wujunyi.oxizip`).
fn legacy_config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("cn.wujunyi.oxizip"))
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join(".config")
                .join("cn.wujunyi.oxizip"),
        )
    }
}

pub fn setup_done_marker() -> Option<PathBuf> {
    Some(config_dir()?.join("setup_done"))
}

fn setup_done_already() -> bool {
    if setup_done_marker().is_some_and(|p| p.is_file()) {
        return true;
    }
    legacy_config_dir()
        .map(|d| d.join("setup_done"))
        .is_some_and(|p| p.is_file())
}

/// Whether Oxizip is already the default opener for common archive types.
/// Used to skip the first-run association page (context menus are installed earlier).
pub fn associations_already_present() -> bool {
    #[cfg(windows)]
    {
        windows_zip_associated()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux_zip_associated()
    }
    #[cfg(target_os = "macos")]
    {
        false
    }
}

#[cfg(windows)]
fn is_our_progid(progid: &str) -> bool {
    progid
        .trim()
        .to_ascii_lowercase()
        .starts_with("cn.wujunyi.oxizip.")
}

#[cfg(windows)]
fn windows_ext_associated(ext: &str) -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let parent =
        format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{ext}");
    // Classic UserChoice.
    if let Ok(k) = hkcu.open_subkey(format!("{parent}\\UserChoice")) {
        if let Ok(p) = k.get_value::<String, _>("ProgId") {
            if is_our_progid(&p) {
                return true;
            }
        }
    }
    // Win11 Settings-managed Latest (Bandizip does not write it; we still read it).
    if let Ok(k) = hkcu.open_subkey(format!("{parent}\\UserChoiceLatest\\ProgId")) {
        if let Ok(p) = k.get_value::<String, _>("ProgId") {
            if is_our_progid(&p) {
                return true;
            }
        }
    }
    // HKCU Classes default (elevated / non-UserChoice installs).
    if let Ok(k) = hkcu.open_subkey(format!(r"Software\Classes\.{ext}")) {
        if let Ok(p) = k.get_value::<String, _>("") {
            if is_our_progid(&p) {
                return true;
            }
        }
    }
    false
}

#[cfg(windows)]
fn windows_zip_associated() -> bool {
    // Any common archive defaulted to Oxizip counts as "already associated".
    ["zip", "7z", "rar", "tar", "gz"]
        .into_iter()
        .any(windows_ext_associated)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_zip_associated() -> bool {
    let home = match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => return false,
    };
    let mimeapps = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("mimeapps.list");
    let Ok(text) = fs::read_to_string(&mimeapps) else {
        return false;
    };
    // Look for application/zip=cn.wujunyi.oxizip.desktop in Default Applications.
    let mut in_defaults = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_defaults = t.eq_ignore_ascii_case("[Default Applications]");
            continue;
        }
        if !in_defaults || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((mime, desktop)) = t.split_once('=') {
            if mime.trim() == "application/zip"
                && desktop
                    .split(';')
                    .any(|d| d.trim() == "cn.wujunyi.oxizip.desktop")
            {
                return true;
            }
        }
    }
    false
}

/// Show the association page on first GUI launch unless the user already
/// finished/skipped it, or Oxizip is already a default opener for archives.
///
/// Installer always relaunches with `--setup`. That flag only means “post-install
/// entry”; it must **not** force the page again when `setup_done` already exists
/// (clean machine reinstall / upgrade keeps `%APPDATA%\Oxizip\setup_done`).
pub fn needs_first_run_setup(_force: bool) -> bool {
    if associations_already_present() {
        mark_setup_done();
        return false;
    }
    !setup_done_already()
}

pub fn mark_setup_done() {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        if let Some(marker) = setup_done_marker() {
            let _ = fs::write(marker, b"1\n");
        }
    }
}

fn cli_beside_gui() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("OXIZIP_CLI") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let gui = std::env::current_exe().ok()?;
    let dir = gui.parent()?;
    #[cfg(windows)]
    let names = ["oxizip-cli.exe", "oxizip.exe"];
    #[cfg(not(windows))]
    let names = ["oxizip-cli", "oxizip"];
    for name in names {
        let cli = dir.join(name);
        // Prefer dedicated CLI binary; fall back only if it exists as separate file.
        if cli.is_file() && cli != gui {
            return Some(cli);
        }
    }
    #[cfg(windows)]
    {
        let cli = dir.join("oxizip-cli.exe");
        if cli.is_file() {
            return Some(cli);
        }
    }
    #[cfg(not(windows))]
    {
        let cli = dir.join("oxizip-cli");
        if cli.is_file() {
            return Some(cli);
        }
    }
    None
}

fn run_shell_cli(lang_arg: &str, extra: &[&str]) -> Result<String, String> {
    let cli = cli_beside_gui().ok_or_else(|| {
        String::from(
            "oxizip-cli not found beside oxizip; reinstall or run: oxizip-cli shell install",
        )
    })?;
    let mut args: Vec<String> = vec![
        "--lang".into(),
        lang_arg.into(),
        "shell".into(),
        "install".into(),
    ];
    for a in extra {
        args.push((*a).into());
    }
    let mut cmd = Command::new(&cli);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .args(&args)
        .output()
        .map_err(|e| format!("spawn {}: {e}", cli.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(if stdout.is_empty() {
            "ok".into()
        } else {
            stdout
        })
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

/// Run `oxizip-cli shell install` using the CLI next to this GUI binary.
/// `exts` empty means all supported formats.
pub fn install_associations(lang_arg: &str, exts: &[&str]) -> Result<String, String> {
    let mut extra: Vec<&str> = Vec::new();
    let joined;
    if !exts.is_empty() {
        extra.push("--ext");
        joined = exts.join(",");
        extra.push(&joined);
    }
    run_shell_cli(lang_arg, &extra)
}

/// Refresh context menus only (no default-open / UserChoice).
pub fn install_menus_only(lang_arg: &str) -> Result<String, String> {
    run_shell_cli(lang_arg, &["--menus-only"])
}

pub fn parse_launch_args() -> (bool, Option<PathBuf>) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Shell job / settings dialogs own the argv; do not treat paths as "open archive".
    if args.iter().any(|a| a == "--job" || a == "--settings") {
        return (false, None);
    }
    let mut force_setup = false;
    let mut open_path = None;
    for arg in args {
        if arg == "--setup" || arg == "-setup" {
            force_setup = true;
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        let p = PathBuf::from(&arg);
        if p.is_file() {
            open_path = Some(p);
        }
    }
    (force_setup, open_path)
}

/// `oxizip --settings [--tab assoc|shell|…]`
pub fn parse_settings_launch() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.iter().any(|a| a == "--settings") {
        return None;
    }
    let mut tab = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--tab" {
            i += 1;
            if let Some(name) = args.get(i) {
                tab = Some(name.clone());
            }
        }
        i += 1;
    }
    Some(tab.unwrap_or_default())
}

/// Association groups shown on the first-run / settings dialog (clean-room layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssocGroup {
    Archive,
    Unix,
    Other,
    Disk,
}

impl AssocGroup {
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Archive => "gui.setup.group.archive",
            Self::Unix => "gui.setup.group.unix",
            Self::Other => "gui.setup.group.other",
            Self::Disk => "gui.setup.group.disk",
        }
    }
}

/// Supported extensions for the association UI (aligned with CLI shell list).
pub fn assoc_catalog() -> &'static [(AssocGroup, &'static [&'static str])] {
    &[
        (
            AssocGroup::Archive,
            &["zip", "zipx", "7z", "rar", "zpaq", "alz", "egg", "cab"],
        ),
        (
            AssocGroup::Unix,
            &["tar", "tgz", "gz", "xz", "zst", "bz2", "lzh", "lha"],
        ),
        (AssocGroup::Other, &["001"]),
        // Present but unchecked by default — keep Windows mount / default handlers.
        (AssocGroup::Disk, &["iso", "wim", "esd", "swm"]),
    ]
}

pub fn default_assoc_selection() -> BTreeMap<&'static str, bool> {
    let mut map = BTreeMap::new();
    for (group, exts) in assoc_catalog() {
        let on = *group != AssocGroup::Disk;
        for ext in *exts {
            map.insert(*ext, on);
        }
    }
    map
}

pub fn selected_exts(selection: &BTreeMap<&'static str, bool>) -> Vec<&'static str> {
    selection
        .iter()
        .filter_map(|(ext, on)| (*on).then_some(*ext))
        .collect()
}
