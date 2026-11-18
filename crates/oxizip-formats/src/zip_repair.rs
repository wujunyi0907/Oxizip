//! Rebuild a damaged ZIP by scanning local file headers and rewriting the
//! central directory + EOCD. Data-descriptor entries (GP bit 3) are recovered
//! when a following descriptor or next signature bounds the payload.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};

const SIG_LOCAL: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const SIG_CENTRAL: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
const SIG_EOCD: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
const SIG_DD: [u8; 4] = [0x50, 0x4B, 0x07, 0x08];

#[derive(Debug, Clone)]
struct LocalEntry {
    version_needed: u16,
    flags: u16,
    method: u16,
    dos_time: u16,
    dos_date: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    name: Vec<u8>,
    extra: Vec<u8>,
    data_offset: u64,
}

/// Repair `src` into `dest` (defaults to `<stem>_repaired.zip` beside source).
pub fn repair_zip(
    src: &Path,
    dest: Option<&Path>,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<PathBuf> {
    cancel.check()?;
    let out = dest
        .map(PathBuf::from)
        .unwrap_or_else(|| default_repaired_path(src, "zip"));
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut input = File::open(src)?;
    let file_len = input.seek(SeekFrom::End(0))?;
    input.seek(SeekFrom::Start(0))?;

    progress.report(ProgressUpdate {
        stage: JobStage::Repair,
        bytes_done: 0,
        bytes_total: Some(file_len),
        entries_done: 0,
        entries_total: None,
        current_entry: None,
        message: Some("scan".into()),
    });

    let entries = scan_local_entries(&mut input, file_len, progress, cancel)?;
    if entries.is_empty() {
        return Err(CoreError::Other(
            "zip repair: no recoverable local file headers found".into(),
        ));
    }

    let mut output = File::create(&out)?;
    let mut central = Vec::new();
    let total = entries.len() as u64;

    for (i, entry) in entries.iter().enumerate() {
        cancel.check()?;
        progress.report(ProgressUpdate {
            stage: JobStage::Repair,
            bytes_done: entry.data_offset.min(file_len),
            bytes_total: Some(file_len),
            entries_done: (i as u64) + 1,
            entries_total: Some(total),
            current_entry: Some(String::from_utf8_lossy(&entry.name).into_owned()),
            message: Some("rewrite".into()),
        });

        let local_off = output.stream_position()?;
        write_local_header(&mut output, entry)?;
        copy_range(
            &mut input,
            &mut output,
            entry.data_offset,
            u64::from(entry.compressed_size),
        )?;
        append_central_header(&mut central, entry, local_off);
    }

    let cd_offset = output.stream_position()?;
    output.write_all(&central)?;
    write_eocd(
        &mut output,
        entries.len() as u16,
        central.len() as u32,
        cd_offset as u32,
    )?;
    output.flush()?;
    Ok(out)
}

pub fn default_repaired_path(src: &Path, ext: &str) -> PathBuf {
    let parent = src.parent().unwrap_or_else(|| Path::new("."));
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("archive");
    parent.join(format!("{stem}_repaired.{ext}"))
}

fn scan_local_entries(
    input: &mut File,
    file_len: u64,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<Vec<LocalEntry>> {
    let mut entries = Vec::new();
    let mut offset = 0u64;
    let mut probe = [0u8; 4];

    while offset + 30 <= file_len {
        cancel.check()?;
        input.seek(SeekFrom::Start(offset))?;
        if input.read(&mut probe)? < 4 {
            break;
        }
        if probe == SIG_LOCAL {
            match try_parse_local(input, offset, file_len)? {
                Some(entry) => {
                    let mut next = entry.data_offset + u64::from(entry.compressed_size);
                    if entry.flags & 0x08 != 0 {
                        next = skip_data_descriptor(input, next, file_len)?;
                    }
                    entries.push(entry);
                    progress.report(ProgressUpdate {
                        stage: JobStage::Repair,
                        bytes_done: next.min(file_len),
                        bytes_total: Some(file_len),
                        entries_done: entries.len() as u64,
                        entries_total: None,
                        current_entry: None,
                        message: Some("scan".into()),
                    });
                    offset = next;
                    continue;
                }
                None => {
                    offset += 1;
                    continue;
                }
            }
        }
        if probe == SIG_CENTRAL || probe == SIG_EOCD {
            break;
        }
        offset += 1;
    }
    Ok(entries)
}

fn try_parse_local(input: &mut File, offset: u64, file_len: u64) -> CoreResult<Option<LocalEntry>> {
    if offset + 30 > file_len {
        return Ok(None);
    }
    input.seek(SeekFrom::Start(offset))?;
    let mut hdr = [0u8; 30];
    if input.read(&mut hdr)? < 30 || hdr[0..4] != SIG_LOCAL {
        return Ok(None);
    }
    let version_needed = u16::from_le_bytes([hdr[4], hdr[5]]);
    let flags = u16::from_le_bytes([hdr[6], hdr[7]]);
    let method = u16::from_le_bytes([hdr[8], hdr[9]]);
    let dos_time = u16::from_le_bytes([hdr[10], hdr[11]]);
    let dos_date = u16::from_le_bytes([hdr[12], hdr[13]]);
    let mut crc32 = u32::from_le_bytes([hdr[14], hdr[15], hdr[16], hdr[17]]);
    let mut compressed_size = u32::from_le_bytes([hdr[18], hdr[19], hdr[20], hdr[21]]);
    let mut uncompressed_size = u32::from_le_bytes([hdr[22], hdr[23], hdr[24], hdr[25]]);
    let name_len = u16::from_le_bytes([hdr[26], hdr[27]]) as usize;
    let extra_len = u16::from_le_bytes([hdr[28], hdr[29]]) as usize;

    if name_len > 65_535 || extra_len > 65_535 {
        return Ok(None);
    }
    if offset + 30 + name_len as u64 + extra_len as u64 > file_len {
        return Ok(None);
    }
    let mut name = vec![0u8; name_len];
    input.read_exact(&mut name)?;
    let mut extra = vec![0u8; extra_len];
    input.read_exact(&mut extra)?;
    let data_offset = offset + 30 + name_len as u64 + extra_len as u64;

    let mut flags_out = flags;
    if flags & 0x08 != 0 && compressed_size == 0 {
        match find_data_descriptor(input, data_offset, file_len)? {
            Some((c, u, crc)) => {
                compressed_size = c;
                uncompressed_size = u;
                crc32 = crc;
                flags_out &= !0x08;
            }
            None => return Ok(None),
        }
    }

    if data_offset + u64::from(compressed_size) > file_len {
        return Ok(None);
    }
    if name.is_empty() && compressed_size == 0 && uncompressed_size == 0 {
        return Ok(None);
    }

    Ok(Some(LocalEntry {
        version_needed,
        flags: flags_out,
        method,
        dos_time,
        dos_date,
        crc32,
        compressed_size,
        uncompressed_size,
        name,
        extra,
        data_offset,
    }))
}

fn find_data_descriptor(
    input: &mut File,
    data_start: u64,
    file_len: u64,
) -> CoreResult<Option<(u32, u32, u32)>> {
    let mut off = data_start;
    let mut probe = [0u8; 4];
    let limit = data_start.saturating_add(64 * 1024 * 1024).min(file_len);
    while off + 4 <= limit {
        input.seek(SeekFrom::Start(off))?;
        if input.read(&mut probe)? < 4 {
            break;
        }
        if probe == SIG_DD {
            let mut rest = [0u8; 12];
            input.read_exact(&mut rest)?;
            let crc = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
            let csize = u32::from_le_bytes([rest[4], rest[5], rest[6], rest[7]]);
            let usize_ = u32::from_le_bytes([rest[8], rest[9], rest[10], rest[11]]);
            if data_start + u64::from(csize) == off {
                return Ok(Some((csize, usize_, crc)));
            }
        }
        if (probe == SIG_LOCAL || probe == SIG_CENTRAL) && off > data_start {
            let csize = (off - data_start) as u32;
            return Ok(Some((csize, 0, 0)));
        }
        off += 1;
    }
    Ok(None)
}

fn skip_data_descriptor(input: &mut File, after_data: u64, file_len: u64) -> CoreResult<u64> {
    if after_data + 4 > file_len {
        return Ok(after_data);
    }
    input.seek(SeekFrom::Start(after_data))?;
    let mut sig = [0u8; 4];
    if input.read(&mut sig)? == 4 && sig == SIG_DD {
        return Ok(after_data + 16);
    }
    Ok(after_data)
}

fn write_local_header(out: &mut File, e: &LocalEntry) -> CoreResult<()> {
    out.write_all(&SIG_LOCAL)?;
    out.write_all(&e.version_needed.to_le_bytes())?;
    out.write_all(&e.flags.to_le_bytes())?;
    out.write_all(&e.method.to_le_bytes())?;
    out.write_all(&e.dos_time.to_le_bytes())?;
    out.write_all(&e.dos_date.to_le_bytes())?;
    out.write_all(&e.crc32.to_le_bytes())?;
    out.write_all(&e.compressed_size.to_le_bytes())?;
    out.write_all(&e.uncompressed_size.to_le_bytes())?;
    out.write_all(&(e.name.len() as u16).to_le_bytes())?;
    out.write_all(&(e.extra.len() as u16).to_le_bytes())?;
    out.write_all(&e.name)?;
    out.write_all(&e.extra)?;
    Ok(())
}

fn append_central_header(buf: &mut Vec<u8>, e: &LocalEntry, local_off: u64) {
    buf.extend_from_slice(&SIG_CENTRAL);
    buf.extend_from_slice(&20u16.to_le_bytes());
    buf.extend_from_slice(&e.version_needed.to_le_bytes());
    buf.extend_from_slice(&e.flags.to_le_bytes());
    buf.extend_from_slice(&e.method.to_le_bytes());
    buf.extend_from_slice(&e.dos_time.to_le_bytes());
    buf.extend_from_slice(&e.dos_date.to_le_bytes());
    buf.extend_from_slice(&e.crc32.to_le_bytes());
    buf.extend_from_slice(&e.compressed_size.to_le_bytes());
    buf.extend_from_slice(&e.uncompressed_size.to_le_bytes());
    buf.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
    buf.extend_from_slice(&(e.extra.len() as u16).to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&(local_off as u32).to_le_bytes());
    buf.extend_from_slice(&e.name);
    buf.extend_from_slice(&e.extra);
}

fn write_eocd(out: &mut File, count: u16, cd_size: u32, cd_offset: u32) -> CoreResult<()> {
    out.write_all(&SIG_EOCD)?;
    out.write_all(&0u16.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?;
    out.write_all(&count.to_le_bytes())?;
    out.write_all(&count.to_le_bytes())?;
    out.write_all(&cd_size.to_le_bytes())?;
    out.write_all(&cd_offset.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?;
    Ok(())
}

fn copy_range(input: &mut File, output: &mut File, start: u64, len: u64) -> CoreResult<()> {
    input.seek(SeekFrom::Start(start))?;
    let mut left = len;
    let mut buf = [0u8; 64 * 1024];
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        input.read_exact(&mut buf[..n])?;
        output.write_all(&buf[..n])?;
        left -= n as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::open_writer;
    use crate::traits::ArchiveReader;
    use crate::types::{CreateOptions, ExtractOptions, FormatId, ListOptions};
    use crate::zip_format::ZipReader;
    use oxizip_core::NullProgress;

    #[test]
    fn repair_rebuilds_truncated_central_directory() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("ok.zip");
        let broken = dir.path().join("broken.zip");
        {
            let mut w = open_writer(&good, FormatId::Zip).unwrap();
            let src = dir.path().join("a.txt");
            fs::write(&src, b"hello-repair").unwrap();
            w.add_path(
                &src,
                &CreateOptions::default(),
                &NullProgress,
                &CancellationToken::new(),
            )
            .unwrap();
            w.finish(&NullProgress, &CancellationToken::new()).unwrap();
        }

        let data = fs::read(&good).unwrap();
        let cut = data.len().saturating_sub(64);
        fs::write(&broken, &data[..cut]).unwrap();

        let out = repair_zip(&broken, None, &NullProgress, &CancellationToken::new()).unwrap();
        let mut reader = ZipReader::open(&out).unwrap();
        let entries = reader
            .list(
                &ListOptions::default(),
                &NullProgress,
                &CancellationToken::new(),
            )
            .unwrap();
        assert!(!entries.is_empty());
        let dest = dir.path().join("out");
        reader
            .extract(
                &ExtractOptions::to_dir(&dest),
                &NullProgress,
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"hello-repair");
    }
}
