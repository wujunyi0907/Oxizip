//! File checksum helpers (CRC32 / SHA-256).

use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::cancel::CancellationToken;
use crate::error::{CoreError, CoreResult};
use crate::progress::{JobStage, Progress, ProgressUpdate};

const BUF: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChecksum {
    pub path: String,
    pub size: u64,
    pub crc32: u32,
    pub sha256: [u8; 32],
}

impl FileChecksum {
    pub fn sha256_hex(&self) -> String {
        hex_encode(&self.sha256)
    }

    pub fn crc32_hex(&self) -> String {
        format!("{:08x}", self.crc32)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Compute CRC32 and SHA-256 for a single file.
pub fn checksum_file(
    path: &Path,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<FileChecksum> {
    cancel.check()?;
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() {
        return Err(CoreError::Other("checksum requires a file, not a directory".into()));
    }
    let size = meta.len();
    let mut file = File::open(path)?;
    let mut buf = [0u8; BUF];
    let mut crc = crc32fast::Hasher::new();
    let mut sha = Sha256::new();
    let mut done = 0u64;

    progress.report(ProgressUpdate {
        stage: JobStage::Checksum,
        bytes_done: 0,
        bytes_total: Some(size),
        entries_done: 0,
        entries_total: Some(1),
        current_entry: Some(path.display().to_string()),
        message: Some("checksum".into()),
    });

    loop {
        cancel.check()?;
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
        sha.update(&buf[..n]);
        done += n as u64;
        progress.report(ProgressUpdate {
            stage: JobStage::Checksum,
            bytes_done: done,
            bytes_total: Some(size),
            entries_done: 0,
            entries_total: Some(1),
            current_entry: Some(path.display().to_string()),
            message: Some("checksum".into()),
        });
    }

    Ok(FileChecksum {
        path: path.display().to_string(),
        size,
        crc32: crc.finalize(),
        sha256: sha.finalize().into(),
    })
}

/// Checksum multiple files sequentially.
pub fn checksum_paths(
    paths: &[impl AsRef<Path>],
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<Vec<FileChecksum>> {
    let mut out = Vec::with_capacity(paths.len());
    let total = paths.len() as u64;
    for (i, path) in paths.iter().enumerate() {
        cancel.check()?;
        let path = path.as_ref();
        progress.report(ProgressUpdate {
            stage: JobStage::Checksum,
            bytes_done: 0,
            bytes_total: None,
            entries_done: i as u64,
            entries_total: Some(total),
            current_entry: Some(path.display().to_string()),
            message: Some("checksum".into()),
        });
        out.push(checksum_file(path, progress, cancel)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::NullProgress;
    use std::io::Write;

    #[test]
    fn checksum_known_bytes() {
        let path = std::env::temp_dir().join(format!(
            "oxizip-checksum-test-{}.txt",
            std::process::id()
        ));
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"hello").unwrap();
        }
        let cancel = CancellationToken::new();
        let cs = checksum_file(&path, &NullProgress, &cancel).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(cs.size, 5);
        assert_eq!(cs.crc32_hex(), "3610a686");
        assert_eq!(
            cs.sha256_hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
