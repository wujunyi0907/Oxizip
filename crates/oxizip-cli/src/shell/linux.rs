//! FreeDesktop / Nautilus / KDE service-menu integration.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use oxizip_i18n::{t, Lang};

use super::current_oxizip;

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())
}

fn applications_dir(home: &Path) -> PathBuf {
    home.join(".local/share/applications")
}

fn nautilus_scripts_dir(home: &Path) -> PathBuf {
    home.join(".local/share/nautilus/scripts")
}

fn kde_servicemenus_dir(home: &Path) -> PathBuf {
    home.join(".local/share/kio/servicemenus")
}

fn marker_files(home: &Path) -> Vec<PathBuf> {
    vec![
        applications_dir(home).join("oxizip-extract.desktop"),
        applications_dir(home).join("oxizip-compress.desktop"),
        nautilus_scripts_dir(home).join("Oxizip Extract Here"),
        nautilus_scripts_dir(home).join("Oxizip Extract Smart"),
        nautilus_scripts_dir(home).join("Oxizip Compress ZIP"),
        kde_servicemenus_dir(home).join("oxizip.desktop"),
    ]
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

fn extract_desktop(exe: &Path) -> String {
    let exe_s = exe.display();
    format!(
        r#"[Desktop Entry]
Type=Application
Version=1.0
Name={name_en}
Name[zh_CN]={name_zh}
Comment=Extract archives with Oxizip
Exec={exe} extract --smart %f
Icon=package-x-generic
Terminal=false
Categories=Utility;Archiving;Compression;
MimeType=application/zip;application/x-zip-compressed;application/x-7z-compressed;application/vnd.rar;application/x-rar-compressed;application/x-tar;application/gzip;application/x-xz;application/zstd;application/x-bzip2;application/x-iso9660-image;application/x-lzh-compressed;
Actions=ExtractHere;ExtractSmart;ExtractToFolder;Test;Checksum;

    [Desktop Action ExtractHere]
Name={here_en}
Name[zh_CN]={here_zh}
Exec={exe} extract --here %f

[Desktop Action ExtractSmart]
Name={smart_en}
Name[zh_CN]={smart_zh}
Exec={exe} extract --smart %f

[Desktop Action ExtractToFolder]
Name={folder_en}
Name[zh_CN]={folder_zh}
Exec={exe} extract --archive-name-dir %f

[Desktop Action Test]
Name={test_en}
Name[zh_CN]={test_zh}
Exec={exe} test %f

[Desktop Action Checksum]
Name={sum_en}
Name[zh_CN]={sum_zh}
Exec={exe} checksum %f
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
        exe = exe_s,
    )
}

fn compress_desktop(exe: &Path) -> String {
    let exe_s = exe.display();
    format!(
        r#"[Desktop Entry]
Type=Application
Version=1.0
Name={name_en}
Name[zh_CN]={name_zh}
Comment=Compress with Oxizip
Exec={exe} create --beside -f zip %F
Icon=package-x-generic
Terminal=false
Categories=Utility;Archiving;Compression;
MimeType=application/octet-stream;inode/directory;
Actions=CompressZip;Compress7z;

[Desktop Action CompressZip]
Name={zip_en}
Name[zh_CN]={zip_zh}
Exec={exe} create --beside -f zip %F

[Desktop Action Compress7z]
Name={z7_en}
Name[zh_CN]={z7_zh}
Exec={exe} create --beside -f 7z %F
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

fn kde_servicemenu(exe: &Path) -> String {
    let exe_s = exe.display();
    format!(
        r#"[Desktop Entry]
Type=Service
ServiceTypes=KonqPopupMenu/Plugin
MimeType=application/zip;application/x-7z-compressed;application/vnd.rar;application/x-tar;inode/directory;application/octet-stream;
Actions=ExtractHere;ExtractSmart;CompressZip;Compress7z;
X-KDE-Priority=TopLevel

[Desktop Action ExtractHere]
Name=Oxizip Extract Here
Name[zh_CN]=Oxizip 解压到此处
Exec={exe} extract --here %f

[Desktop Action ExtractSmart]
Name=Oxizip Extract Smart
Name[zh_CN]=Oxizip 智能解压
Exec={exe} extract --smart %f

[Desktop Action CompressZip]
Name=Oxizip Compress ZIP
Name[zh_CN]=Oxizip 压缩为 ZIP
Exec={exe} create --beside -f zip %F

[Desktop Action Compress7z]
Name=Oxizip Compress 7z
Name[zh_CN]=Oxizip 压缩为 7z
Exec={exe} create --beside -f 7z %F
"#,
        exe = exe_s,
    )
}

fn refresh_desktop_db(apps: &Path) {
    let _ = Command::new("update-desktop-database")
        .arg(apps)
        .status();
}

pub fn install(lang: Lang) -> Result<String, String> {
    let exe = current_oxizip()?;
    let home = home_dir()?;
    let apps = applications_dir(&home);
    write_file(
        &apps.join("oxizip-extract.desktop"),
        &extract_desktop(&exe),
    )?;
    write_file(
        &apps.join("oxizip-compress.desktop"),
        &compress_desktop(&exe),
    )?;

    let scripts = nautilus_scripts_dir(&home);
    write_file(
        &scripts.join("Oxizip Extract Here"),
        &nautilus_script(&exe, "extract --here"),
    )?;
    write_file(
        &scripts.join("Oxizip Extract Smart"),
        &nautilus_script(&exe, "extract --smart"),
    )?;
    write_file(
        &scripts.join("Oxizip Compress ZIP"),
        &nautilus_script(&exe, "create --beside -f zip"),
    )?;

    write_file(
        &kde_servicemenus_dir(&home).join("oxizip.desktop"),
        &kde_servicemenu(&exe),
    )?;

    refresh_desktop_db(&apps);
    Ok(format!(
        "{} ({})",
        t(lang, "shell.install_ok"),
        exe.display()
    ))
}

pub fn uninstall(lang: Lang) -> Result<String, String> {
    let home = home_dir()?;
    for path in marker_files(&home) {
        let _ = fs::remove_file(path);
    }
    refresh_desktop_db(&applications_dir(&home));
    Ok(t(lang, "shell.uninstall_ok").to_string())
}

pub fn status(lang: Lang) -> Result<String, String> {
    let home = home_dir()?;
    let installed = applications_dir(&home)
        .join("oxizip-extract.desktop")
        .is_file();
    if installed {
        Ok(t(lang, "shell.status_installed").to_string())
    } else {
        Ok(t(lang, "shell.status_missing").to_string())
    }
}
