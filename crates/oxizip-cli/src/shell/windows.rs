//! HKCU Explorer context-menu verbs (no COM DLL).

use std::path::Path;

use oxizip_i18n::{t, Lang};
use winreg::enums::*;
use winreg::RegKey;

use super::icons::{ensure_icons_dir, icon_path_for_ext};
use super::{archive_extensions, current_oxizip};

const VERB_PREFIX: &str = "Oxizip";

fn classes_root() -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey_with_flags("Software\\Classes", KEY_READ | KEY_WRITE)
        .map_err(|e| format!("open HKCU\\Software\\Classes: {e}"))
}

fn association_key(ext: &str) -> String {
    format!("SystemFileAssociations\\.{ext}")
}

fn quote_path(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

fn icon_value(icon: &Path) -> String {
    format!("{},0", quote_path(icon))
}

fn write_default_icon(classes: &RegKey, ext: &str, icon: &Path) -> Result<(), String> {
    let base = association_key(ext);
    let (key, _) = classes
        .create_subkey(format!("{base}\\DefaultIcon"))
        .map_err(|e| format!("create DefaultIcon for .{ext}: {e}"))?;
    key.set_value("", &icon_value(icon))
        .map_err(|e| format!("set DefaultIcon for .{ext}: {e}"))?;
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
    let _ = key.set_value("Icon", &icon_value(icon));
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
    let _ = key.set_value("Icon", &icon_value(icon));
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
        write_default_icon(classes, ext, &icon)?;
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
    // Checksum on any file (*), not only archives.
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

pub fn install(lang: Lang) -> Result<String, String> {
    let exe = current_oxizip()?;
    let icons_dir = ensure_icons_dir(&exe)?;
    let classes = classes_root()?;
    write_all_verbs(&classes, &exe, &icons_dir, lang)?;
    Ok(format!(
        "{} ({} ; icons={})",
        t(lang, "shell.install_ok"),
        exe.display(),
        icons_dir.display()
    ))
}

pub fn uninstall(lang: Lang) -> Result<String, String> {
    let classes = classes_root()?;
    delete_all_verbs(&classes)?;
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
