//! Minimal ISO 9660 reader (primary volume, single-extent files).

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use oxizip_core::{
    safe_dest_path, CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate,
};

use crate::traits::ArchiveReader;
use crate::types::{EntryMeta, ExtractOptions, FormatId, ListOptions};

const SECTOR: usize = 2048;

#[derive(Debug, Clone)]
struct IsoEntry {
    name: String,
    lba: u32,
    size: u32,
    is_dir: bool,
}

pub struct IsoReader {
    path: PathBuf,
}

impl IsoReader {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let mut file = File::open(&path)?;
        let _ = read_root_entries(&mut file)?;
        Ok(Self { path })
    }

    fn entries(&self) -> CoreResult<Vec<IsoEntry>> {
        let mut file = File::open(&self.path)?;
        read_root_entries(&mut file)
    }
}

impl ArchiveReader for IsoReader {
    fn format(&self) -> FormatId {
        FormatId::Iso
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
                uncompressed_size: Some(u64::from(e.size)),
                compressed_size: Some(u64::from(e.size)),
                modified: None,
                method: Some("iso9660".into()),
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
        // Dest root must exist for safe_dest_path canonicalize.
        let _ = opts.dest_dir.canonicalize().map_err(CoreError::Io)?;
        let entries = self.entries()?;
        let total = entries.len() as u64;
        let filter = opts.entries.as_ref();
        let mut file = File::open(&self.path)?;

        for (i, e) in entries.iter().enumerate() {
            cancel.check()?;
            if e.is_dir {
                continue;
            }
            if let Some(entries) = filter {
                if !entries.iter().any(|n| n.eq_ignore_ascii_case(&e.name)) {
                    continue;
                }
            }

            let out_path = safe_dest_path(&opts.dest_dir, &e.name)?;
            progress.report(ProgressUpdate {
                stage: JobStage::Extract,
                bytes_done: 0,
                bytes_total: Some(u64::from(e.size)),
                entries_done: (i as u64) + 1,
                entries_total: Some(total),
                current_entry: Some(e.name.clone()),
                message: None,
            });

            let Some(out_path) =
                crate::util::resolve_conflict_path(out_path, opts.conflict)
            else {
                continue;
            };
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            file.seek(SeekFrom::Start(u64::from(e.lba) * SECTOR as u64))?;
            let mut buf = vec![0u8; e.size as usize];
            file.read_exact(&mut buf)?;
            let mut out = File::create(&out_path)?;
            out.write_all(&buf)?;
        }
        Ok(())
    }
}

fn read_root_entries(file: &mut File) -> CoreResult<Vec<IsoEntry>> {
    file.seek(SeekFrom::Start(16 * SECTOR as u64))?;
    let mut pvd = [0u8; 2048];
    file.read_exact(&mut pvd)?;
    if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return Err(CoreError::Other("not an ISO 9660 primary volume".into()));
    }
    let root = &pvd[156..190];
    let root_lba = u32::from_le_bytes(root[2..6].try_into().unwrap());
    let root_size = u32::from_le_bytes(root[10..14].try_into().unwrap());
    read_dir(file, root_lba, root_size)
}

fn read_dir(file: &mut File, lba: u32, size: u32) -> CoreResult<Vec<IsoEntry>> {
    let mut data = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(u64::from(lba) * SECTOR as u64))?;
    file.read_exact(&mut data)?;

    let mut entries = Vec::new();
    let mut off = 0usize;
    while off < data.len() {
        let rec_len = data[off] as usize;
        if rec_len == 0 {
            let next = ((off / SECTOR) + 1) * SECTOR;
            if next <= off || next >= data.len() {
                break;
            }
            off = next;
            continue;
        }
        if off + rec_len > data.len() || rec_len < 34 {
            break;
        }
        let rec = &data[off..off + rec_len];
        let file_lba = u32::from_le_bytes(rec[2..6].try_into().unwrap());
        let file_size = u32::from_le_bytes(rec[10..14].try_into().unwrap());
        let flags = rec[25];
        let name_len = rec[32] as usize;
        if 33 + name_len > rec.len() {
            return Err(CoreError::Other("iso name truncated".into()));
        }
        let raw_name = &rec[33..33 + name_len];
        if raw_name == [0x00] || raw_name == [0x01] {
            off += rec_len;
            continue;
        }
        let mut name = String::from_utf8_lossy(raw_name).into_owned();
        if let Some(idx) = name.find(';') {
            name.truncate(idx);
        }
        name = name.trim().to_string();
        if name.is_empty() {
            off += rec_len;
            continue;
        }
        let is_dir = flags & 0x02 != 0;
        entries.push(IsoEntry {
            name,
            lba: file_lba,
            size: file_size,
            is_dir,
        });
        off += rec_len;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::IsoWriter;
    use crate::traits::ArchiveWriter;
    use crate::types::CreateOptions;
    use oxizip_core::{CancellationToken, NullProgress};
    use tempfile::tempdir;

    #[test]
    fn iso_roundtrip() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("t.iso");
        let src = dir.path().join("hello.txt");
        fs::write(&src, b"hello iso").unwrap();

        let mut w = Box::new(IsoWriter::create(&archive).unwrap());
        let cancel = CancellationToken::new();
        w.add_path(&src, &CreateOptions::default(), &NullProgress, &cancel)
            .unwrap();
        w.finish(&NullProgress, &cancel).unwrap();

        let mut r = IsoReader::open(&archive).unwrap();
        let entries = r
            .list(&ListOptions::default(), &NullProgress, &cancel)
            .unwrap();
        assert!(!entries.is_empty());
        assert!(entries.iter().any(|e| e.name.contains("HELLO")));

        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        r.extract(&ExtractOptions::to_dir(&out), &NullProgress, &cancel)
            .unwrap();
        let extracted = fs::read_dir(&out)
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| e.file_type().ok().map(|t| t.is_file()).unwrap_or(false))
            .unwrap();
        assert_eq!(fs::read(extracted.path()).unwrap(), b"hello iso");
    }
}
