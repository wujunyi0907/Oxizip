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

pub fn run(action: ShellAction, lang: Lang) -> ExitCode {
    match action {
        ShellAction::Install => install(lang),
        ShellAction::Uninstall => uninstall(lang),
        ShellAction::Status => status(lang),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ShellAction {
    Install,
    Uninstall,
    Status,
}

fn install(lang: Lang) -> ExitCode {
    match platform_install(lang) {
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
fn platform_install(lang: Lang) -> Result<String, String> {
    windows::install(lang)
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
fn platform_install(lang: Lang) -> Result<String, String> {
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
fn platform_install(_lang: Lang) -> Result<String, String> {
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

/// Archive extensions registered for context actions.
pub fn archive_extensions() -> &'static [&'static str] {
    &[
        "zip", "zipx", "7z", "rar", "tar", "tgz", "gz", "xz", "zst", "bz2", "lzh", "lha", "iso",
        "zpaq", "cab", "wim", "esd", "swm", "alz", "egg", "exe", "001",
    ]
}
