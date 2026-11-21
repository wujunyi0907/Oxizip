//! Locate sibling `oxizip-cli.exe` / `oxizip.exe` and spawn jobs.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::{
    CreateProcessW, CREATE_NO_WINDOW, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{IShellItem, IShellItemArray, SIGDN_FILESYSPATH};

use crate::win::dll_module;

fn install_dir() -> Result<PathBuf> {
    let mut buf = [0u16; 260];
    let n = unsafe {
        windows::Win32::System::LibraryLoader::GetModuleFileNameW(dll_module(), &mut buf)
    };
    if n == 0 {
        return Err(Error::from_win32());
    }
    let dll = PathBuf::from(String::from_utf16_lossy(&buf[..n as usize]));
    dll.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::from(E_FAIL))
}

pub fn cli_path() -> Result<PathBuf> {
    let cli = install_dir()?.join("oxizip-cli.exe");
    if cli.is_file() {
        Ok(cli)
    } else {
        Err(Error::from(E_FAIL))
    }
}

pub fn gui_path() -> Result<PathBuf> {
    let gui = install_dir()?.join("oxizip.exe");
    if gui.is_file() {
        Ok(gui)
    } else {
        Err(Error::from(E_FAIL))
    }
}

/// Absolute icon location for Win11 `IExplorerCommand::GetIcon`.
/// Prefer bare `.ico` under `assets\icons\` (same layout as classic verb Icon values);
/// fall back to `oxizip.exe,0` if the ico is missing.
pub fn icon_location_for_action(action: &str) -> Result<String> {
    let dir = install_dir()?;
    let icons = dir.join("assets").join("icons");
    let stem = match action {
        "CompressZip" | "ExtractHere" | "ExtractSmart" | "ExtractToFolder" => "zip",
        "Compress7z" => "7z",
        _ => "oxizip",
    };
    let preferred = icons.join(format!("{stem}.ico"));
    if preferred.is_file() {
        return Ok(normalize_icon_path(&preferred));
    }
    let app = icons.join("oxizip.ico");
    if app.is_file() {
        return Ok(normalize_icon_path(&app));
    }
    let exe = dir.join("oxizip.exe");
    if exe.is_file() {
        return Ok(format!("{},0", normalize_icon_path(&exe)));
    }
    Err(Error::from(E_FAIL))
}

fn normalize_icon_path(path: &Path) -> String {
    let s = path.display().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\"', "\\\""))
}

fn run_exe(exe: &Path, args: &[String], hide_console: bool) -> Result<()> {
    let mut cmdline = quote(&exe.display().to_string());
    for a in args {
        cmdline.push(' ');
        if a.contains(' ') || a.contains('"') {
            cmdline.push_str(&quote(a));
        } else {
            cmdline.push_str(a);
        }
    }
    let mut cmdline_wide: Vec<u16> = cmdline.encode_utf16().chain(std::iter::once(0)).collect();
    let app_wide: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();
    let flags = if hide_console {
        CREATE_NO_WINDOW
    } else {
        Default::default()
    };
    unsafe {
        CreateProcessW(
            PCWSTR(app_wide.as_ptr()),
            PWSTR(cmdline_wide.as_mut_ptr()),
            None,
            None,
            false,
            flags,
            None,
            None,
            &si,
            &mut pi,
        )?;
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
    }
    Ok(())
}

pub fn run_cli(args: &[String]) -> Result<()> {
    run_exe(&cli_path()?, args, true)
}

/// Compress / extract use GUI job dialog; other actions still use CLI.
pub fn run_action(action: &str, paths: &[String]) -> Result<()> {
    let args = args_for_action(action, paths);
    if matches!(
        action,
        "CompressZip" | "Compress7z" | "ExtractHere" | "ExtractSmart" | "ExtractToFolder"
    ) {
        run_exe(&gui_path()?, &args, false)
    } else {
        run_cli(&args)
    }
}

pub fn args_for_action(action: &str, paths: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    match action {
        "ExtractHere" => {
            out.push("--job".into());
            out.push("extract".into());
            out.push("--here".into());
        }
        "ExtractSmart" => {
            out.push("--job".into());
            out.push("extract".into());
            out.push("--smart".into());
        }
        "ExtractToFolder" => {
            out.push("--job".into());
            out.push("extract".into());
            out.push("--archive-name-dir".into());
        }
        "Test" => out.push("test".into()),
        "Checksum" => out.push("checksum".into()),
        "Repair" => out.push("repair".into()),
        "CompressZip" => {
            out.push("--job".into());
            out.push("create".into());
            out.push("--beside".into());
            out.push("-f".into());
            out.push("zip".into());
        }
        "Compress7z" => {
            out.push("--job".into());
            out.push("create".into());
            out.push("--beside".into());
            out.push("-f".into());
            out.push("7z".into());
        }
        _ => {
            out.push("--job".into());
            out.push("extract".into());
            out.push("--smart".into());
        }
    }
    out.extend(paths.iter().cloned());
    out
}

pub fn paths_from_array(items: Option<&IShellItemArray>) -> Result<Vec<String>> {
    let Some(items) = items else {
        return Ok(Vec::new());
    };
    let count = unsafe { items.GetCount()? };
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        let item: IShellItem = unsafe { items.GetItemAt(i)? };
        let path = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
        paths.push(unsafe { path.to_string()? });
        unsafe {
            windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as _));
        }
    }
    Ok(paths)
}

pub fn is_archive_path(path: &str) -> bool {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "zip"
            | "zipx"
            | "7z"
            | "rar"
            | "tar"
            | "gz"
            | "tgz"
            | "bz2"
            | "xz"
            | "txz"
            | "zst"
            | "tzst"
            | "lz"
            | "lzma"
            | "cab"
            | "iso"
            | "wim"
            | "esd"
            | "swm"
            | "alz"
            | "egg"
            | "zpaq"
            | "001"
    )
}

pub fn selection_kind(paths: &[String]) -> SelectionKind {
    if paths.is_empty() {
        return SelectionKind::Background;
    }
    if paths.iter().all(|p| is_archive_path(p)) {
        SelectionKind::Archives
    } else {
        SelectionKind::FilesOrDirs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionKind {
    Archives,
    FilesOrDirs,
    Background,
}

pub fn child_actions(kind: SelectionKind) -> &'static [&'static str] {
    match kind {
        SelectionKind::Archives => &[
            "ExtractHere",
            "ExtractSmart",
            "ExtractToFolder",
            "Test",
            "Checksum",
            "Repair",
        ],
        SelectionKind::FilesOrDirs => &["CompressZip", "Compress7z", "Checksum"],
        SelectionKind::Background => &["CompressZip", "Compress7z"],
    }
}

/// All submenu items for Win11 Sparse parents. `EnumSubCommands` has no selection;
/// each child's `GetState` hides inapplicable actions.
pub fn child_actions_sparse_all() -> &'static [&'static str] {
    &[
        "ExtractHere",
        "ExtractSmart",
        "ExtractToFolder",
        "Test",
        "Checksum",
        "Repair",
        "CompressZip",
        "Compress7z",
    ]
}

pub fn title_zh_short(action: &str) -> &'static str {
    match action {
        "Parent" => "Oxizip",
        "ExtractHere" => "解压到此处",
        "ExtractSmart" => "智能解压",
        "ExtractToFolder" => "解压到归档名文件夹",
        "Test" => "测试压缩包",
        "Checksum" => "校验和信息",
        "Repair" => "修复压缩包",
        "CompressZip" => "压缩为 ZIP",
        "Compress7z" => "压缩为 7z",
        _ => "Oxizip",
    }
}
