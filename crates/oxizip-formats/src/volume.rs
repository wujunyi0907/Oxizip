//! Raw byte-split multi-volume archives (`.001` / `.002` / …), 7-Zip style.

use std::fs::{self, File};
use std::io::{copy, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use oxizip_core::{CoreError, CoreResult};
use tempfile::NamedTempFile;

/// True if `path` looks like a split volume (`.001` or `.7z.001` / `.zip.001`).
pub fn is_volume_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if let Some(ext) = name.rsplit('.').next() {
        if ext.len() == 3 && ext.chars().all(|c| c.is_ascii_digit()) {
            let n: u32 = ext.parse().unwrap_or(0);
            return (1..=999).contains(&n);
        }
    }
    false
}

/// Resolve the first volume path for joining (normalize `.002` → `.001`).
pub fn first_volume_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if let Some((stem, ext)) = name.rsplit_once('.') {
        if ext.len() == 3 && ext.chars().all(|c| c.is_ascii_digit()) {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            return parent.join(format!("{stem}.001"));
        }
    }
    path.to_path_buf()
}

fn volume_path_for_stem(stem: &Path, index: u32) -> PathBuf {
    let parent = stem.parent().unwrap_or_else(|| Path::new("."));
    let name = stem
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".into());
    parent.join(format!("{name}.{index:03}"))
}

/// Concatenate `.001`, `.002`, … into a temp file. Keep the returned handle alive while reading.
pub fn join_volumes(first: &Path) -> CoreResult<NamedTempFile> {
    let first = first_volume_path(first);
    if !first.is_file() {
        return Err(CoreError::Other(format!(
            "volume not found: {}",
            first.display()
        )));
    }
    let mut out = NamedTempFile::new().map_err(CoreError::Io)?;
    let mut part: u32 = 1;
    loop {
        let part_path = {
            let name = first.file_name().and_then(|s| s.to_str()).unwrap_or("a.001");
            let (stem, _) = name.rsplit_once('.').unwrap_or((name, "001"));
            first
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{stem}.{part:03}"))
        };
        if !part_path.is_file() {
            if part == 1 {
                return Err(CoreError::Other(format!(
                    "volume not found: {}",
                    part_path.display()
                )));
            }
            break;
        }
        let mut input = BufReader::new(File::open(&part_path)?);
        copy(&mut input, out.as_file_mut())?;
        part += 1;
    }
    out.as_file_mut().flush()?;
    out.as_file_mut()
        .seek(SeekFrom::Start(0))
        .map_err(CoreError::Io)?;
    Ok(out)
}

/// Split `src` into `dest_stem.001`, `.002`, … of at most `volume_size` bytes.
pub fn split_into_volumes(src: &Path, dest_stem: &Path, volume_size: u64) -> CoreResult<Vec<PathBuf>> {
    if volume_size == 0 {
        return Err(CoreError::Other("volume size must be > 0".into()));
    }
    let mut file = File::open(src)?;
    let total = file.metadata()?.len();
    if total == 0 {
        return Err(CoreError::Other("nothing to split".into()));
    }
    let mut out_paths = Vec::new();
    let mut offset = 0u64;
    let mut index = 1u32;
    let mut buf = vec![0u8; (1024 * 1024).min(volume_size as usize).max(64 * 1024)];
    while offset < total {
        let part = volume_path_for_stem(dest_stem, index);
        if let Some(parent) = part.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let mut out = BufWriter::new(File::create(&part)?);
        let mut remaining = volume_size.min(total - offset);
        while remaining > 0 {
            let n = (remaining as usize).min(buf.len());
            file.read_exact(&mut buf[..n])?;
            out.write_all(&buf[..n])?;
            remaining -= n as u64;
            offset += n as u64;
        }
        out.flush()?;
        out_paths.push(part);
        index += 1;
    }
    Ok(out_paths)
}

/// After creating a monolithic archive at `archive`, optionally split it.
/// When multiple volumes are produced, the monolithic file is removed.
pub fn maybe_split_archive(archive: &Path, volume_size: Option<u64>) -> CoreResult<()> {
    let Some(size) = volume_size else {
        return Ok(());
    };
    let parts = split_into_volumes(archive, archive, size)?;
    if parts.len() == 1 {
        let only = &parts[0];
        if only != archive {
            let _ = fs::remove_file(archive);
            fs::rename(only, archive)?;
        }
        return Ok(());
    }
    fs::remove_file(archive)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn split_and_join_roundtrip() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("data.bin");
        let payload = (0u8..200).collect::<Vec<_>>();
        fs::write(&src, &payload).unwrap();
        let stem = dir.path().join("pack.bin");
        let parts = split_into_volumes(&src, &stem, 64).unwrap();
        assert!(parts.len() > 1);
        assert!(is_volume_path(&parts[0]));
        let joined = join_volumes(&parts[0]).unwrap();
        let bytes = fs::read(joined.path()).unwrap();
        assert_eq!(bytes, payload);
    }
}
