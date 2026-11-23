//! Minimal LZH writer using stored method `-lh0-` (no dictionary compression).

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};

use crate::traits::ArchiveWriter;
use crate::types::{CreateOptions, FormatId};
use crate::util::{collect_pack_files_with_opts, ensure_parent};

pub struct LzhWriter {
    path: PathBuf,
    files: Vec<(PathBuf, String)>,
}

impl LzhWriter {
    pub fn create(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Self {
            path,
            files: Vec::new(),
        })
    }
}

impl ArchiveWriter for LzhWriter {
    fn format(&self) -> FormatId {
        FormatId::Lzh
    }

    fn add_path(
        &mut self,
        path: &Path,
        opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        let packed = collect_pack_files_with_opts(&[path], opts, cancel)?;
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
            // LZH level-0 filename is a single path component traditionally; keep basename for safety.
            let name = Path::new(&f.name)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(f.name);
            if name.len() > 255 {
                return Err(CoreError::Other("lzh filename too long".into()));
            }
            self.files.push((f.source, name));
        }
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        _progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        let mut out = BufWriter::new(File::create(&self.path)?);
        for (source, name) in &self.files {
            cancel.check()?;
            write_lh0_entry(&mut out, source, name)?;
        }
        out.flush()?;
        Ok(())
    }
}

fn write_lh0_entry(out: &mut impl Write, source: &Path, name: &str) -> CoreResult<()> {
    let mut data = Vec::new();
    File::open(source)?.read_to_end(&mut data)?;
    let size = data.len() as u32;
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len() as u8;

    // Level-0 header body (after size+checksum): method(5)+csize(4)+usize(4)+time(4)+attr(1)+level(1)+namelen(1)+name+crc(2)
    let header_size = (22 + name_bytes.len()) as u8;

    let mut header = Vec::with_capacity(header_size as usize);
    header.extend_from_slice(b"-lh0-");
    header.extend_from_slice(&size.to_le_bytes()); // compressed == original for lh0
    header.extend_from_slice(&size.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes()); // DOS time/date unused
    header.push(0x20); // attr
    header.push(0x00); // level 0
    header.push(name_len);
    header.extend_from_slice(name_bytes);
    let crc = crc16_arc(&data);
    header.extend_from_slice(&crc.to_le_bytes());

    let checksum = header.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));

    out.write_all(&[header_size, checksum])?;
    out.write_all(&header)?;
    out.write_all(&data)?;
    Ok(())
}

/// CRC-16/ARC used by LHA.
fn crc16_arc(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}
