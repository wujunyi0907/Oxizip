//! HKCU Explorer context-menu verbs + file-type icons + open associations.
//!
//! Association model aligned with Bandizip.exe (clean-room, from strings + registry):
//! - `Software\Classes\<ProgID>` + `.{ext}` default + DefaultIcon + OpenWithProgids
//! - `RegisteredApplications` + `Capabilities` / `FileAssociations`
//! - classic `FileExts\%s\UserChoice` (`ProgId` + `Hash`) only — **no** `UserChoiceLatest`
//! - `SHChangeNotify(SHCNE_ASSOCCHANGED)`
//! - Cascade static verbs (`ExtendedSubCommandsKey`) + optional `oxizip-shellext.dll`
//!   (classic `IContextMenu3` + Win11 Sparse `IExplorerCommand`)
//!
//! Elevated installs also mirror ProgID / `.{ext}` under HKLM (Bandizip setup path).

use std::path::Path;

use oxizip_i18n::{t, Lang};
use winreg::enums::*;
use winreg::RegKey;

use super::icons::{ensure_icons_dir, icon_path_for_ext, icon_registry_value};
use super::user_choice;
use super::{
    archive_extensions, assoc_cleanup_extensions, current_oxizip, current_oxizip_gui,
    resolve_install_exts,
};

const VERB_PREFIX: &str = "Oxizip";
/// ProgID / 关联 ID（反向域名风格，对齐 identifier `cn.wujunyi.oxizip`）
const PROGID_PREFIX: &str = "cn.wujunyi.oxizip";
/// RegisteredApplications / Capabilities path (Default Programs registration).
const CAPABILITIES_PATH: &str = "Software\\cn.wujunyi.oxizip\\Capabilities";
const APP_REG_NAME: &str = "Oxizip";

fn assoc_type_label(lang: Lang, ext: &str) -> String {
    t(lang, "shell.assoc_type").replace("{ext}", ext)
}

fn classes_root() -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey_with_flags("Software\\Classes", KEY_READ | KEY_WRITE)
        .map_err(|e| format!("open HKCU\\Software\\Classes: {e}"))
}

fn hklm_classes_root() -> Option<RegKey> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm.open_subkey_with_flags("Software\\Classes", KEY_READ | KEY_WRITE)
        .ok()
}

fn is_process_elevated() -> bool {
    use std::os::windows::raw::HANDLE;
    use std::ptr;
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: HANDLE, access: u32, token: *mut HANDLE) -> i32;
        fn GetTokenInformation(
            token: HANDLE,
            class: u32,
            info: *mut u8,
            len: u32,
            ret_len: *mut u32,
        ) -> i32;
        fn GetCurrentProcess() -> HANDLE;
        fn CloseHandle(handle: HANDLE) -> i32;
    }
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION: u32 = 20;
    #[repr(C)]
    struct TokenElevation {
        token_is_elevated: u32,
    }
    unsafe {
        let mut token: HANDLE = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elev = TokenElevation {
            token_is_elevated: 0,
        };
        let mut ret = 0u32;
        let ok = GetTokenInformation(
            token,
            TOKEN_ELEVATION,
            (&mut elev as *mut TokenElevation).cast(),
            std::mem::size_of::<TokenElevation>() as u32,
            &mut ret,
        );
        CloseHandle(token);
        ok != 0 && elev.token_is_elevated != 0
    }
}

fn association_key(ext: &str) -> String {
    format!("SystemFileAssociations\\.{ext}")
}

fn progid_for_ext(ext: &str) -> String {
    format!("{PROGID_PREFIX}.{ext}")
}

fn quote_path(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

fn is_our_progid(progid: &str) -> bool {
    progid
        .to_ascii_lowercase()
        .starts_with(&PROGID_PREFIX.to_ascii_lowercase())
}

fn file_exts_parent(ext: &str) -> String {
    format!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.{ext}")
}

/// Write DefaultIcon in places Explorer consults (Bandizip: ProgID + `.{ext}` + SFA).
fn write_format_icons(classes: &RegKey, ext: &str, icon: &Path) -> Result<(), String> {
    let value = icon_registry_value(icon);
    let progid = progid_for_ext(ext);

    let (prog_icon, _) = classes
        .create_subkey(format!("{progid}\\DefaultIcon"))
        .map_err(|e| format!("create {progid}\\DefaultIcon: {e}"))?;
    prog_icon
        .set_value("", &value)
        .map_err(|e| format!("set {progid}\\DefaultIcon: {e}"))?;

    let (ext_icon, _) = classes
        .create_subkey(format!(".{ext}\\DefaultIcon"))
        .map_err(|e| format!("create .{ext}\\DefaultIcon: {e}"))?;
    ext_icon
        .set_value("", &value)
        .map_err(|e| format!("set .{ext}\\DefaultIcon: {e}"))?;

    let (sfa_icon, _) = classes
        .create_subkey(format!("{}\\DefaultIcon", association_key(ext)))
        .map_err(|e| format!("create SFA DefaultIcon for .{ext}: {e}"))?;
    sfa_icon
        .set_value("", &value)
        .map_err(|e| format!("set SFA DefaultIcon for .{ext}: {e}"))?;

    Ok(())
}

fn read_classic_user_progid(ext: &str) -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let parent = file_exts_parent(ext);
    let k = hkcu.open_subkey(format!("{parent}\\UserChoice")).ok()?;
    let p: String = k.get_value("ProgId").ok()?;
    let p = p.trim().to_string();
    if p.is_empty() {
        None
    } else {
        Some(p)
    }
}

