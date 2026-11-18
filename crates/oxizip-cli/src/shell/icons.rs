//! Resolve / stage format `.ico` files for shell DefaultIcon and verb Icon values.

use std::fs;
use std::path::{Path, PathBuf};

/// Icon file stem for a registered archive extension (without `.ico`).
pub fn icon_stem_for_ext(ext: &str) -> &'static str {
    match ext {
        "zip" | "jar" | "exe" => "zip",
        "zipx" => "zipx",
        "7z" => "7z",
        "rar" => "rar",
        "tar" => "tar",
        "tgz" => "tgz",
        "gz" => "gz",
        "xz" | "zst" | "txz" => "xz",
        "bz2" => "bz2",
        "tbz" => "tbz",
        "tbz2" => "tbz2",
        "lzh" => "lzh",
        "lha" => "lha",
        "iso" => "iso",
        "zpaq" => "zpaq",
        "cab" => "cab",
        "alz" => "alz",
        "egg" => "egg",
        // No dedicated icon yet — fall back to product icon.
        "wim" | "esd" | "swm" | "001" => "oxizip",
        _ => "oxizip",
    }
}

const ICON_FILES: &[&str] = &[
    "oxizip.ico",
    "zip.ico",
    "zipx.ico",
    "7z.ico",
    "rar.ico",
    "tar.ico",
    "tgz.ico",
    "gz.ico",
    "xz.ico",
    "bz2.ico",
    "tbz.ico",
    "tbz2.ico",
    "lzh.ico",
    "lha.ico",
    "iso.ico",
    "zpaq.ico",
    "cab.ico",
    "alz.ico",
    "egg.ico",
];

/// Ensure icons live next to the oxizip binary and return that directory.
pub fn ensure_icons_dir(exe: &Path) -> Result<PathBuf, String> {
    let dest = exe
        .parent()
        .ok_or_else(|| "oxizip has no parent directory".to_string())?
        .join("assets")
        .join("icons");
    fs::create_dir_all(&dest).map_err(|e| format!("create icons dir {}: {e}", dest.display()))?;

    let src = find_source_icons_dir(exe);
    for name in ICON_FILES {
        let target = dest.join(name);
        if target.is_file() {
            continue;
        }
        if let Some(ref src_dir) = src {
            let from = src_dir.join(name);
            if from.is_file() {
                fs::copy(&from, &target)
                    .map_err(|e| format!("copy {} -> {}: {e}", from.display(), target.display()))?;
            }
        }
    }

    if !dest.join("oxizip.ico").is_file() {
        return Err(format!(
            "missing oxizip.ico under {} (source={:?})",
            dest.display(),
            src
        ));
    }
    Ok(dest)
}

/// Absolute path to the ico used for `ext`, falling back to `oxizip.ico`.
pub fn icon_path_for_ext(icons_dir: &Path, ext: &str) -> PathBuf {
    let stem = icon_stem_for_ext(ext);
    let preferred = icons_dir.join(format!("{stem}.ico"));
    if preferred.is_file() {
        preferred
    } else {
        icons_dir.join("oxizip.ico")
    }
}

fn find_source_icons_dir(exe: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(dir) = exe.parent() {
        candidates.push(dir.join("assets").join("icons"));
        candidates.push(dir.join("icons"));
        // cargo run: target/{debug,release}/oxizip.exe → walk up to workspace
        for ancestor in dir.ancestors().take(8) {
            candidates.push(
                ancestor
                    .join("crates")
                    .join("oxizip-gui")
                    .join("assets")
                    .join("icons"),
            );
            candidates.push(ancestor.join("assets").join("icons"));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(8) {
            candidates.push(
                ancestor
                    .join("crates")
                    .join("oxizip-gui")
                    .join("assets")
                    .join("icons"),
            );
        }
    }

    candidates.into_iter().find(|p| p.join("oxizip.ico").is_file())
}
