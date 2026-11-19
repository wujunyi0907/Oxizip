//! LZH/LHA reader (level-0 headers; stored `-lh0-` / dir `-lhd-`).

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use oxizip_core::{
    safe_dest_path, CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate,
};

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

#[derive(Debug, Clone)]
struct LzhEntry {
    name: String,
    method: String,
    compressed_size: u64,
    original_size: u64,
    data_offset: u64,
    is_dir: bool,
}

pub struct LzhReader {
    path: PathBuf,
}

impl LzhReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let mut file = File::open(&path)?;
        let _ = parse_entries(&mut file)?;
        Ok(Self { path })
    }

    fn entries(&self) -> CoreResult<Vec<LzhEntry>> {
        let mut file = File::open(&self.path)?;
        parse_entries(&mut file)
    }
}

impl ArchiveReader for LzhReader {
    fn format(&self) -> FormatId {
        FormatId::Lzh
    }

    fn list(
        &mut self,
        _opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>> {
        cancel.check()?;
        let entries = self.entries()?;
        let total = entries.len() as u64;
        let mut out = Vec::with_capacity(entries.len());
        for (i, e) in entries.into_iter().enumerate() {
            cancel.check()?;
            out.push(EntryMeta {
                name: e.name.clone(),
                is_dir: e.is_dir,
                uncompressed_size: Some(e.original_size),
                compressed_size: Some(e.compressed_size),
                modified: None,
                method: Some(e.method.clone()),
            });
            progress.report(ProgressUpdate {
                stage: JobStage::List,
                bytes_done: 0,
                bytes_total: None,
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(e.name),
                message: None,
            });
        }
        Ok(out)
    }

    fn extract(
        &mut self,
        opts: &ExtractOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        cancel.check()?;
        fs::create_dir_all(&opts.dest_dir)?;
        let dest_root = opts.dest_dir.canonicalize().map_err(CoreError::Io)?;
        let entries = self.entries()?;
        let total = entries.len() as u64;
        let filter = opts.entries.as_ref();
        let mut file = File::open(&self.path)?;

        for (i, e) in entries.iter().enumerate() {
            cancel.check()?;
            if let Some(entries) = filter {
                if !entries.iter().any(|n| n == &e.name) {
                    continue;
                }
            }

            let out_path = safe_dest_path(&dest_root, &e.name)?;
            progress.report(ProgressUpdate {
                stage: JobStage::Extract,
                bytes_done: 0,
                bytes_total: Some(e.original_size),
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(e.name.clone()),
                message: None,
            });

            if e.is_dir {
                fs::create_dir_all(&out_path)?;
                continue;
            }

            if out_path.exists() && !opts.overwrite {
                continue;
            }
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            match e.method.as_str() {
                "-lh0-" => {}
                other => {
                    return Err(CoreError::Other(format!(
                        "lzh method {other} not supported for extract (only stored -lh0-)"
                    )));
                }
            }

            file.seek(SeekFrom::Start(e.data_offset))?;
            let mut data = vec![0u8; e.compressed_size as usize];
            file.read_exact(&mut data)?;
            if e.compressed_size != e.original_size {
                return Err(CoreError::Other("lzh stored entry size mismatch".into()));
            }
            let mut out = File::create(&out_path)?;
            out.write_all(&data)?;
        }
        Ok(())
    }
}

fn parse_entries(file: &mut File) -> CoreResult<Vec<LzhEntry>> {
    let mut entries = Vec::new();
    loop {
        let mut hs = [0u8; 1];
        match file.read(&mut hs)? {
            0 => break,
            1 => {}
            _ => unreachable!(),
        }
        let header_size = hs[0];
        if header_size == 0 {
            break;
        }

        let mut checksum = [0u8; 1];
        file.read_exact(&mut checksum)?;
        let mut header = vec![0u8; header_size as usize];
        file.read_exact(&mut header)?;
        let sum = header.iter().fold(0u8, |a, b| a.wrapping_add(*b));
        if sum != checksum[0] {
            return Err(CoreError::Other("lzh header checksum mismatch".into()));
        }
        if header.len() < 21 {
            return Err(CoreError::Other("lzh header too short".into()));
        }

        let method = String::from_utf8_lossy(&header[0..5]).into_owned();
        let compressed_size = u32::from_le_bytes(header[5..9].try_into().unwrap()) as u64;
        let original_size = u32::from_le_bytes(header[9..13].try_into().unwrap()) as u64;
        let level = header[18];
        if level != 0 {
            return Err(CoreError::Other(format!(
                "lzh header level {level} not supported yet (need level 0)"
            )));
        }
        let name_len = header[19] as usize;
        if header.len() < 20 + name_len + 2 {
            return Err(CoreError::Other("lzh header name truncated".into()));
        }
        let name = String::from_utf8_lossy(&header[20..20 + name_len])
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();
        if name.is_empty() {
            return Err(CoreError::Other("lzh empty entry name".into()));
        }
        // Skip remaining extended level-0 bytes (crc already inside header for our writer).
        let data_offset = file.stream_position()?;
        let is_dir = method == "-lhd-" || name.ends_with('/');

        entries.push(LzhEntry {
            name,
            method,
            compressed_size,
            original_size,
            data_offset,
            is_dir,
        });

        file.seek(SeekFrom::Current(compressed_size as i64))?;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::LzhWriter;
    use crate::traits::ArchiveWriter;
    use crate::types::CreateOptions;
    use oxizip_core::{validate_entry_name, CancellationToken, NullProgress};
    use tempfile::tempdir;

    #[test]
    fn lzh_roundtrip_and_slip_name_rejected() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("t.lzh");
        let src = dir.path().join("hello.txt");
        fs::write(&src, b"hello lzh").unwrap();

        let mut w = Box::new(LzhWriter::create(&archive).unwrap());
        let cancel = CancellationToken::new();
        w.add_path(&src, &CreateOptions::default(), &NullProgress, &cancel)
            .unwrap();
        w.finish(&NullProgress, &cancel).unwrap();

        let mut r = LzhReader::open(&archive).unwrap();
        let entries = r
            .list(&ListOptions::default(), &NullProgress, &cancel)
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hello.txt");

        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        r.extract(&ExtractOptions::to_dir(&out), &NullProgress, &cancel)
            .unwrap();
        assert_eq!(fs::read(out.join("hello.txt")).unwrap(), b"hello lzh");
        assert!(validate_entry_name("../x").is_err());
    }
}
