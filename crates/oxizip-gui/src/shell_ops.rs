//! OS helpers: trash, reveal in file manager, elevated spawn, clipboard files.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// Spawn another Oxizip GUI process (same executable).
pub fn spawn_new_window() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Command::new(exe)
        .spawn()
        .map_err(|e| e.to_string())
        .map(|_| ())
}

/// Spawn a dedicated `--settings` dialog process (modal sibling).
pub fn spawn_settings_window(tab: Option<&str>) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("--settings");
    if let Some(name) = tab {
        let name = name.trim();
        if !name.is_empty() {
            cmd.arg("--tab").arg(name);
        }
    }
    cmd.spawn().map_err(|e| e.to_string())
}

/// Enable / disable a native window (Windows HWND). Used so the main GUI stays
/// visible but non-operable while the settings dialog process is open.
pub fn set_native_window_enabled(raw_hwnd: u64, enabled: bool) {
    #[cfg(windows)]
    {
        if raw_hwnd == 0 {
            return;
        }
        #[link(name = "user32")]
        extern "system" {
            fn EnableWindow(hwnd: isize, enable: i32) -> i32;
        }
        unsafe {
            EnableWindow(raw_hwnd as isize, if enabled { 1 } else { 0 });
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (raw_hwnd, enabled);
    }
}

/// Spawn elevated new window (Windows UAC / Linux pkexec).
pub fn spawn_new_window_admin() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let file: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
        let op: Vec<u16> = "runas\0".encode_utf16().collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                op.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as usize <= 32 {
            return Err(format!("ShellExecute runas failed ({result:?})"));
        }
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("pkexec")
            .arg(&exe)
            .spawn()
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = exe;
        Err("elevated new window is not supported on macOS in this build".into())
    }
}

/// Reveal `path` in the system file manager (select when possible).
pub fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(parent) = path.parent() {
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())
                .map(|_| ())
        } else {
            Err("no parent directory".into())
        }
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
}

/// Open a directory in the file manager.
pub fn open_folder(path: &Path) -> Result<(), String> {
    open::that(path).map_err(|e| e.to_string())
}

/// Move file to trash / recycle bin.
pub fn trash_file(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::{
            SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FO_DELETE,
            SHFILEOPSTRUCTW,
        };

        let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
        from.push(0);
        from.push(0);
        let mut op = SHFILEOPSTRUCTW {
            hwnd: std::ptr::null_mut(),
            wFunc: FO_DELETE,
            pFrom: from.as_ptr(),
            pTo: std::ptr::null(),
            fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT) as u16,
            fAnyOperationsAborted: 0,
            hNameMappings: std::ptr::null_mut(),
            lpszProgressTitle: std::ptr::null(),
        };
        let rc = unsafe { SHFileOperationW(&mut op) };
        if rc != 0 {
            return Err(format!("SHFileOperation failed ({rc})"));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        if Command::new("gio")
            .args(["trash", "--"])
            .arg(path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        if Command::new("trash-put")
            .arg(path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        Err("trash helper not found (gio trash / trash-put)".into())
    }
}

/// Permanently delete a file.
pub fn delete_file_permanent(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

/// Copy plain text to clipboard.
pub fn clipboard_set_text(text: &str) -> Result<(), String> {
    let mut clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clip.set_text(text.to_string()).map_err(|e| e.to_string())
}

/// Read file paths from clipboard (best-effort).
pub fn clipboard_file_paths() -> Result<Vec<PathBuf>, String> {
    #[cfg(windows)]
    {
        windows_clipboard_files()
    }
    #[cfg(not(windows))]
    {
        // Try text lines that look like paths / file:// URIs.
        let mut clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let text = clip.get_text().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("file://") {
                let decoded = urlencoding_fallback(rest);
                out.push(PathBuf::from(decoded));
            } else if Path::new(line).exists() {
                out.push(PathBuf::from(line));
            }
        }
        if out.is_empty() {
            Err("clipboard has no file paths".into())
        } else {
            Ok(out)
        }
    }
}

/// Put local files on the clipboard as file drop list (Windows CF_HDROP; else text paths).
pub fn clipboard_set_files(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("no files".into());
    }
    #[cfg(windows)]
    {
        windows_clipboard_set_files(paths)
    }
    #[cfg(not(windows))]
    {
        let text = paths
            .iter()
            .map(|p| format!("file://{}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");
        clipboard_set_text(&text)
    }
}

#[cfg(not(windows))]
fn urlencoding_fallback(s: &str) -> String {
    // Minimal %XX decode for file URIs.
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(windows)]
fn windows_clipboard_files() -> Result<Vec<PathBuf>, String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_HDROP;

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("OpenClipboard failed".into());
        }
        let ok = (|| {
            if IsClipboardFormatAvailable(u32::from(CF_HDROP)) == 0 {
                return Err("no CF_HDROP on clipboard".into());
            }
            let handle = GetClipboardData(u32::from(CF_HDROP));
            if handle.is_null() {
                return Err("GetClipboardData failed".into());
            }
            let ptr = GlobalLock(handle) as *const u8;
            if ptr.is_null() {
                return Err("GlobalLock failed".into());
            }
            // DROPFILES: pFiles offset at 0 (u32), then paths double-null UTF16 after offset.
            let offset = u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]) as usize;
            let paths_ptr = ptr.add(offset) as *const u16;
            let mut out = Vec::new();
            let mut i = 0usize;
            loop {
                let mut len = 0usize;
                while *paths_ptr.add(i + len) != 0 {
                    len += 1;
                }
                if len == 0 {
                    break;
                }
                let slice = std::slice::from_raw_parts(paths_ptr.add(i), len);
                out.push(PathBuf::from(OsString::from_wide(slice)));
                i += len + 1;
            }
            GlobalUnlock(handle);
            if out.is_empty() {
                Err("CF_HDROP empty".into())
            } else {
                Ok(out)
            }
        })();
        CloseClipboard();
        ok
    }
}

