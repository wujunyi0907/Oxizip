//! FreeDesktop file associations + Nautilus / KDE service-menu integration.
//!
//! Mirrors the Windows shell install surface for the GUI association page:
//! - default open handler for selected extensions (`.desktop` + `xdg-mime default`)
//! - optional custom MIME package for uncommon types
//! - context actions via `.desktop` Actions / Nautilus scripts / KDE service menus

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use oxizip_i18n::{t, Lang};

use super::{
    archive_extensions, current_oxizip, current_oxizip_gui, resolve_install_exts,
};

/// Desktop file id used with `xdg-mime default` (must match the file name).
const OPEN_DESKTOP_ID: &str = "cn.wujunyi.oxizip.desktop";
const MIME_PACKAGE_NAME: &str = "oxizip-associations.xml";

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())
}

fn applications_dir(home: &Path) -> PathBuf {
    home.join(".local/share/applications")
}

fn mime_packages_dir(home: &Path) -> PathBuf {
    home.join(".local/share/mime/packages")
}

fn config_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
}

fn nautilus_scripts_dir(home: &Path) -> PathBuf {
    home.join(".local/share/nautilus/scripts")
}

fn kde_servicemenus_dir(home: &Path) -> PathBuf {
    home.join(".local/share/kio/servicemenus")
}

const NAUTILUS_SCRIPT_NAMES: &[&str] = &[
    "Oxizip Extract Here",
    "Oxizip Extract Smart",
    "Oxizip Extract To Folder",
    "Oxizip Extract Here and Delete",
    "Oxizip Extract Smart and Delete",
    "Oxizip Extract To Folder and Delete",
    "Oxizip Extract to Another Folder",
    "Oxizip Open",
    "Oxizip Compress ZIP",
    "Oxizip Compress 7z",
    "Oxizip Compress ZIPX",
    "Oxizip Compress TAR",
    "Oxizip Compress TGZ",
    "Oxizip Compress GZ",
    "Oxizip Compress XZ",
    "Oxizip Compress ZPAQ",
    "Oxizip Compress Each ZIP",
    "Oxizip Compress Each 7z",
];

fn marker_files(home: &Path) -> Vec<PathBuf> {
    let scripts = nautilus_scripts_dir(home);
    let mut paths = vec![
        applications_dir(home).join(OPEN_DESKTOP_ID),
        applications_dir(home).join("oxizip-extract.desktop"),
        applications_dir(home).join("oxizip-compress.desktop"),
        mime_packages_dir(home).join(MIME_PACKAGE_NAME),
        kde_servicemenus_dir(home).join("oxizip.desktop"),
    ];
    paths.extend(
        NAUTILUS_SCRIPT_NAMES
            .iter()
            .map(|name| scripts.join(name)),
    );
    paths
}

/// FreeDesktop MIME types claimed for an extension (may include Oxizip-private types).
fn mimes_for_ext(ext: &str) -> &'static [&'static str] {
    match ext {
        "zip" => &["application/zip", "application/x-zip-compressed"],
        "zipx" => &["application/x-zipx"],
        "7z" => &["application/x-7z-compressed"],
        "rar" => &["application/vnd.rar", "application/x-rar-compressed"],
        "tar" => &["application/x-tar"],
        "tgz" => &[
            "application/gzip",
            "application/x-gzip",
            "application/x-gtar",
        ],
        "gz" => &["application/gzip", "application/x-gzip"],
        "xz" => &["application/x-xz"],
        "zst" => &["application/zstd"],
        "bz2" => &["application/x-bzip2"],
        "lzh" | "lha" => &["application/x-lzh-compressed"],
        "iso" => &["application/x-iso9660-image"],
        "zpaq" => &["application/x-zpaq"],
        "cab" => &["application/vnd.ms-cab-compressed"],
        "wim" | "esd" | "swm" => &["application/x-ms-wim"],
        "alz" => &["application/x-alz"],
        "egg" => &["application/x-egg"],
        "001" => &["application/x-oxizip-001"],
        _ => &[],
    }
}

/// MIME types that need a user-local shared-mime-info package (not always shipped).
fn custom_mime_defs() -> &'static [(&'static str, &'static str, &'static [&'static str])] {
    &[
        ("application/x-zipx", "ZIPX archive", &["*.zipx"]),
        ("application/x-zpaq", "ZPAQ archive", &["*.zpaq"]),
        (
            "application/x-ms-wim",
            "Windows Imaging Format",
            &["*.wim", "*.esd", "*.swm"],
        ),
        ("application/x-alz", "ALZ archive", &["*.alz"]),
        ("application/x-egg", "EGG archive", &["*.egg"]),
        ("application/x-oxizip-001", "Split archive part", &["*.001"]),
    ]
}

