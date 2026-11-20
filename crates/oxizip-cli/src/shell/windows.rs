//! HKCU Explorer context-menu verbs + file-type icons + open associations (no COM DLL).
//!
//! Association model aligned with Bandizip.exe (clean-room, from strings + registry):
//! - `Software\Classes\<ProgID>` + `.{ext}` default + DefaultIcon + OpenWithProgids
//! - `RegisteredApplications` + `Capabilities` / `FileAssociations`
//! - classic `FileExts\%s\UserChoice` (`ProgId` + `Hash`) only — **no** `UserChoiceLatest`
//! - `SHChangeNotify(SHCNE_ASSOCCHANGED)`
//!
//! Elevated installs also mirror ProgID / `.{ext}` under HKLM (Bandizip setup path).

use std::path::Path;

use oxizip_i18n::{t, Lang};
use winreg::enums::*;
use winreg::RegKey;

use super::icons::{ensure_icons_dir, icon_path_for_ext, icon_registry_value};
use super::user_choice;
use super::{
    assoc_cleanup_extensions, current_oxizip, current_oxizip_gui, resolve_install_exts,
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
    let _ = open.set_value("Icon", &icon_registry_value(icon));
    let _ = open.set_value("FriendlyAppName", &APP_REG_NAME.to_string());
    let (cmd, _) = open
        .create_subkey("command")
        .map_err(|e| format!("create progid command: {e}"))?;
    cmd.set_value("", &format!("{} \"%1\"", quote_path(gui)))
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

fn write_verb(
    classes: &RegKey,
    ext: &str,
    verb: &str,
    label: &str,
    exe: &Path,
    args: &str,
    icon: &Path,
) -> Result<(), String> {
    let base = format!("{}\\shell\\{VERB_PREFIX}.{verb}", association_key(ext));
    let key = classes
        .create_subkey(&base)
        .map_err(|e| format!("create {base}: {e}"))?
        .0;
    key.set_value("", &label)
        .map_err(|e| format!("set label {base}: {e}"))?;
    let _ = key.set_value("Icon", &icon_registry_value(icon));
    let (cmd_key, _) = key
        .create_subkey("command")
        .map_err(|e| format!("create command under {base}: {e}"))?;
    let command = format!("{} {args}", quote_path(exe));
    cmd_key
        .set_value("", &command)
        .map_err(|e| format!("set command {base}: {e}"))?;
    Ok(())
}

fn delete_verb(classes: &RegKey, ext: &str, verb: &str) -> Result<(), String> {
    let base = format!("{}\\shell\\{VERB_PREFIX}.{verb}", association_key(ext));
    match classes.delete_subkey_all(&base) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("delete {base}: {e}")),
    }
}

fn write_compress_verb(
    root: &RegKey,
    verb: &str,
    label: &str,
    exe: &Path,
    args: &str,
    icon: &Path,
) -> Result<(), String> {
    let key_name = format!("{VERB_PREFIX}.{verb}");
    let key = root
        .create_subkey(&key_name)
        .map_err(|e| format!("create compress verb: {e}"))?
        .0;
    key.set_value("", &label)
        .map_err(|e| format!("set compress label: {e}"))?;
    let _ = key.set_value("Icon", &icon_registry_value(icon));
    let (cmd_key, _) = key
        .create_subkey("command")
        .map_err(|e| format!("create compress command: {e}"))?;
    let command = format!("{} {args}", quote_path(exe));
    cmd_key
        .set_value("", &command)
        .map_err(|e| format!("set compress command: {e}"))?;
    Ok(())
}