#[cfg(windows)]
fn windows_clipboard_set_files(paths: &[PathBuf]) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
    };
    use windows_sys::Win32::System::Ole::CF_HDROP;
    use windows_sys::Win32::UI::Shell::DROPFILES;

    let mut encoded: Vec<u16> = Vec::new();
    for p in paths {
        encoded.extend(p.as_os_str().encode_wide());
        encoded.push(0);
    }
    encoded.push(0);

    let header_size = std::mem::size_of::<DROPFILES>();
    let bytes_paths = encoded.len() * 2;
    let total = header_size + bytes_paths;

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("OpenClipboard failed".into());
        }
        EmptyClipboard();
        let hmem = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total);
        if hmem.is_null() {
            CloseClipboard();
            return Err("GlobalAlloc failed".into());
        }
        let ptr = GlobalLock(hmem) as *mut u8;
        if ptr.is_null() {
            CloseClipboard();
            return Err("GlobalLock failed".into());
        }
        let dropfiles = DROPFILES {
            pFiles: header_size as u32,
            pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
            fNC: 0,
            fWide: 1,
        };
        std::ptr::copy_nonoverlapping(
            &dropfiles as *const DROPFILES as *const u8,
            ptr,
            header_size,
        );
        std::ptr::copy_nonoverlapping(
            encoded.as_ptr() as *const u8,
            ptr.add(header_size),
            bytes_paths,
        );
        GlobalUnlock(hmem);
        if SetClipboardData(u32::from(CF_HDROP), hmem).is_null() {
            CloseClipboard();
            return Err("SetClipboardData failed".into());
        }
        CloseClipboard();
    }
    Ok(())
}

/// Locate README beside the executable or in common install layouts.
pub fn find_readme(prefer_zh: bool) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let names = if prefer_zh {
        ["README.md", "README.en.md"]
    } else {
        ["README.en.md", "README.md"]
    };
    for name in names {
        let p = exe_dir.join(name);
        if p.is_file() {
            return Some(p);
        }
        let p2 = exe_dir.join("..").join(name);
        if p2.is_file() {
            return Some(p2.canonicalize().unwrap_or(p2));
        }
    }
    None
}
