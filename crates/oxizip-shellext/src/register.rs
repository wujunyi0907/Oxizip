//! `DllRegisterServer` / `DllUnregisterServer`.
//!
//! Classic `ContextMenuHandlers` are **not** registered (avoids duplicate Oxizip entries
//! beside static verbs). Win11 modern menus use Sparse `SurrogateServer` only.
//! Unregister still scrubs any leftover classic handlers from older builds.

use windows::core::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;

use crate::win::{dll_module, CLSID_OXIZIP_BKG, CLSID_OXIZIP_CTX};

fn guid_braced(g: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

fn module_path() -> Result<String> {
    let mut buf = [0u16; 520];
    let n = unsafe { GetModuleFileNameW(dll_module(), &mut buf) };
    if n == 0 {
        return Err(Error::from_win32());
    }
    Ok(String::from_utf16_lossy(&buf[..n as usize]))
}

fn set_sz(root: HKEY, path: &str, name: PCWSTR, value: &str) -> Result<()> {
    let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let wide_val: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let mut key = HKEY::default();
    let mut disposition = REG_CREATE_KEY_DISPOSITION::default();
    unsafe {
        RegCreateKeyExW(
            root,
            PCWSTR(wide_path.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            Some(&mut disposition),
        )
        .ok()?;
        RegSetValueExW(
            key,
            name,
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                wide_val.as_ptr() as *const u8,
                wide_val.len() * 2,
            )),
        )
        .ok()?;
        RegCloseKey(key).ok()?;
    }
    Ok(())
}

fn delete_tree(root: HKEY, path: &str) {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = RegDeleteTreeW(root, PCWSTR(wide.as_ptr()));
    }
}

fn scrub_context_menu_handlers() {
    for key in [
        "*\\shellex\\ContextMenuHandlers\\OxizipCtx",
        "Directory\\shellex\\ContextMenuHandlers\\OxizipCtx",
        "Directory\\Background\\shellex\\ContextMenuHandlers\\OxizipCtx",
    ] {
        delete_tree(HKEY_LOCAL_MACHINE, &format!("Software\\Classes\\{key}"));
        delete_tree(HKEY_CURRENT_USER, &format!("Software\\Classes\\{key}"));
    }
}

/// Register InprocServer32 CLSIDs for Sparse SurrogateServer; never classic shellex.
pub fn register_server() -> Result<()> {
    scrub_context_menu_handlers();
    let dll = module_path()?;
    let ctx = guid_braced(&CLSID_OXIZIP_CTX);
    let bkg = guid_braced(&CLSID_OXIZIP_BKG);

    for (clsid, title) in [(&ctx, "Oxizip Explorer Command"), (&bkg, "Oxizip Background Command")]
    {
        let base = format!("Software\\Classes\\CLSID\\{clsid}");
        set_sz(HKEY_LOCAL_MACHINE, &base, PCWSTR::null(), title)?;
        set_sz(
            HKEY_LOCAL_MACHINE,
            &format!("{base}\\InprocServer32"),
            PCWSTR::null(),
            &dll,
        )?;
        set_sz(
            HKEY_LOCAL_MACHINE,
            &format!("{base}\\InprocServer32"),
            w!("ThreadingModel"),
            "Apartment",
        )?;
    }

    Ok(())
}

pub fn unregister_server() -> Result<()> {
    scrub_context_menu_handlers();
    let ctx = guid_braced(&CLSID_OXIZIP_CTX);
    let bkg = guid_braced(&CLSID_OXIZIP_BKG);
    for clsid in [&ctx, &bkg] {
        delete_tree(
            HKEY_LOCAL_MACHINE,
            &format!("Software\\Classes\\CLSID\\{clsid}"),
        );
        delete_tree(
            HKEY_CURRENT_USER,
            &format!("Software\\Classes\\CLSID\\{clsid}"),
        );
    }
    Ok(())
}