fn write_all_verbs(
    classes: &RegKey,
    exe: &Path,
    gui: &Path,
    icons_dir: &Path,
    lang: Lang,
    exts: &[&str],
) -> Result<(), String> {
    let archive_verbs = [
        ("ExtractHere", "shell.extract_here", "extract --here \"%1\""),
        (
            "ExtractSmart",
            "shell.extract_smart",
            "extract --smart \"%1\"",
        ),
        (
            "ExtractToFolder",
            "shell.extract_to_folder",
            "extract --archive-name-dir \"%1\"",
        ),
        ("Test", "shell.test", "test \"%1\""),
        ("Checksum", "shell.checksum", "checksum \"%1\""),
        ("Repair", "shell.repair", "repair \"%1\""),
    ];
    for ext in exts {
        let icon = icon_path_for_ext(icons_dir, ext);
        write_open_association(classes, ext, gui, &icon, lang, true)?;
        for (verb, key, args) in archive_verbs {
            write_verb(classes, ext, verb, t(lang, key), exe, args, &icon)?;
        }
    }

    let zip_icon = icon_path_for_ext(icons_dir, "zip");
    let seven_icon = icon_path_for_ext(icons_dir, "7z");
    let app_icon = icon_path_for_ext(icons_dir, "oxizip");
    let compress = [
        (
            "CompressZip",
            "shell.compress_zip",
            "create --beside -f zip \"%1\"",
            &zip_icon,
        ),
        (
            "Compress7z",
            "shell.compress_7z",
            "create --beside -f 7z \"%1\"",
            &seven_icon,
        ),
    ];
    for root_name in ["*\\shell", "Directory\\shell"] {
        let root = classes
            .create_subkey(root_name)
            .map_err(|e| format!("create {root_name}: {e}"))?
            .0;
        for (verb, key, args, icon) in compress {
            write_compress_verb(&root, verb, t(lang, key), exe, args, icon)?;
        }
    }
    {
        let root = classes
            .create_subkey("*\\shell")
            .map_err(|e| format!("create *\\shell: {e}"))?
            .0;
        write_compress_verb(
            &root,
            "Checksum",
            t(lang, "shell.checksum"),
            exe,
            "checksum \"%1\"",
            &app_icon,
        )?;
    }
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

fn delete_all_verbs(classes: &RegKey) -> Result<(), String> {
    for ext in assoc_cleanup_extensions() {
        for verb in [
            "ExtractHere",
            "ExtractSmart",
            "ExtractToFolder",
            "Test",
            "Checksum",
            "Repair",
        ] {
            delete_verb(classes, ext, verb)?;
        }
        delete_default_icon(classes, ext)?;
        delete_open_association(classes, ext)?;
    }
    for root_name in ["*\\shell", "Directory\\shell"] {
        if let Ok(root) = classes.open_subkey_with_flags(root_name, KEY_READ | KEY_WRITE) {
            for verb in ["CompressZip", "Compress7z", "Checksum"] {
                let key_name = format!("{VERB_PREFIX}.{verb}");
                let _ = root.delete_subkey_all(key_name);
            }
        }
    }
    Ok(())
}

fn cleanup_exe_association(classes: &RegKey) {
    for verb in [
        "ExtractHere",
        "ExtractSmart",
        "ExtractToFolder",
        "Test",
        "Checksum",
        "Repair",
    ] {
        let _ = delete_verb(classes, "exe", verb);
    }
    let _ = delete_default_icon(classes, "exe");
    let _ = delete_open_association(classes, "exe");
}

pub fn install(lang: Lang, requested_exts: &[String]) -> Result<String, String> {
    let exts = resolve_install_exts(requested_exts)?;
    let exe = current_oxizip()?;
    let gui = current_oxizip_gui().unwrap_or_else(|_| exe.clone());
    let icons_dir = ensure_icons_dir(&exe)?;
    let classes = classes_root()?;
    cleanup_exe_association(&classes);
    delete_all_verbs(&classes)?;
    register_capabilities(lang, &exts)?;
    write_all_verbs(&classes, &exe, &gui, &icons_dir, lang, &exts)?;

    // Bandizip setup (elevated) also registers under HKLM\Software\Classes.
    if is_process_elevated() {
        if let Some(hklm) = hklm_classes_root() {
            for ext in &exts {
                let icon = icon_path_for_ext(&icons_dir, ext);
                let _ = write_open_association(&hklm, ext, &gui, &icon, lang, false);
            }
        }
    }

    notify_assoc_changed();
    Ok(format!(
        "{} (exts={} ; cli={} ; gui={} ; icons={})",
        t(lang, "shell.install_ok"),
        exts.join(","),
        exe.display(),
        gui.display(),
        icons_dir.display()
    ))
}

pub fn uninstall(lang: Lang) -> Result<String, String> {
    let classes = classes_root()?;
    cleanup_exe_association(&classes);
    delete_all_verbs(&classes)?;
    unregister_capabilities();
    notify_assoc_changed();
    Ok(t(lang, "shell.uninstall_ok").to_string())
}

pub fn status(lang: Lang) -> Result<String, String> {
    let classes = classes_root()?;
    let probe = format!(
        "{}\\shell\\{VERB_PREFIX}.ExtractHere\\command",
        association_key("zip")
    );
    if classes.open_subkey(probe).is_ok() {
        Ok(t(lang, "shell.status_installed").to_string())
    } else {
        Ok(t(lang, "shell.status_missing").to_string())
    }
}