fn read_latest_user_progid(ext: &str) -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let parent = file_exts_parent(ext);
    let k = hkcu
        .open_subkey(format!("{parent}\\UserChoiceLatest\\ProgId"))
        .ok()?;
    let p: String = k.get_value("ProgId").ok()?;
    let p = p.trim().to_string();
    if p.is_empty() {
        None
    } else {
        Some(p)
    }
}

/// Clear only keys that still point at Oxizip (do not wipe Bandizip / system defaults).
fn clear_user_choice_if_ours(ext: &str) {
    let parent = file_exts_parent(ext);
    if read_classic_user_progid(ext).is_some_and(|p| is_our_progid(&p)) {
        user_choice::delete_classic_user_choice_key(&parent);
    }
    if read_latest_user_progid(ext).is_some_and(|p| is_our_progid(&p)) {
        // Legacy Oxizip builds wrote Latest; Bandizip never does — clean ours only.
        user_choice::delete_latest_user_choice_key(&parent);
    }
}

fn clear_file_exts_open_with(ext: &str) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let progid = progid_for_ext(ext);
    let parent = file_exts_parent(ext);
    if let Ok(ow) =
        hkcu.open_subkey_with_flags(format!("{parent}\\OpenWithProgids"), KEY_READ | KEY_WRITE)
    {
        let _ = ow.delete_value(&progid);
    }
    if let Ok(list) =
        hkcu.open_subkey_with_flags(format!("{parent}\\OpenWithList"), KEY_READ | KEY_WRITE)
    {
        let mut drop_names = Vec::new();
        for (name, value) in list.enum_values().filter_map(|v| v.ok()) {
            let s = value.to_string().to_ascii_lowercase();
            if s.contains("oxizip") {
                drop_names.push(name);
            }
        }
        for name in drop_names {
            let _ = list.delete_value(&name);
        }
    }
}

/// Drop HKCU `Software\\Classes\\.{ext}` once Oxizip no longer claims it.
fn release_hkcu_ext_override(classes: &RegKey, ext: &str) {
    let progid = progid_for_ext(ext);
    let ext_path = format!(".{ext}");
    let Ok(ext_key) = classes.open_subkey_with_flags(&ext_path, KEY_READ | KEY_WRITE) else {
        return;
    };

    let ours_or_empty = match ext_key.get_value::<String, _>("") {
        Ok(current) => current.is_empty() || current.eq_ignore_ascii_case(&progid),
        Err(_) => true,
    };

    let _ = ext_key.delete_subkey_all("DefaultIcon");
    if let Ok(open_with) = ext_key.open_subkey_with_flags("OpenWithProgids", KEY_READ | KEY_WRITE) {
        let _ = open_with.delete_value(&progid);
    }

    if ours_or_empty {
        drop(ext_key);
        let _ = classes.delete_subkey_all(&ext_path);
        return;
    }

    if let Ok(open_with) = ext_key.open_subkey_with_flags("OpenWithProgids", KEY_READ | KEY_WRITE) {
        let remaining = open_with.enum_values().filter_map(|v| v.ok()).count();
        drop(open_with);
        if remaining == 0 {
            let _ = ext_key.delete_subkey_all("OpenWithProgids");
        }
    }
}

fn register_capabilities(lang: Lang, exts: &[&str]) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (caps, _) = hkcu
        .create_subkey(CAPABILITIES_PATH)
        .map_err(|e| format!("create Capabilities: {e}"))?;
    caps.set_value("ApplicationName", &APP_REG_NAME.to_string())
        .map_err(|e| format!("set ApplicationName: {e}"))?;
    caps.set_value(
        "ApplicationDescription",
        &t(lang, "shell.app_desc").to_string(),
    )
    .map_err(|e| format!("set ApplicationDescription: {e}"))?;

    let (fa, _) = caps
        .create_subkey("FileAssociations")
        .map_err(|e| format!("create FileAssociations: {e}"))?;
    for name in fa.enum_values().filter_map(|v| v.ok().map(|(n, _)| n)) {
        let _ = fa.delete_value(&name);
    }
    for ext in exts {
        fa.set_value(format!(".{ext}"), &progid_for_ext(ext))
            .map_err(|e| format!("set FileAssociations .{ext}: {e}"))?;
    }

    let (reg_apps, _) = hkcu
        .create_subkey("Software\\RegisteredApplications")
        .map_err(|e| format!("create RegisteredApplications: {e}"))?;
    reg_apps
        .set_value(APP_REG_NAME, &CAPABILITIES_PATH.to_string())
        .map_err(|e| format!("set RegisteredApplications: {e}"))?;
    Ok(())
}

fn unregister_capabilities() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all("Software\\cn.wujunyi.oxizip");
    if let Ok(reg_apps) =
        hkcu.open_subkey_with_flags("Software\\RegisteredApplications", KEY_READ | KEY_WRITE)
    {
        let _ = reg_apps.delete_value(APP_REG_NAME);
    }
}

