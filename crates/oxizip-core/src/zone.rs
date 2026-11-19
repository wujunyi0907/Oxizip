//! Windows Zone.Identifier (Mark-of-the-Web) propagation.
//!
//! On non-Windows platforms these helpers are no-ops so callers can stay
//! platform-agnostic.

use std::path::Path;

use crate::error::CoreResult;

/// Read the Zone.Identifier ADS attached to `path`, if any.
#[cfg(windows)]
pub fn read_zone_identifier(path: &Path) -> Option<Vec<u8>> {
    let ads = ads_path(path)?;
    std::fs::read(&ads).ok().filter(|b| !b.is_empty())
}

#[cfg(not(windows))]
pub fn read_zone_identifier(_path: &Path) -> Option<Vec<u8>> {
    None
}

/// Write Zone.Identifier ADS onto `path`.
#[cfg(windows)]
pub fn write_zone_identifier(path: &Path, data: &[u8]) -> CoreResult<()> {
    if data.is_empty() {
        return Ok(());
    }
    if let Some(ads) = ads_path(path) {
        std::fs::write(&ads, data)?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn write_zone_identifier(_path: &Path, _data: &[u8]) -> CoreResult<()> {
    Ok(())
}

/// Copy the archive's Zone.Identifier onto every regular file under `dest_dir`.
pub fn propagate_zone_identifier(archive: &Path, dest_dir: &Path) -> CoreResult<()> {
    let Some(data) = read_zone_identifier(archive) else {
        return Ok(());
    };
    walk_and_tag(dest_dir, &data)
}

#[cfg(windows)]
fn ads_path(path: &Path) -> Option<std::path::PathBuf> {
    // NTFS ADS: `file:Zone.Identifier`
    let mut s = path.as_os_str().to_os_string();
    s.push(":Zone.Identifier");
    Some(std::path::PathBuf::from(s))
}

fn walk_and_tag(dir: &Path, data: &[u8]) -> CoreResult<()> {
    if !dir.is_dir() {
        if dir.is_file() {
            write_zone_identifier(dir, data)?;
        }
        return Ok(());
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let entries = match std::fs::read_dir(&cur) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                write_zone_identifier(&path, data)?;
            }
        }
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn roundtrip_zone_ads() {
        let path =
            std::env::temp_dir().join(format!("oxizip-zone-test-{}.bin", std::process::id()));
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"x").unwrap();
        }
        let payload = b"[ZoneTransfer]\r\nZoneId=3\r\n";
        write_zone_identifier(&path, payload).unwrap();
        let got = read_zone_identifier(&path).unwrap();
        assert_eq!(got, payload);
        let _ = std::fs::remove_file(&path);
    }
}
