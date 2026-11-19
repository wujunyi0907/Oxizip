//! Cross-platform shell / file-manager integration.
//!
//! Windows: HKCU Explorer verbs calling this `oxizip` binary.
//! Linux: FreeDesktop `.desktop` files + Nautilus scripts + KDE service menus.

use std::path::PathBuf;
use std::process::ExitCode;

use oxizip_i18n::Lang;

#[cfg(windows)]
mod icons;
#[cfg(windows)]
mod windows;
#[cfg(all(unix, not(target_os = "macos")))]
mod linux;

pub fn current_oxizip() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

/// Locate GUI `oxizip` next to the CLI binary (same install directory).
pub fn current_oxizip_gui() -> Result<PathBuf, String> {
    let cli = current_oxizip()?;
    let dir = cli
        .parent()
        .ok_or_else(|| "oxizip-cli has no parent directory".to_string())?;
    #[cfg(windows)]
    let gui = dir.join("oxizip.exe");
    #[cfg(not(windows))]
    let gui = dir.join("oxizip");
    if gui.is_file() {
        Ok(gui)
    } else {
        Err(format!("oxizip (GUI) not found beside {}", cli.display()))
    }
}

pub fn run(action: ShellAction, lang: Lang) -> ExitCode {
    match action {
        ShellAction::Install { exts } => install(lang, &exts),
        ShellAction::Uninstall => uninstall(lang),
        ShellAction::Status => status(lang),
    }
}

#[derive(Debug, Clone)]
pub enum ShellAction {
    Install { exts: Vec<String> },
    Uninstall,
    Status,
}

fn install(lang: Lang, exts: &[String]) -> ExitCode {
    match platform_install(lang, exts) {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn uninstall(lang: Lang) -> ExitCode {
    match platform_uninstall(lang) {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn status(lang: Lang) -> ExitCode {
    match platform_status(lang) {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(windows)]
fn platform_install(lang: Lang, exts: &[String]) -> Result<String, String> {
    windows::install(lang, exts)
}
#[cfg(windows)]
fn platform_uninstall(lang: Lang) -> Result<String, String> {
    windows::uninstall(lang)
}
#[cfg(windows)]
fn platform_status(lang: Lang) -> Result<String, String> {
    windows::status(lang)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_install(lang: Lang, _exts: &[String]) -> Result<String, String> {
    // Linux desktop files cover MIME types as a set; per-ext filter is Windows-oriented.
    let _ = _exts;
    linux::install(lang)
}
#[cfg(all(unix, not(target_os = "macos")))]
fn platform_uninstall(lang: Lang) -> Result<String, String> {
    linux::uninstall(lang)
}
#[cfg(all(unix, not(target_os = "macos")))]
fn platform_status(lang: Lang) -> Result<String, String> {
    linux::status(lang)
}

#[cfg(target_os = "macos")]
fn platform_install(_lang: Lang, _exts: &[String]) -> Result<String, String> {
    Err(
        "macOS shell integration is not implemented yet; use CLI extract/create modes directly"
            .into(),
    )
}
#[cfg(target_os = "macos")]
fn platform_uninstall(_lang: Lang) -> Result<String, String> {
    Err("macOS shell integration is not implemented yet".into())
}
#[cfg(target_os = "macos")]
fn platform_status(_lang: Lang) -> Result<String, String> {
    Ok("macOS: shell menus not registered (unsupported in this build)".into())
}

/// Archive extensions registered for context actions / default open / icons.
/// Note: `.exe` is intentionally excluded — never hijack executable icons or Open.
pub fn archive_extensions() -> &'static [&'static str] {
    &[
        "zip", "zipx", "7z", "rar", "tar", "tgz", "gz", "xz", "zst", "bz2", "lzh", "lha", "iso",
        "zpaq", "cab", "wim", "esd", "swm", "alz", "egg", "001",
    ]
}