fn notify_assoc_changed() {
    #[link(name = "shell32")]
    extern "system" {
        fn SHChangeNotify(event: i32, flags: u32, item1: *const u8, item2: *const u8);
    }
    const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
    const SHCNF_IDLIST: u32 = 0x0000;
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

fn write_open_association(
    classes: &RegKey,
    ext: &str,
    gui: &Path,
    icon: &Path,
    lang: Lang,
    with_user_choice: bool,
    filter: &MenuFilter,
) -> Result<(), String> {
    let progid = progid_for_ext(ext);
    let (prog, _) = classes
        .create_subkey(&progid)
        .map_err(|e| format!("create progid {progid}: {e}"))?;
    // Bandizip-style type label: "7z 压缩文件" — Explorer may append " (.7z)".
    let type_label = assoc_type_label(lang, ext);
    prog.set_value("", &type_label)
        .map_err(|e| format!("set progid label: {e}"))?;
    let _ = prog.set_value("FriendlyTypeName", &type_label);

    write_format_icons(classes, ext, icon)?;

    let (open, _) = prog
        .create_subkey("shell\\open")
        .map_err(|e| format!("create progid open: {e}"))?;
    open.set_value("", &t(lang, "shell.open_with_oxizip").to_string())
        .map_err(|e| format!("set open label: {e}"))?;
    if filter.hide_icon {
        let _ = open.delete_value("Icon");
    } else {
        let _ = open.set_value("Icon", &icon_registry_value(icon));
    }
    let _ = open.set_value("FriendlyAppName", &APP_REG_NAME.to_string());
    let (cmd, _) = open
        .create_subkey("command")
        .map_err(|e| format!("create progid command: {e}"))?;
    let open_args = open_command_args(filter);
    cmd.set_value("", &format!("{} {open_args}", quote_path(gui)))
        .map_err(|e| format!("set progid command: {e}"))?;

    // Claim `.{ext}` default ProgID (Bandizip writes this under HKLM when elevated).
    let (ext_key, _) = classes
        .create_subkey(format!(".{ext}"))
        .map_err(|e| format!("create .{ext}: {e}"))?;
    ext_key
        .set_value("", &progid)
        .map_err(|e| format!("set .{ext} progid: {e}"))?;
    let (open_with, _) = ext_key
        .create_subkey("OpenWithProgids")
        .map_err(|e| format!("create OpenWithProgids: {e}"))?;
    open_with
        .set_value(&progid, &"")
        .map_err(|e| format!("set OpenWithProgids: {e}"))?;

    if with_user_choice {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok((fe, _)) = hkcu.create_subkey(format!(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.{ext}\\OpenWithProgids"
        )) {
            let _ = fe.set_value(&progid, &"");
        }
        // Classic UserChoice only — Bandizip does not write UserChoiceLatest.
        user_choice::set_user_default(ext, &progid)?;
    }
    Ok(())
}

fn delete_open_association(classes: &RegKey, ext: &str) -> Result<(), String> {
    let progid = progid_for_ext(ext);
    match classes.delete_subkey_all(&progid) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("delete progid {progid}: {e}")),
    }
    if let Some(hklm) = hklm_classes_root() {
        let _ = hklm.delete_subkey_all(&progid);
        release_hkcu_ext_override(&hklm, ext);
    }
    release_hkcu_ext_override(classes, ext);
    clear_file_exts_open_with(ext);
    clear_user_choice_if_ours(ext);
    Ok(())
}

/// Classic Explorer (Win10 / 「显示更多选项」): **flat** verbs only — no cascade.
/// Win11 modern cascade lives solely in Sparse `IExplorerCommand` (`oxizip-shellext`).
const CASCADE_PARENT: &str = VERB_PREFIX;
const CASCADE_MENU_SUFFIX: &str = "OxizipMenu";

fn write_verb(
    classes: &RegKey,
    ext: &str,
    verb: &str,
    label: &str,
    exe: &Path,
    args: &str,
    icon: &Path,
    multi_select: &str,
) -> Result<(), String> {
    let key_path = format!("{}\\shell\\{VERB_PREFIX}.{verb}", association_key(ext));
    let key = classes
        .create_subkey(&key_path)
        .map_err(|e| format!("create {key_path}: {e}"))?
        .0;
    key.set_value("", &label)
        .map_err(|e| format!("set label {key_path}: {e}"))?;
    let _ = key.set_value("MUIVerb", &label.to_string());
    // Icon is set by callers that honor hide_icon via write_verb_filtered.
    let _ = key.set_value("Icon", &icon_registry_value(icon));
    let _ = key.set_value("MultiSelectModel", &multi_select.to_string());
    let (cmd_key, _) = key
        .create_subkey("command")
        .map_err(|e| format!("create command {key_path}: {e}"))?;
    let command = format!("{} {args}", quote_path(exe));
    cmd_key
        .set_value("", &command)
        .map_err(|e| format!("set command {key_path}: {e}"))?;
    Ok(())
}

