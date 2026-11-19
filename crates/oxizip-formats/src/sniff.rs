use std::fs::File;
use std::io::Read;
use std::path::Path;

use oxizip_core::{CoreError, CoreResult};

use crate::types::FormatId;

const SNIFF_LEN: usize = 512;

/// Detect archive format from magic bytes and optional filename hints.
pub fn sniff_path(path: &Path) -> CoreResult<FormatId> {
    let mut file = File::open(path)?;
    let mut buf = [0u8; SNIFF_LEN];
    let n = file.read(&mut buf)?;
    sniff_bytes(&buf[..n], Some(path)).ok_or(CoreError::UnsupportedFormat)
}

pub fn sniff_bytes(header: &[u8], path: Option<&Path>) -> Option<FormatId> {
    if let Some(id) = sniff_magic(header) {
        // Prefer compound tar.* when extension says so and magic is a compressor.
        if let Some(path) = path {
            if let Some(from_name) = sniff_extension(path) {
                return Some(reconcile(id, from_name));
            }
        }
        return Some(id);
    }
    path.and_then(sniff_extension)
}

fn sniff_magic(header: &[u8]) -> Option<FormatId> {
    if header.starts_with(&[0x50, 0x4B, 0x03, 0x04])
        || header.starts_with(&[0x50, 0x4B, 0x05, 0x06])
        || header.starts_with(&[0x50, 0x4B, 0x07, 0x08])
    {
        return Some(FormatId::Zip);
    }
    if header.starts_with(&[0x1F, 0x8B]) {
        return Some(FormatId::Gz);
    }
    if header.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
        return Some(FormatId::Xz);
    }
    if header.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return Some(FormatId::Zst);
    }
    if header.starts_with(b"BZh") {
        return Some(FormatId::Bz2);
    }
    if header.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some(FormatId::SevenZ);
    }
    if header.starts_with(b"Rar!\x1A\x07\x00") || header.starts_with(b"Rar!\x1A\x07\x01\x00") {
        return Some(FormatId::Rar);
    }
    // Microsoft Cabinet
    if header.starts_with(b"MSCF") {
        return Some(FormatId::Cab);
    }
    // Microsoft WIM
    if header.starts_with(b"MSWIM\0\0") || header.starts_with(b"WLPWM\0\0") {
        return Some(FormatId::Wim);
    }
    // ALZip ALZ
    if header.starts_with(b"ALZ\x01") {
        return Some(FormatId::Alz);
    }
    // ESTsoft EGG ("EGGA" little-endian = 0x41474745)
    if header.starts_with(b"EGGA") {
        return Some(FormatId::Egg);
    }
    // ustar tar
    if header.len() >= 265 && &header[257..262] == b"ustar" {
        return Some(FormatId::Tar);
    }
    // ISO 9660 primary volume descriptor at sector 16 typically; also accept CD001 anywhere early
    if header.windows(5).any(|w| w == b"CD001") {
        return Some(FormatId::Iso);
    }
    // LZH level-0/1 header starts with header size then method like -lh0-
    if header.len() >= 7 && header[2] == b'-' && header[3] == b'l' && header[6] == b'-' {
        return Some(FormatId::Lzh);
    }
    None
}

fn sniff_extension(path: &Path) -> Option<FormatId> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Some(FormatId::TarGz);
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        return Some(FormatId::TarXz);
    }
    if name.ends_with(".tar.zst") || name.ends_with(".tar.zstd") {
        return Some(FormatId::TarZst);
    }
    if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
        return Some(FormatId::TarBz2);
    }
    if name.ends_with(".tar") {
        return Some(FormatId::Tar);
    }
    if name.ends_with(".zipx") {
        return Some(FormatId::Zipx);
    }
    if name.ends_with(".zip") || name.ends_with(".jar") {
        return Some(FormatId::Zip);
    }
    if name.ends_with(".gz") {
        return Some(FormatId::Gz);
    }
    if name.ends_with(".xz") {
        return Some(FormatId::Xz);
    }
    if name.ends_with(".zst") || name.ends_with(".zstd") {
        return Some(FormatId::Zst);
    }
    if name.ends_with(".bz2") {
        return Some(FormatId::Bz2);
    }
    if name.ends_with(".7z") {
        return Some(FormatId::SevenZ);
    }
    if name.ends_with(".lzh") || name.ends_with(".lha") {
        return Some(FormatId::Lzh);
    }
    if name.ends_with(".iso") {
        return Some(FormatId::Iso);
    }
    if name.ends_with(".zpaq") {
        return Some(FormatId::Zpaq);
    }
    if name.ends_with(".exe") {
        return Some(FormatId::ExeSfx);
    }
    if name.ends_with(".rar") {
        return Some(FormatId::Rar);
    }
    if name.ends_with(".cab") {
        return Some(FormatId::Cab);
    }
    if name.ends_with(".wim") || name.ends_with(".esd") || name.ends_with(".swm") {
        return Some(FormatId::Wim);
    }
    if name.ends_with(".alz") {
        return Some(FormatId::Alz);
    }
    if name.ends_with(".egg") {
        return Some(FormatId::Egg);
    }
    None
}

fn reconcile(magic: FormatId, from_name: FormatId) -> FormatId {
    match (magic, from_name) {
        (FormatId::Gz, FormatId::TarGz) => FormatId::TarGz,
        (FormatId::Xz, FormatId::TarXz) => FormatId::TarXz,
        (FormatId::Zst, FormatId::TarZst) => FormatId::TarZst,
        (FormatId::Bz2, FormatId::TarBz2) => FormatId::TarBz2,
        (FormatId::Zip, FormatId::Zipx) => FormatId::Zipx,
        (FormatId::Zip, FormatId::ExeSfx) => FormatId::ExeSfx,
        _ => match magic {
            FormatId::Zip | FormatId::SevenZ | FormatId::Rar | FormatId::Tar | FormatId::Iso => {
                magic
            }
            _ => from_name,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn detects_zip_magic() {
        let h = [0x50, 0x4B, 0x03, 0x04, 0, 0, 0, 0];
        assert_eq!(sniff_bytes(&h, None), Some(FormatId::Zip));
    }

    #[test]
    fn tar_gz_from_name_and_gzip_magic() {
        let h = [0x1F, 0x8B, 0x08, 0];
        let p = Path::new("archive.tar.gz");
        assert_eq!(sniff_bytes(&h, Some(p)), Some(FormatId::TarGz));
    }
}
