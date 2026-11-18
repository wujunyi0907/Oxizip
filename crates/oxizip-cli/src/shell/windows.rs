//! HKCU Explorer context-menu verbs + file-type icons + open associations (no COM DLL).

use std::path::Path;

use oxizip_i18n::{t, Lang};
use winreg::enums::*;
use winreg::RegKey;

use super::icons::{ensure_icons_dir, icon_path_for_ext, icon_registry_value};
use super::{archive_extensions, current_oxizip, current_oxizip_gui};

const VERB_PREFIX: &str = "Oxizip";
/// ProgID / 关联 ID（反向域名风格，对齐 identifier `cn.wujunyi.oxizip`）
const PROGID_PREFIX: &str = "cn.wujunyi.oxizip";

fn classes_root() -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey_with_flags("Software\\Classes", KEY_READ | KEY_WRITE)
        .map_err(|e| format!("open HKCU\\Software\\Classes: {e}"))
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

/// Write DefaultIcon in all places Explorer may consult for a file type.
fn write_format_icons(classes: &RegKey, ext: &str, icon: &Path) -> Result<(), String> {
    let value = icon_registry_value(icon);

    // 1) ProgID (used when .{ext} default = our ProgID)
    let progid = progid_for_ext(ext);
    let (prog_icon, _) = classes
        .create_subkey(format!("{progid}\\DefaultIcon"))
        .map_err(|e| format!("create {progid}\\DefaultIcon: {e}"))?;
    prog_icon
        .set_value("", &value)
        .map_err(|e| format!("set {progid}\\DefaultIcon: {e}"))?;

    // 2) Directly under .{ext}\DefaultIcon
    let (ext_icon, _) = classes
        .create_subkey(format!(".{ext}\\DefaultIcon"))
        .map_err(|e| format!("create .{ext}\\DefaultIcon: {e}"))?;
    ext_icon
        .set_value("", &value)
        .map_err(|e| format!("set .{ext}\\DefaultIcon: {e}"))?;

    // 3) SystemFileAssociations (Explorer overlays / some views)
    let (sfa_icon, _) = classes
        .create_subkey(format!("{}\\DefaultIcon", association_key(ext)))
        .map_err(|e| format!("create SFA DefaultIcon for .{ext}: {e}"))?;
    sfa_icon
        .set_value("", &value)
        .map_err(|e| format!("set SFA DefaultIcon for .{ext}: {e}"))?;

    Ok(())
}

fn clear_user_choice(ext: &str) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.{ext}"
    );
    if let Ok(key) = hkcu.open_subkey_with_flags(&path, KEY_READ | KEY_WRITE) {
        let _ = key.delete_subkey_all("UserChoice");
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
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, std::ptr::null(), std::ptr::null());
    }
}

fn write_open_association(
    classes: &RegKey,
    ext: &str,
    gui: &Path,
    icon: &Path,
    lang: Lang,
) -> Result<(), String> {
    let progid = progid_for_ext(ext);
    let (prog, _) = classes
        .create_subkey(&progid)
        .map_err(|e| format!("create progid {progid}: {e}"))?;
    prog.set_value("", &format!("Oxizip {ext}"))
        .map_err(|e| format!("set progid label: {e}"))?;

    write_format_icons(classes, ext, icon)?;

    let (open, _) = prog
        .create_subkey("shell\\open")
        .map_err(|e| format!("create progid open: {e}"))?;
    open.set_value("", &t(lang, "shell.open_with_oxizip").to_string())
        .map_err(|e| format!("set open label: {e}"))?;
    let _ = open.set_value("Icon", &icon_registry_value(icon));
    let (cmd, _) = open
        .create_subkey("command")
        .map_err(|e| format!("create progid command: {e}"))?;
    cmd.set_value("", &format!("{} \"%1\"", quote_path(gui)))
        .map_err(|e| format!("set progid command: {e}"))?;

    // Make Oxizip the current-user default handler for this extension.
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

    // Drop previous UserChoice so our ProgID (and its icon) become active.
    clear_user_choice(ext);
    Ok(())
}

fn delete_open_association(classes: &RegKey, ext: &str) -> Result<(), String> {
    let progid = progid_for_ext(ext);
    match classes.delete_subkey_all(&progid) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("delete progid {progid}: {e}")),
    }
    if let Ok(ext_key) = classes.open_subkey_with_flags(format!(".{ext}"), KEY_READ | KEY_WRITE) {
        let _ = ext_key.delete_value("");
        let _ = ext_key.delete_subkey_all("DefaultIcon");
        if let Ok(open_with) =
            ext_key.open_subkey_with_flags("OpenWithProgids", KEY_READ | KEY_WRITE)
        {
            let _ = open_with.delete_value(&progid);
        }
    }
    clear_user_choice(ext);
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
) -> Result<(), String> {
    let archive_verbs = [
        ("ExtractHere", "shell.extract_here", "extract --here \"%1\""),
        ("ExtractSmart", "shell.extract_smart", "extract --smart \"%1\""),
        (
            "ExtractToFolder",
            "shell.extract_to_folder",
            "extract --archive-name-dir \"%1\"",
        ),
        ("Test", "shell.test", "test \"%1\""),
        ("Checksum", "shell.checksum", "checksum \"%1\""),
        ("Repair", "shell.repair", "repair \"%1\""),
    ];
    for ext in archive_extensions() {
        let icon = icon_path_for_ext(icons_dir, ext);
        write_open_association(classes, ext, gui, &icon, lang)?;
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
    for ext in archive_extensions() {
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

/// Remove any leftover `.exe` association from older Oxizip builds (do not touch executables).
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

pub fn install(lang: Lang) -> Result<String, String> {
    let exe = current_oxizip()?;
    let gui = current_oxizip_gui().unwrap_or_else(|_| exe.clone());
    let icons_dir = ensure_icons_dir(&exe)?;
    let classes = classes_root()?;
    cleanup_exe_association(&classes);
    write_all_verbs(&classes, &exe, &gui, &icons_dir, lang)?;
    notify_assoc_changed();
    Ok(format!(
        "{} (cli={} ; gui={} ; icons={})",
        t(lang, "shell.install_ok"),
        exe.display(),
        gui.display(),
        icons_dir.display()
    ))
}

pub fn uninstall(lang: Lang) -> Result<String, String> {
    let classes = classes_root()?;
    cleanup_exe_association(&classes);
    delete_all_verbs(&classes)?;
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