fn write_verb_filtered(
    classes: &RegKey,
    ext: &str,
    verb: &str,
    label: &str,
    exe: &Path,
    args: &str,
    icon: &Path,
    multi_select: &str,
    filter: &MenuFilter,
) -> Result<(), String> {
    write_verb(classes, ext, verb, label, exe, args, icon, multi_select)?;
    if filter.hide_icon {
        let key_path = format!("{}\\shell\\{VERB_PREFIX}.{verb}", association_key(ext));
        if let Ok(key) = classes.open_subkey_with_flags(&key_path, KEY_SET_VALUE) {
            let _ = key.delete_value("Icon");
        }
    }
    Ok(())
}

fn write_compress_verb(
    classes: &RegKey,
    root_name: &str,
    verb: &str,
    label: &str,
    exe: &Path,
    args: &str,
    icon: &Path,
) -> Result<(), String> {
    let key_path = format!("{root_name}\\shell\\{VERB_PREFIX}.{verb}");
    let key = classes
        .create_subkey(&key_path)
        .map_err(|e| format!("create {key_path}: {e}"))?
        .0;
    key.set_value("", &label)
        .map_err(|e| format!("set label {key_path}: {e}"))?;
    let _ = key.set_value("MUIVerb", &label.to_string());
    let _ = key.set_value("Icon", &icon_registry_value(icon));
    let _ = key.set_value("MultiSelectModel", &"Player".to_string());
    let (cmd_key, _) = key
        .create_subkey("command")
        .map_err(|e| format!("create command {key_path}: {e}"))?;
    let command = format!("{} {args}", quote_path(exe));
    cmd_key
        .set_value("", &command)
        .map_err(|e| format!("set command {key_path}: {e}"))?;
    Ok(())
}

fn write_all_verbs_filtered(
    classes: &RegKey,
    exe: &Path,
    gui: &Path,
    icons_dir: &Path,
    lang: Lang,
    exts: &[&str],
    filter: &MenuFilter,
) -> Result<(), String> {
    if filter.disabled {
        for ext in exts {
            let icon = icon_path_for_ext(icons_dir, ext);
            write_open_association(classes, ext, gui, &icon, lang, true, filter)?;
        }
        return Ok(());
    }
    for ext in exts {
        let icon = icon_path_for_ext(icons_dir, ext);
        write_open_association(classes, ext, gui, &icon, lang, true, filter)?;
        write_archive_verbs_filtered(classes, ext, exe, gui, &icon, lang, filter)?;
    }
    write_files_dirs_verbs_filtered(
        classes,
        "*",
        exe,
        gui,
        icons_dir,
        lang,
        true,
        "%1",
        filter,
    )?;
    write_files_dirs_verbs_filtered(
        classes,
        "Directory",
        exe,
        gui,
        icons_dir,
        lang,
        false,
        "%1",
        filter,
    )?;
    write_files_dirs_verbs_filtered(
        classes,
        "Directory\\Background",
        exe,
        gui,
        icons_dir,
        lang,
        false,
        "%V",
        filter,
    )?;
    Ok(())
}

fn delete_default_icon(classes: &RegKey, ext: &str) -> Result<(), String> {
    let base = format!("{}\\DefaultIcon", association_key(ext));
    match classes.delete_subkey_all(&base) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("delete {base}: {e}")),
    }
}

