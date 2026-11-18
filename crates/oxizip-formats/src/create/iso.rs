//! Minimal ISO 9660 Level-1 writer (single-extent files, 8.3-ish names uppercased).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};

use crate::traits::ArchiveWriter;
use crate::types::{CreateOptions, FormatId};
use crate::util::{collect_pack_files, ensure_parent};

const SECTOR: usize = 2048;

pub struct IsoWriter {
    path: PathBuf,
    files: Vec<(PathBuf, String)>,
}

impl IsoWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Self {
            path,
            files: Vec::new(),
        })
    }
}

impl ArchiveWriter for IsoWriter {
    fn format(&self) -> FormatId {
        FormatId::Iso
    }

    fn add_path(
        &mut self,
        path: &Path,
        _opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        let packed = collect_pack_files(&[path], cancel)?;
        for f in packed {
            progress.report(ProgressUpdate {
                stage: JobStage::Create,
                bytes_done: 0,
                bytes_total: None,
                entries_done: 0,
                entries_total: None,
                current_entry: Some(f.name.clone()),
                message: None,
            });
            let base = Path::new(&f.name)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(f.name);
            self.files.push((f.source, iso_name(&base)?));
        }
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        _progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        write_iso(&self.path, &self.files, cancel)
    }
}

fn iso_name(name: &str) -> CoreResult<String> {
    let upper = name.to_ascii_uppercase();
    let cleaned: String = upper
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        return Err(CoreError::Other("empty iso name".into()));
    }
    // Keep within common ISO name length; not strict 8.3 to stay usable.
    Ok(if cleaned.len() > 30 {
        cleaned[..30].to_string()
    } else {
        cleaned
    })
}