fn collect_mimes(exts: &[&str]) -> Vec<&'static str> {
    let mut set = BTreeSet::new();
    for ext in exts {
        for mime in mimes_for_ext(ext) {
            set.insert(*mime);
        }
    }
    set.into_iter().collect()
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.starts_with("Oxizip "))
        {
            let mut perms = fs::metadata(path)
                .map_err(|e| format!("stat {}: {e}", path.display()))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms)
                .map_err(|e| format!("chmod {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn mime_type_list(mimes: &[&str]) -> String {
    let mut s = mimes.join(";");
    if !s.is_empty() && !s.ends_with(';') {
        s.push(';');
    }
    s
}

fn open_desktop(gui: &Path, mimes: &[&str]) -> String {
    format!(
        r#"[Desktop Entry]
Type=Application
Version=1.0
Name=Oxizip
GenericName=Archive Manager
GenericName[zh_CN]=压缩管理器
Comment={desc_en}
Comment[zh_CN]={desc_zh}
Exec={exe} %F
TryExec={exe}
Icon=package-x-generic
Terminal=false
Categories=Utility;Archiving;Compression;
MimeType={mimes}
Keywords=zip;7z;rar;tar;archive;compress;oxizip;
StartupNotify=true
StartupWMClass=oxizip
NoDisplay=false
"#,
        desc_en = t(Lang::En, "shell.app_desc"),
        desc_zh = t(Lang::ZhCn, "shell.app_desc"),
        exe = shell_escape_desktop_exec(gui),
        mimes = mime_type_list(mimes),
    )
}

fn extract_desktop(cli: &Path, gui: &Path, mimes: &[&str]) -> String {
    let cli_s = shell_escape_desktop_exec(cli);
    let gui_s = shell_escape_desktop_exec(gui);
    format!(
        r#"[Desktop Entry]
Type=Application
Version=1.0
Name={name_en}
Name[zh_CN]={name_zh}
Comment=Extract archives with Oxizip
Exec={gui} --job extract --smart %f
Icon=package-x-generic
Terminal=false
NoDisplay=true
Categories=Utility;Archiving;Compression;
MimeType={mimes}
Actions=ExtractHere;ExtractSmart;ExtractToFolder;Test;Checksum;

[Desktop Action ExtractHere]
Name={here_en}
Name[zh_CN]={here_zh}
Exec={gui} --job extract --here %f

[Desktop Action ExtractSmart]
Name={smart_en}
Name[zh_CN]={smart_zh}
Exec={gui} --job extract --smart %f

[Desktop Action ExtractToFolder]
Name={folder_en}
Name[zh_CN]={folder_zh}
Exec={gui} --job extract --archive-name-dir %f

[Desktop Action Test]
Name={test_en}
Name[zh_CN]={test_zh}
Exec={cli} test %f

[Desktop Action Checksum]
Name={sum_en}
Name[zh_CN]={sum_zh}
Exec={cli} checksum %f
"#,
        name_en = t(Lang::En, "shell.desktop_extract"),
        name_zh = t(Lang::ZhCn, "shell.desktop_extract"),
        here_en = t(Lang::En, "shell.extract_here"),
        here_zh = t(Lang::ZhCn, "shell.extract_here"),
        smart_en = t(Lang::En, "shell.extract_smart"),
        smart_zh = t(Lang::ZhCn, "shell.extract_smart"),
        folder_en = t(Lang::En, "shell.extract_to_folder"),
        folder_zh = t(Lang::ZhCn, "shell.extract_to_folder"),
        test_en = t(Lang::En, "shell.test"),
        test_zh = t(Lang::ZhCn, "shell.test"),
        sum_en = t(Lang::En, "shell.checksum"),
        sum_zh = t(Lang::ZhCn, "shell.checksum"),
        cli = cli_s,
        gui = gui_s,
        mimes = mime_type_list(mimes),
    )
}

fn compress_desktop(gui: &Path) -> String {
    let exe_s = shell_escape_desktop_exec(gui);
    format!(
        r#"[Desktop Entry]
Type=Application
Version=1.0
Name={name_en}
Name[zh_CN]={name_zh}
Comment=Compress with Oxizip
Exec={exe} --job create --beside -f zip %F
Icon=package-x-generic
Terminal=false
NoDisplay=true
Categories=Utility;Archiving;Compression;
MimeType=application/octet-stream;inode/directory;
Actions=CompressZip;Compress7z;

[Desktop Action CompressZip]
Name={zip_en}
Name[zh_CN]={zip_zh}
Exec={exe} --job create --beside -f zip %F

[Desktop Action Compress7z]
Name={z7_en}
Name[zh_CN]={z7_zh}
Exec={exe} --job create --beside -f 7z %F
"#,
        name_en = t(Lang::En, "shell.desktop_compress"),
        name_zh = t(Lang::ZhCn, "shell.desktop_compress"),
        zip_en = t(Lang::En, "shell.compress_zip"),
        zip_zh = t(Lang::ZhCn, "shell.compress_zip"),
        z7_en = t(Lang::En, "shell.compress_7z"),
        z7_zh = t(Lang::ZhCn, "shell.compress_7z"),
        exe = exe_s,
    )
}

fn nautilus_script(exe: &Path, args: &str) -> String {
    format!(
        "#!/bin/sh\n# Oxizip Nautilus script\nexec {} {} \"$@\"\n",
        shell_quote(&exe.display().to_string()),
        args
    )
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// Escape for Desktop Entry `Exec=` (paths with spaces).
fn shell_escape_desktop_exec(path: &Path) -> String {
    let s = path.display().to_string();
    if s.chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\\'))
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s
    }
}

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
        }
    }
}