fn delete_key_tree(classes: &RegKey, path: &str) {
    match classes.delete_subkey_all(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

fn delete_legacy_flat_verbs(classes: &RegKey, ext: &str) {
    // Pre-cascade layout: Oxizip.ExtractHere etc. under SFA\shell.
    for verb in [
        "ExtractHere",
        "ExtractSmart",
        "ExtractToFolder",
        "ExtractHereDel",
        "ExtractSmartDel",
        "ExtractToFolderDel",
        "ExtractDialog",
        "OpenWith",
        "Test",
        "Checksum",
        "Repair",
    ] {
        delete_key_tree(
            classes,
            &format!("{}\\shell\\{VERB_PREFIX}.{verb}", association_key(ext)),
        );
    }
}

fn delete_all_verbs(classes: &RegKey) -> Result<(), String> {
    for ext in assoc_cleanup_extensions() {
        let sfa = association_key(ext);
        delete_key_tree(classes, &format!("{sfa}\\shell\\{CASCADE_PARENT}"));
        delete_key_tree(classes, &format!("{sfa}\\{CASCADE_MENU_SUFFIX}"));
        delete_legacy_flat_verbs(classes, ext);
        delete_default_icon(classes, ext)?;
        delete_open_association(classes, ext)?;
    }
    for root in ["*", "Directory", "Directory\\Background"] {
        delete_key_tree(classes, &format!("{root}\\shell\\{CASCADE_PARENT}"));
        delete_key_tree(classes, &format!("{root}\\{CASCADE_MENU_SUFFIX}"));
        // Legacy flat compress verbs.
        if let Ok(shell) =
            classes.open_subkey_with_flags(format!("{root}\\shell"), KEY_READ | KEY_WRITE)
        {
            for verb in [
                "CompressZip",
                "Compress7z",
                "CompressZipx",
                "CompressTar",
                "CompressTgz",
                "CompressGz",
                "CompressXz",
                "CompressZpaq",
                "CompressEachZip",
                "CompressEach7z",
                "Checksum",
            ] {
                let _ = shell.delete_subkey_all(format!("{VERB_PREFIX}.{verb}"));
            }
        }
    }
    Ok(())
}

fn cleanup_exe_association(classes: &RegKey) {
    let sfa = association_key("exe");
    delete_key_tree(classes, &format!("{sfa}\\shell\\{CASCADE_PARENT}"));
    delete_key_tree(classes, &format!("{sfa}\\{CASCADE_MENU_SUFFIX}"));
    delete_legacy_flat_verbs(classes, "exe");
    let _ = delete_default_icon(classes, "exe");
    let _ = delete_open_association(classes, "exe");
}

fn shellext_dll(cli: &Path) -> Option<std::path::PathBuf> {
    let dir = cli.parent()?;
    let dll = dir.join("oxizip-shellext.dll");
    dll.is_file().then_some(dll)
}

fn run_regsvr32(dll: &Path, unregister: bool) {
    let mut cmd = std::process::Command::new("regsvr32.exe");
    cmd.arg("/s");
    if unregister {
        cmd.arg("/u");
    }
    cmd.arg(dll);
    let _ = cmd.status();
}

/// Ensure classic `ContextMenuHandlers` from older builds are gone (cascade only via Sparse).
fn scrub_classic_context_menu_handlers() {
    let keys = [
        r"Software\Classes\*\shellex\ContextMenuHandlers\OxizipCtx",
        r"Software\Classes\Directory\shellex\ContextMenuHandlers\OxizipCtx",
        r"Software\Classes\Directory\Background\shellex\ContextMenuHandlers\OxizipCtx",
    ];
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    for key in keys {
        let _ = hkcu.delete_subkey_all(key);
    }
    // HKLM / regsvr32 only when elevated — otherwise deletes can stall.
    if is_process_elevated() {
        let hklm = winreg::RegKey::predef(HKEY_LOCAL_MACHINE);
        for key in keys {
            let _ = hklm.delete_subkey_all(key);
        }
        if let Ok(exe) = current_oxizip() {
            if let Some(dll) = shellext_dll(&exe) {
                run_regsvr32(&dll, true);
            }
        }
    }
}

/// Win11 Sparse Package only (modern cascade). Do **not** register classic IContextMenu.
fn register_shellext_stack(cli: &Path) -> String {
    scrub_classic_context_menu_handlers();
    let Some(_dll) = shellext_dll(cli) else {
        return "shellext=absent".into();
    };
    if !is_process_elevated() {
        return "shellext=dll (sparse needs elevation — re-run shell install as admin, or reinstall)".into();
    }
    let install_dir = match cli.parent() {
        Some(d) => d,
        None => return "shellext=dll".into(),
    };
    let script = install_dir.join("scripts").join("register-sparse.ps1");
    // Prefer packaged script next to install; fall back to repo-relative when developing.
    let script_fallback = cli
        .parent()
        .and_then(|d| d.parent())
        .map(|d| d.join("scripts").join("register-sparse.ps1"));
    let script_path = if script.is_file() {
        Some(script)
    } else {
        script_fallback.filter(|p| p.is_file())
    };
    let status = if let Some(script) = script_path {
        std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script.display().to_string(),
                "-InstallDir",
                &install_dir.display().to_string(),
            ])
            .status()
    } else {
        let manifest = install_dir.join("sparse").join("AppxManifest.xml");
        if !manifest.is_file() {
            return "shellext=dll (sparse manifest missing)".into();
        }
        let cmd = format!(
            "$ErrorActionPreference='Stop'; \
             $p=Get-AppxPackage -Name 'cn.wujunyi.oxizip.shellext' -EA SilentlyContinue; \
             if($p){{Remove-AppxPackage -Package $p.PackageFullName -EA SilentlyContinue}}; \
             Add-AppxPackage -Path '{}' -Register -ExternalLocation '{}'",
            manifest.display(),
            install_dir.display()
        );
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &cmd])
            .status()
    };
    match status {
        Ok(s) if s.success() => "shellext=sparse".into(),
        Ok(s) => format!("shellext=dll (sparse exit {s})"),
        Err(e) => format!("shellext=dll (sparse err {e})"),
    }
}

fn unregister_shellext_stack(cli: &Path) {
    scrub_classic_context_menu_handlers();
    if let Some(dir) = cli.parent() {
        let script = dir.join("scripts").join("unregister-sparse.ps1");
        if script.is_file() {
            let _ = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &script.display().to_string(),
                    "-InstallDir",
                    &dir.display().to_string(),
                ])
                .status();
            return;
        }
    }
    let _ = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "$p=Get-AppxPackage -Name 'cn.wujunyi.oxizip.shellext' -EA SilentlyContinue; if($p){Remove-AppxPackage -Package $p.PackageFullName}",
        ])
        .status();
}