fn write_iso(path: &Path, files: &[(PathBuf, String)], cancel: &CancellationToken) -> CoreResult<()> {
    cancel.check()?;
    let mut payloads = Vec::new();
    for (src, name) in files {
        cancel.check()?;
        let mut data = Vec::new();
        File::open(src)?.read_to_end(&mut data)?;
        let sectors = data.len().div_ceil(SECTOR);
        payloads.push((name.clone(), data, sectors));
    }

    // Layout:
    // LBA 0..15 system area
    // LBA 16 PVD
    // LBA 17 terminator
    // LBA 18 path table (LE)
    // LBA 19 path table (BE) optional duplicate
    // LBA 20 root directory
    // LBA 21+ file data
    let root_lba: u32 = 20;
    let mut next_lba: u32 = 21;
    let mut file_records = Vec::new();
    for (name, data, sectors) in &payloads {
        file_records.push((name.clone(), next_lba, data.len() as u32, data.clone()));
        next_lba += *sectors as u32;
    }
    let volume_sectors = next_lba;

    let mut out = File::create(path)?;
    // System area
    out.write_all(&vec![0u8; SECTOR * 16])?;

    // PVD
    let mut pvd = vec![0u8; SECTOR];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    write_str(&mut pvd[40..72], "OXIZIP");
    write_both_endian_u32(&mut pvd[80..88], volume_sectors);
    write_both_endian_u16(&mut pvd[120..124], 1); // volume set size
    write_both_endian_u16(&mut pvd[124..128], 1); // volume sequence
    write_both_endian_u16(&mut pvd[128..132], SECTOR as u16);
    write_both_endian_u32(&mut pvd[132..140], 1); // path table size (sectors*records simplified)
    // L path table location
    pvd[140..144].copy_from_slice(&18u32.to_le_bytes());
    // M path table location
    pvd[148..152].copy_from_slice(&19u32.to_be_bytes());
    // Root directory record at 156
    write_dir_record(&mut pvd[156..190], root_lba, SECTOR as u32, true, "");
    write_str(&mut pvd[190..318], ""); // volume set id
    write_str(&mut pvd[318..446], "OXIZIP");
    write_str(&mut pvd[446..574], "OXIZIP");
    write_str(&mut pvd[574..702], "OXIZIP");
    pvd[881] = 1; // file structure version
    out.write_all(&pvd)?;

    // Terminator
    let mut term = vec![0u8; SECTOR];
    term[0] = 0xFF;
    term[1..6].copy_from_slice(b"CD001");
    term[6] = 1;
    out.write_all(&term)?;

    // Path table LE (root only)
    let mut pt = vec![0u8; SECTOR];
    pt[0] = 1; // name length for root
    pt[2..6].copy_from_slice(&root_lba.to_le_bytes());
    pt[6..8].copy_from_slice(&1u16.to_le_bytes()); // parent = 1
    pt[8] = 0x00; // root name
    out.write_all(&pt)?;
    // Path table BE
    let mut pt_be = vec![0u8; SECTOR];
    pt_be[0] = 1;
    pt_be[2..6].copy_from_slice(&root_lba.to_be_bytes());
    pt_be[6..8].copy_from_slice(&1u16.to_be_bytes());
    pt_be[8] = 0x00;
    out.write_all(&pt_be)?;

    // Root directory sector
    let mut root = vec![0u8; SECTOR];
    let mut off = 0usize;
    // . and ..
    off += write_dir_record(&mut root[off..], root_lba, SECTOR as u32, true, "\0");
    off += write_dir_record(&mut root[off..], root_lba, SECTOR as u32, true, "\u{1}");
    for (name, lba, size, _) in &file_records {
        cancel.check()?;
        let rec_len = write_dir_record(&mut root[off..], *lba, *size, false, name);
        off += rec_len;
        if off >= SECTOR {
            return Err(CoreError::Other(
                "too many files for single-sector ISO root (simplify inputs)".into(),
            ));
        }
    }
    let _ = off;
    out.write_all(&root)?;

    // File payloads
    for (_name, _lba, _size, data) in &file_records {
        cancel.check()?;
        out.write_all(data)?;
        let pad = (SECTOR - (data.len() % SECTOR)) % SECTOR;
        if pad > 0 {
            out.write_all(&vec![0u8; pad])?;
        }
    }

    // Ensure volume size
    let end = out.stream_position()?;
    let need = u64::from(volume_sectors) * SECTOR as u64;
    if end < need {
        out.seek(SeekFrom::Start(need - 1))?;
        out.write_all(&[0])?;
    }
    Ok(())
}

fn write_str(dst: &mut [u8], s: &str) {
    dst.fill(b' ');
    let bytes = s.as_bytes();
    let n = bytes.len().min(dst.len());
    dst[..n].copy_from_slice(&bytes[..n]);
}

fn write_both_endian_u32(dst: &mut [u8], v: u32) {
    dst[0..4].copy_from_slice(&v.to_le_bytes());
    dst[4..8].copy_from_slice(&v.to_be_bytes());
}

fn write_both_endian_u16(dst: &mut [u8], v: u16) {
    dst[0..2].copy_from_slice(&v.to_le_bytes());
    dst[2..4].copy_from_slice(&v.to_be_bytes());
}

fn write_dir_record(buf: &mut [u8], lba: u32, data_len: u32, is_dir: bool, name: &str) -> usize {
    let name_bytes: &[u8] = match name {
        "\0" => &[0x00],
        "\u{1}" => &[0x01],
        other => other.as_bytes(),
    };
    let mut len = 33 + name_bytes.len();
    if len % 2 == 1 {
        len += 1;
    }
    buf[..len].fill(0);
    buf[0] = len as u8;
    buf[2..10].copy_from_slice(&{
        let mut t = [0u8; 8];
        write_both_endian_u32(&mut t, lba);
        t
    });
    buf[10..18].copy_from_slice(&{
        let mut t = [0u8; 8];
        write_both_endian_u32(&mut t, data_len);
        t
    });
    buf[25] = if is_dir { 0x02 } else { 0x00 };
    buf[32] = name_bytes.len() as u8;
    buf[33..33 + name_bytes.len()].copy_from_slice(name_bytes);
    len
}