fn parse_menu_filter_file(bytes: &[u8]) -> MenuFilter {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return MenuFilter::default();
    };
    let b = |key: &str, default: bool| -> bool {
        v.get(key).and_then(|x| x.as_bool()).unwrap_or(default)
    };
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
    }
}

fn load_menu_filter(home: &Path) -> MenuFilter {
    let path = config_dir(home).join("oxizip").join("shell_menu.json");
    let Ok(bytes) = fs::read(&path) else {
        return MenuFilter::default();
    };
    parse_menu_filter_file(&bytes)
}

struct KdeMenuAction {
    id: &'static str,
    name_en: &'static str,
    name_zh: &'static str,
    exec_suffix: &'static str,
}

fn kde_servicemenu(gui: &Path, mimes: &[&str], filter: &MenuFilter) -> String {
    let gui_s = shell_escape_desktop_exec(gui);
    let mut mime = mime_type_list(mimes);
    if !mime.contains("inode/directory") {
        mime.push_str("inode/directory;application/octet-stream;");
    }
    let mut actions: Vec<KdeMenuAction> = Vec::new();
    let push = |actions: &mut Vec<KdeMenuAction>, id, name_en, name_zh, exec_suffix| {
        actions.push(KdeMenuAction {
            id,
            name_en,
            name_zh,
            exec_suffix,
        });
    };
    if filter.extract_here {
        push(
            &mut actions,
            "ExtractHere",
            t(Lang::En, "shell.extract_here"),
            t(Lang::ZhCn, "shell.extract_here"),
            "--job extract --here %f",
        );
    }
    if filter.extract_smart {
        push(
            &mut actions,
            "ExtractSmart",
            t(Lang::En, "shell.extract_smart"),
            t(Lang::ZhCn, "shell.extract_smart"),
            "--job extract --smart %f",
        );
    }
    if filter.extract_to_folder {
        push(
            &mut actions,
            "ExtractToFolder",
            t(Lang::En, "shell.extract_to_folder"),
            t(Lang::ZhCn, "shell.extract_to_folder"),
            "--job extract --archive-name-dir %f",
        );
    }
    if filter.extract_here_del {
        push(
            &mut actions,
            "ExtractHereDel",
            t(Lang::En, "shell.extract_here_del"),
            t(Lang::ZhCn, "shell.extract_here_del"),
            "--job extract --here --delete-archive %f",
        );
    }
    if filter.extract_smart_del {
        push(
            &mut actions,
            "ExtractSmartDel",
            t(Lang::En, "shell.extract_smart_del"),
            t(Lang::ZhCn, "shell.extract_smart_del"),
            "--job extract --smart --delete-archive %f",
        );
    }
    if filter.extract_to_folder_del {
        push(
            &mut actions,
            "ExtractToFolderDel",
            t(Lang::En, "shell.extract_folder_del"),
            t(Lang::ZhCn, "shell.extract_folder_del"),
            "--job extract --archive-name-dir --delete-archive %f",
        );
    }
    if filter.extract_dialog {
        push(
            &mut actions,
            "ExtractDialog",
            t(Lang::En, "shell.extract_dialog"),
            t(Lang::ZhCn, "shell.extract_dialog"),
            "--job extract --dialog %f",
        );
    }
    if filter.open_with {
        push(
            &mut actions,
            "OpenWith",
            t(Lang::En, "shell.open_with"),
            t(Lang::ZhCn, "shell.open_with"),
            "%f",
        );
    }
    if filter.compress_zip {
        push(
            &mut actions,
            "CompressZip",
            t(Lang::En, "shell.compress_zip"),
            t(Lang::ZhCn, "shell.compress_zip"),
            "--job create --beside -f zip %F",
        );
    }
    if filter.compress_7z {
        push(
            &mut actions,
            "Compress7z",
            t(Lang::En, "shell.compress_7z"),
            t(Lang::ZhCn, "shell.compress_7z"),
            "--job create --beside -f 7z %F",
        );
    }
    if filter.compress_zipx {
        push(
            &mut actions,
            "CompressZipx",
            t(Lang::En, "shell.compress_zipx"),
            t(Lang::ZhCn, "shell.compress_zipx"),
            "--job create --beside -f zipx %F",
        );
    }
    if filter.compress_tar {
        push(
            &mut actions,
            "CompressTar",
            t(Lang::En, "shell.compress_tar"),
            t(Lang::ZhCn, "shell.compress_tar"),
            "--job create --beside -f tar %F",
        );
    }
    if filter.compress_tgz {
        push(
            &mut actions,
            "CompressTgz",
            t(Lang::En, "shell.compress_tgz"),
            t(Lang::ZhCn, "shell.compress_tgz"),
            "--job create --beside -f tgz %F",
        );
    }
    if filter.compress_gz {
        push(
            &mut actions,
            "CompressGz",
            t(Lang::En, "shell.compress_gz"),
            t(Lang::ZhCn, "shell.compress_gz"),
            "--job create --beside -f gz %F",
        );
    }
    if filter.compress_xz {
        push(
            &mut actions,
            "CompressXz",
            t(Lang::En, "shell.compress_xz"),
            t(Lang::ZhCn, "shell.compress_xz"),
            "--job create --beside -f xz %F",
        );
    }
    if filter.compress_zpaq {
        push(
            &mut actions,
            "CompressZpaq",
            t(Lang::En, "shell.compress_zpaq"),
            t(Lang::ZhCn, "shell.compress_zpaq"),
            "--job create --beside -f zpaq %F",
        );
    }
    if filter.compress_each_zip {
        push(
            &mut actions,
            "CompressEachZip",
            t(Lang::En, "shell.compress_each_zip"),
            t(Lang::ZhCn, "shell.compress_each_zip"),
            "--job create --beside --each -f zip %F",
        );
    }
    if filter.compress_each_7z {
        push(
            &mut actions,
            "CompressEach7z",
            t(Lang::En, "shell.compress_each_7z"),
            t(Lang::ZhCn, "shell.compress_each_7z"),
            "--job create --beside --each -f 7z %F",
        );
    }
    let action_ids = actions
        .iter()
        .map(|a| a.id)
        .collect::<Vec<_>>()
        .join(";");
    let mut out = format!(
        r#"[Desktop Entry]
Type=Service
ServiceTypes=KonqPopupMenu/Plugin
MimeType={mimes}
Actions={action_ids};
X-KDE-Priority=TopLevel
"#,
        mimes = mime,
        action_ids = action_ids,
    );
    for action in &actions {
        out.push_str(&format!(
            "\n[Desktop Action {id}]\nName={name_en}\nName[zh_CN]={name_zh}\nExec={gui} {exec_suffix}\n",
            id = action.id,
            name_en = action.name_en,
            name_zh = action.name_zh,
            gui = gui_s,
            exec_suffix = action.exec_suffix,
        ));
    }
    out
}