pub fn install(lang: Lang, requested_exts: &[String], menus_only: bool) -> Result<String, String> {
    let exe = current_oxizip()?;
    let gui = current_oxizip_gui().unwrap_or_else(|_| exe.clone());
    let icons_dir = ensure_icons_dir(&exe)?;
    let classes = classes_root()?;
    cleanup_exe_association(&classes);
    delete_all_verbs(&classes)?;

    let filter = load_menu_filter();

    if menus_only {
        // Context menus for every supported archive type + * / Directory compress.
        // Default-open / UserChoice is left to the GUI association page, but
        // ProgID `shell\open\command` must still follow double-click prefs.
        let menu_exts = archive_extensions();
        write_menus_only(&classes, &exe, &gui, &icons_dir, lang, menu_exts, &filter)?;
        update_progid_open_commands(&classes, &gui, &icons_dir, lang, menu_exts, &filter)?;
        let shellext = if filter.cascaded && !filter.disabled {
            register_shellext_stack(&exe)
        } else {
            unregister_shellext_stack(&exe);
            "shellext skipped".into()
        };
        notify_assoc_changed();
        return Ok(format!(
            "{} (menus-only; exts={} ; cli={} ; gui={} ; {})",
            t(lang, "shell.install_ok"),
            menu_exts.join(","),
            exe.display(),
            gui.display(),
            shellext
        ));
    }

    let exts = resolve_install_exts(requested_exts)?;
    register_capabilities(lang, &exts)?;
    write_all_verbs_filtered(&classes, &exe, &gui, &icons_dir, lang, &exts, &filter)?;

    // Bandizip setup (elevated) also registers under HKLM\Software\Classes.
    if is_process_elevated() {
        if let Some(hklm) = hklm_classes_root() {
            for ext in &exts {
                let icon = icon_path_for_ext(&icons_dir, ext);
                let _ = write_open_association(&hklm, ext, &gui, &icon, lang, false, &filter);
            }
        }
    }

    let shellext = if filter.cascaded && !filter.disabled {
        register_shellext_stack(&exe)
    } else {
        unregister_shellext_stack(&exe);
        "shellext skipped".into()
    };
    notify_assoc_changed();
    Ok(format!(
        "{} (exts={} ; cli={} ; gui={} ; icons={} ; {})",
        t(lang, "shell.install_ok"),
        exts.join(","),
        exe.display(),
        gui.display(),
        icons_dir.display(),
        shellext
    ))
}

/// Optional menu filter written by the GUI (`shell_menu.json` next to prefs).
#[derive(Debug, Clone)]
struct MenuFilter {
    extract_here: bool,
    extract_smart: bool,
    extract_to_folder: bool,
    extract_here_del: bool,
    extract_smart_del: bool,
    extract_to_folder_del: bool,
    extract_dialog: bool,
    open_with: bool,
    test: bool,
    checksum: bool,
    compress_zip: bool,
    compress_7z: bool,
    compress_zipx: bool,
    compress_tar: bool,
    compress_tgz: bool,
    compress_gz: bool,
    compress_xz: bool,
    compress_zpaq: bool,
    compress_each_zip: bool,
    compress_each_7z: bool,
    disabled: bool,
    cascaded: bool,
    hide_icon: bool,
    delete_after_dbl_extract: bool,
    /// `open` | `extract_smart` | `extract_to_folder`
    double_click: String,
}

impl Default for MenuFilter {
    fn default() -> Self {
        Self {
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
            disabled: false,
            cascaded: true,
            hide_icon: false,
            delete_after_dbl_extract: false,
            double_click: "open".into(),
        }
    }
}

fn load_menu_filter() -> MenuFilter {
    let path = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("Oxizip").join("shell_menu.json"));
    let Some(path) = path else {
        return MenuFilter::default();
    };
    parse_menu_filter_file(&path)
}

fn parse_menu_filter_file(path: &std::path::Path) -> MenuFilter {
    let Ok(bytes) = std::fs::read(path) else {
        return MenuFilter::default();
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return MenuFilter::default();
    };
    let b = |key: &str, default: bool| -> bool {
        v.get(key).and_then(|x| x.as_bool()).unwrap_or(default)
    };
    let double_click = v
        .get("double_click")
        .and_then(|x| x.as_str())
        .unwrap_or("open")
        .to_string();
    MenuFilter {
        extract_here: b("extract_here", true),
        extract_smart: b("extract_smart", true),
        extract_to_folder: b("extract_to_folder", true),
        extract_here_del: b("extract_here_del", false),
        extract_smart_del: b("extract_smart_del", false),
        extract_to_folder_del: b("extract_to_folder_del", false),
        extract_dialog: b("extract_dialog", true),
        open_with: b("open_with", true),
        test: b("test", true),
        checksum: b("checksum", true),
        compress_zip: b("compress_zip", true),
        compress_7z: b("compress_7z", true),
        compress_zipx: b("compress_zipx", false),
        compress_tar: b("compress_tar", false),
        compress_tgz: b("compress_tgz", false),
        compress_gz: b("compress_gz", false),
        compress_xz: b("compress_xz", false),
        compress_zpaq: b("compress_zpaq", false),
        compress_each_zip: b("compress_each_zip", false),
        compress_each_7z: b("compress_each_7z", false),
        disabled: b("disabled", false),
        cascaded: b("cascaded", true),
        hide_icon: b("hide_icon", false),
        delete_after_dbl_extract: b("delete_after_dbl_extract", false),
        double_click,
    }
}

fn open_command_args(filter: &MenuFilter) -> String {
    let mut args = match filter.double_click.as_str() {
        "extract_smart" => "--job extract --smart \"%1\"".to_string(),
        "extract_to_folder" => "--job extract --archive-name-dir \"%1\"".to_string(),
        _ => "\"%1\"".to_string(),
    };
    if filter.delete_after_dbl_extract
        && matches!(
            filter.double_click.as_str(),
            "extract_smart" | "extract_to_folder"
        )
    {
        args.push_str(" --delete-archive");
    }
    args
}

/// Refresh existing Oxizip ProgID `shell\open\command` for double-click prefs.
/// Does not claim `.{ext}` default or UserChoice (association page owns that).
fn update_progid_open_commands(
    classes: &RegKey,
    gui: &Path,
    icons_dir: &Path,
    lang: Lang,
    exts: &[&str],
    filter: &MenuFilter,
) -> Result<(), String> {
    for ext in exts {
        let progid = progid_for_ext(ext);
        let Ok(prog) = classes.open_subkey_with_flags(&progid, KEY_READ | KEY_WRITE) else {
            // ProgID not registered yet — skip until association install creates it.
            continue;
        };
        let icon = icon_path_for_ext(icons_dir, ext);
        let (open, _) = prog
            .create_subkey("shell\\open")
            .map_err(|e| format!("create {progid} open: {e}"))?;
        open.set_value("", &t(lang, "shell.open_with_oxizip").to_string())
            .map_err(|e| format!("set open label: {e}"))?;
        if filter.hide_icon {
            let _ = open.delete_value("Icon");
        } else {
            let _ = open.set_value("Icon", &icon_registry_value(&icon));
        }
        let _ = open.set_value("FriendlyAppName", &APP_REG_NAME.to_string());
        let (cmd, _) = open
            .create_subkey("command")
            .map_err(|e| format!("create {progid} open command: {e}"))?;
        let open_args = open_command_args(filter);
        cmd.set_value("", &format!("{} {open_args}", quote_path(gui)))
            .map_err(|e| format!("set {progid} open command: {e}"))?;
    }
    Ok(())
}

/// Flat / Sparse-backed verbs without claiming default open (UserChoice).
fn write_menus_only(
    classes: &RegKey,
    exe: &Path,
    gui: &Path,
    icons_dir: &Path,
    lang: Lang,
    exts: &[&str],
    filter: &MenuFilter,
) -> Result<(), String> {
    if filter.disabled {
        return Ok(());
    }
    for ext in exts {
        let icon = icon_path_for_ext(icons_dir, ext);
        let _ = write_format_icons(classes, ext, &icon);
        write_archive_verbs_filtered(classes, ext, exe, gui, &icon, lang, filter)?;
    }
    write_files_dirs_verbs_filtered(
        classes,
        "*",
        exe,
        gui,
        icons_dir,
        lang,
        true,
        "%1",
        filter,
    )?;
    write_files_dirs_verbs_filtered(
        classes,
        "Directory",
        exe,
        gui,
        icons_dir,
        lang,
        false,
        "%1",
        filter,
    )?;
    write_files_dirs_verbs_filtered(
        classes,
        "Directory\\Background",
        exe,
        gui,
        icons_dir,
        lang,
        false,
        "%V",
        filter,
    )?;
    Ok(())
}