fn sync_nautilus_script(
    scripts: &Path,
    name: &str,
    gui: &Path,
    args: Option<&str>,
) -> Result<(), String> {
    let path = scripts.join(name);
    if let Some(args) = args {
        let body = if args.is_empty() {
            format!(
                "#!/bin/sh\n# Oxizip Nautilus script\nexec {} \"$@\"\n",
                shell_quote(&gui.display().to_string()),
            )
        } else {
            nautilus_script(gui, args)
        };
        write_file(&path, &body)?;
    } else {
        let _ = fs::remove_file(&path);
    }
    Ok(())
}

fn sync_nautilus_scripts(scripts: &Path, gui: &Path, filter: &MenuFilter) -> Result<(), String> {
    let on = |flag: bool, args: &'static str| -> Option<&'static str> {
        if flag && !filter.disabled {
            Some(args)
        } else {
            None
        }
    };
    sync_nautilus_script(
        scripts,
        "Oxizip Extract Here",
        gui,
        on(filter.extract_here, "--job extract --here"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Extract Smart",
        gui,
        on(filter.extract_smart, "--job extract --smart"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Extract To Folder",
        gui,
        on(filter.extract_to_folder, "--job extract --archive-name-dir"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Extract Here and Delete",
        gui,
        on(
            filter.extract_here_del,
            "--job extract --here --delete-archive",
        ),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Extract Smart and Delete",
        gui,
        on(
            filter.extract_smart_del,
            "--job extract --smart --delete-archive",
        ),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Extract To Folder and Delete",
        gui,
        on(
            filter.extract_to_folder_del,
            "--job extract --archive-name-dir --delete-archive",
        ),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Extract to Another Folder",
        gui,
        on(filter.extract_dialog, "--job extract --dialog"),
    )?;
    sync_nautilus_script(scripts, "Oxizip Open", gui, on(filter.open_with, ""))?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress ZIP",
        gui,
        on(filter.compress_zip, "--job create --beside -f zip"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress 7z",
        gui,
        on(filter.compress_7z, "--job create --beside -f 7z"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress ZIPX",
        gui,
        on(filter.compress_zipx, "--job create --beside -f zipx"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress TAR",
        gui,
        on(filter.compress_tar, "--job create --beside -f tar"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress TGZ",
        gui,
        on(filter.compress_tgz, "--job create --beside -f tgz"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress GZ",
        gui,
        on(filter.compress_gz, "--job create --beside -f gz"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress XZ",
        gui,
        on(filter.compress_xz, "--job create --beside -f xz"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress ZPAQ",
        gui,
        on(filter.compress_zpaq, "--job create --beside -f zpaq"),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress Each ZIP",
        gui,
        on(
            filter.compress_each_zip,
            "--job create --beside --each -f zip",
        ),
    )?;
    sync_nautilus_script(
        scripts,
        "Oxizip Compress Each 7z",
        gui,
        on(filter.compress_each_7z, "--job create --beside --each -f 7z"),
    )?;
    Ok(())
}

fn build_mime_package(exts: &[&str]) -> Option<String> {
    let needed: Vec<_> = custom_mime_defs()
        .iter()
        .filter(|(mime, _, _)| exts.iter().any(|ext| mimes_for_ext(ext).contains(mime)))
        .collect();
    if needed.is_empty() {
        return None;
    }
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
"#,
    );
    for (mime, comment, globs) in needed {
        xml.push_str(&format!(
            "  <mime-type type=\"{mime}\">\n    <comment>{comment}</comment>\n"
        ));
        for glob in *globs {
            xml.push_str(&format!("    <glob pattern=\"{glob}\"/>\n"));
        }
        xml.push_str("  </mime-type>\n");
    }
    xml.push_str("</mime-info>\n");
    Some(xml)
}

fn refresh_desktop_db(apps: &Path) {
    let _ = Command::new("update-desktop-database").arg(apps).status();
}

fn refresh_mime_db(home: &Path) {
    let mime_dir = home.join(".local/share/mime");
    let _ = Command::new("update-mime-database").arg(&mime_dir).status();
}

fn xdg_mime_default(desktop_id: &str, mime: &str) {
    let _ = Command::new("xdg-mime")
        .args(["default", desktop_id, mime])
        .status();
}

/// Remove Oxizip defaults we previously claimed from `mimeapps.list`.
fn clear_oxizip_defaults(home: &Path) {
    let path = config_dir(home).join("mimeapps.list");
    let Ok(file) = fs::File::open(&path) else {
        return;
    };
    let reader = std::io::BufReader::new(file);
    let mut out = Vec::new();
    let mut changed = false;
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.contains(OPEN_DESKTOP_ID) {
            // Drop lines that only point at our desktop id.
            if let Some((_, rhs)) = trimmed.split_once('=') {
                let keep = rhs
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && *s != OPEN_DESKTOP_ID)
                    .collect::<Vec<_>>();
                if keep.is_empty() {
                    changed = true;
                    continue;
                }
                if keep.join(";") != rhs.trim_matches(';') {
                    let key = trimmed.split('=').next().unwrap_or("");
                    out.push(format!("{key}={};", keep.join(";")));
                    changed = true;
                    continue;
                }
            }
        }
        out.push(line);
    }
    if changed {
        if let Ok(mut f) = fs::File::create(&path) {
            for line in out {
                let _ = writeln!(f, "{line}");
            }
        }
    }
}

pub fn install(lang: Lang, requested_exts: &[String], menus_only: bool) -> Result<String, String> {
    let exts: Vec<&str> = if menus_only {
        // Menus cover all archive types; default-open is left to the association UI.
        archive_extensions().to_vec()
    } else {
        resolve_install_exts(requested_exts)?
    };
    let mimes = collect_mimes(&exts);
    if mimes.is_empty() {
        return Err("no MIME types for selected extensions".into());
    }

    let cli = current_oxizip()?;
    let gui = current_oxizip_gui().unwrap_or_else(|_| cli.clone());
    let home = home_dir()?;
    let apps = applications_dir(&home);
    let filter = load_menu_filter(&home);

    if !menus_only {
        // Clean previous default claims before rewriting the desktop + mimeapps entries.
        clear_oxizip_defaults(&home);
        write_file(&apps.join(OPEN_DESKTOP_ID), &open_desktop(&gui, &mimes))?;
    }

    if !filter.disabled {
        write_file(
            &apps.join("oxizip-extract.desktop"),
            &extract_desktop(&cli, &gui, &mimes),
        )?;
        write_file(
            &apps.join("oxizip-compress.desktop"),
            &compress_desktop(&gui),
        )?;
    } else {
        let _ = fs::remove_file(apps.join("oxizip-extract.desktop"));
        let _ = fs::remove_file(apps.join("oxizip-compress.desktop"));
    }

    if !menus_only {
        let mime_pkg = mime_packages_dir(&home).join(MIME_PACKAGE_NAME);
        if let Some(xml) = build_mime_package(&exts) {
            write_file(&mime_pkg, &xml)?;
            refresh_mime_db(&home);
        } else {
            let _ = fs::remove_file(&mime_pkg);
            refresh_mime_db(&home);
        }
    }

    let scripts = nautilus_scripts_dir(&home);
    let _ = fs::create_dir_all(&scripts);
    sync_nautilus_scripts(&scripts, &gui, &filter)?;

    let kde = kde_servicemenus_dir(&home).join("oxizip.desktop");
    if !filter.disabled {
        write_file(&kde, &kde_servicemenu(&gui, &mimes, &filter))?;
    } else {
        let _ = fs::remove_file(&kde);
    }

    refresh_desktop_db(&apps);
    if !menus_only {
        for mime in &mimes {
            xdg_mime_default(OPEN_DESKTOP_ID, mime);
        }
    }

    Ok(format!(
        "{} ({}exts={} ; gui={} ; cli={})",
        t(lang, "shell.install_ok"),
        if menus_only { "menus-only; " } else { "" },
        exts.join(","),
        gui.display(),
        cli.display()
    ))
}

pub fn uninstall(lang: Lang) -> Result<String, String> {
    let home = home_dir()?;
    clear_oxizip_defaults(&home);
    for path in marker_files(&home) {
        let _ = fs::remove_file(path);
    }
    refresh_mime_db(&home);
    refresh_desktop_db(&applications_dir(&home));
    Ok(t(lang, "shell.uninstall_ok").to_string())
}

pub fn status(lang: Lang) -> Result<String, String> {
    let home = home_dir()?;
    let installed = applications_dir(&home).join(OPEN_DESKTOP_ID).is_file();
    if installed {
        Ok(t(lang, "shell.status_installed").to_string())
    } else {
        Ok(t(lang, "shell.status_missing").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_mimes_include_standard_types() {
        let m = mimes_for_ext("zip");
        assert!(m.contains(&"application/zip"));
    }

    #[test]
    fn collect_selected_exts_only() {
        let mimes = collect_mimes(&["7z", "zpaq"]);
        assert!(mimes.contains(&"application/x-7z-compressed"));
        assert!(mimes.contains(&"application/x-zpaq"));
        assert!(!mimes.iter().any(|m| *m == "application/zip"));
    }
}