fn write_archive_verbs_filtered(
    classes: &RegKey,
    ext: &str,
    cli: &Path,
    gui: &Path,
    icon: &Path,
    lang: Lang,
    filter: &MenuFilter,
) -> Result<(), String> {
    if filter.extract_here {
        write_verb_filtered(
            classes,
            ext,
            "ExtractHere",
            t(lang, "shell.extract_here"),
            gui,
            "--job extract --here \"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    if filter.extract_smart {
        write_verb_filtered(
            classes,
            ext,
            "ExtractSmart",
            t(lang, "shell.extract_smart"),
            gui,
            "--job extract --smart \"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    if filter.extract_to_folder {
        write_verb_filtered(
            classes,
            ext,
            "ExtractToFolder",
            t(lang, "shell.extract_to_folder"),
            gui,
            "--job extract --archive-name-dir \"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    if filter.test {
        write_verb_filtered(
            classes,
            ext,
            "Test",
            t(lang, "shell.test"),
            cli,
            "test \"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    if filter.checksum {
        write_verb_filtered(
            classes,
            ext,
            "Checksum",
            t(lang, "shell.checksum"),
            cli,
            "checksum \"%1\"",
            icon,
            "Single",
            filter,
        )?;
    }
    write_verb_filtered(
        classes,
        ext,
        "Repair",
        t(lang, "shell.repair"),
        cli,
        "repair \"%1\"",
        icon,
        "Single",
        filter,
    )?;
    if filter.extract_here_del {
        write_verb_filtered(
            classes,
            ext,
            "ExtractHereDel",
            t(lang, "shell.extract_here_del"),
            gui,
            "--job extract --here --delete-archive \"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    if filter.extract_smart_del {
        write_verb_filtered(
            classes,
            ext,
            "ExtractSmartDel",
            t(lang, "shell.extract_smart_del"),
            gui,
            "--job extract --smart --delete-archive \"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    if filter.extract_to_folder_del {
        write_verb_filtered(
            classes,
            ext,
            "ExtractToFolderDel",
            t(lang, "shell.extract_folder_del"),
            gui,
            "--job extract --archive-name-dir --delete-archive \"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    if filter.extract_dialog {
        write_verb_filtered(
            classes,
            ext,
            "ExtractDialog",
            t(lang, "shell.extract_dialog"),
            gui,
            "--job extract --dialog \"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    if filter.open_with {
        write_verb_filtered(
            classes,
            ext,
            "OpenWith",
            t(lang, "shell.open_with"),
            gui,
            "\"%1\"",
            icon,
            "Document",
            filter,
        )?;
    }
    Ok(())
}

fn write_compress_verb_filtered(
    classes: &RegKey,
    root_name: &str,
    verb: &str,
    label: &str,
    exe: &Path,
    args: &str,
    icon: &Path,
    filter: &MenuFilter,
) -> Result<(), String> {
    write_compress_verb(classes, root_name, verb, label, exe, args, icon)?;
    if filter.hide_icon {
        let key_path = format!("{root_name}\\shell\\{VERB_PREFIX}.{verb}");
        if let Ok(key) = classes.open_subkey_with_flags(&key_path, KEY_SET_VALUE) {
            let _ = key.delete_value("Icon");
        }
    }
    Ok(())
}

fn write_files_dirs_verbs_filtered(
    classes: &RegKey,
    root_name: &str,
    cli: &Path,
    gui: &Path,
    icons_dir: &Path,
    lang: Lang,
    with_checksum: bool,
    path_ph: &str,
    filter: &MenuFilter,
) -> Result<(), String> {
    let zip_icon = icon_path_for_ext(icons_dir, "zip");
    let seven_icon = icon_path_for_ext(icons_dir, "7z");
    let app_icon = icon_path_for_ext(icons_dir, "oxizip");
    let compress = |enabled: bool, verb: &str, label: &str, fmt: &str, icon: &Path| -> Result<(), String> {
        if !enabled {
            return Ok(());
        }
        write_compress_verb_filtered(
            classes,
            root_name,
            verb,
            label,
            gui,
            &format!("--job create --beside -f {fmt} \"{path_ph}\""),
            icon,
            filter,
        )
    };
    compress(
        filter.compress_zip,
        "CompressZip",
        t(lang, "shell.compress_zip"),
        "zip",
        &zip_icon,
    )?;
    compress(
        filter.compress_7z,
        "Compress7z",
        t(lang, "shell.compress_7z"),
        "7z",
        &seven_icon,
    )?;
    compress(
        filter.compress_zipx,
        "CompressZipx",
        t(lang, "shell.compress_zipx"),
        "zipx",
        &icon_path_for_ext(icons_dir, "zipx"),
    )?;
    compress(
        filter.compress_tar,
        "CompressTar",
        t(lang, "shell.compress_tar"),
        "tar",
        &icon_path_for_ext(icons_dir, "tar"),
    )?;
    compress(
        filter.compress_tgz,
        "CompressTgz",
        t(lang, "shell.compress_tgz"),
        "tgz",
        &icon_path_for_ext(icons_dir, "tgz"),
    )?;
    compress(
        filter.compress_gz,
        "CompressGz",
        t(lang, "shell.compress_gz"),
        "gz",
        &icon_path_for_ext(icons_dir, "gz"),
    )?;
    compress(
        filter.compress_xz,
        "CompressXz",
        t(lang, "shell.compress_xz"),
        "xz",
        &icon_path_for_ext(icons_dir, "xz"),
    )?;
    compress(
        filter.compress_zpaq,
        "CompressZpaq",
        t(lang, "shell.compress_zpaq"),
        "zpaq",
        &icon_path_for_ext(icons_dir, "zpaq"),
    )?;
    if filter.compress_each_zip {
        write_compress_verb_filtered(
            classes,
            root_name,
            "CompressEachZip",
            t(lang, "shell.compress_each_zip"),
            gui,
            &format!("--job create --beside --each -f zip \"{path_ph}\""),
            &zip_icon,
            filter,
        )?;
    }
    if filter.compress_each_7z {
        write_compress_verb_filtered(
            classes,
            root_name,
            "CompressEach7z",
            t(lang, "shell.compress_each_7z"),
            gui,
            &format!("--job create --beside --each -f 7z \"{path_ph}\""),
            &seven_icon,
            filter,
        )?;
    }
    if with_checksum && filter.checksum {
        write_compress_verb_filtered(
            classes,
            root_name,
            "Checksum",
            t(lang, "shell.checksum"),
            cli,
            &format!("checksum \"{path_ph}\""),
            &app_icon,
            filter,
        )?;
    }
    Ok(())
}

pub fn uninstall(lang: Lang) -> Result<String, String> {
    if let Ok(exe) = current_oxizip() {
        unregister_shellext_stack(&exe);
    }
    let classes = classes_root()?;
    cleanup_exe_association(&classes);
    delete_all_verbs(&classes)?;
    unregister_capabilities();
    notify_assoc_changed();
    Ok(t(lang, "shell.uninstall_ok").to_string())
}

pub fn status(lang: Lang) -> Result<String, String> {
    let classes = classes_root()?;
    let flat = format!(
        "{}\\shell\\{VERB_PREFIX}.ExtractHere\\command",
        association_key("zip")
    );
    let cascade = format!("{}\\shell\\{CASCADE_PARENT}", association_key("zip"));
    if classes.open_subkey(&flat).is_ok() || classes.open_subkey(cascade).is_ok() {
        Ok(t(lang, "shell.status_installed").to_string())
    } else {
        Ok(t(lang, "shell.status_missing").to_string())
    }
}
