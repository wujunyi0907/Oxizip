//! ZIP archive-level comment read / write.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use oxizip_core::{CoreError, CoreResult};
use zip::{ZipArchive, ZipWriter};

/// Read the archive comment from a ZIP / ZIPX file (lossy UTF-8).
pub fn zip_read_comment(path: &Path) -> CoreResult<String> {
    let file = File::open(path)?;
    let archive = ZipArchive::new(BufReader::new(file))
        .map_err(|e| CoreError::Other(format!("zip open for comment: {e}")))?;
    Ok(String::from_utf8_lossy(archive.comment()).into_owned())
}

/// Replace the archive comment. Rewrites via append writer finish.
pub fn zip_write_comment(path: &Path, comment: &str) -> CoreResult<()> {
    if comment.len() > u16::MAX as usize {
        return Err(CoreError::Other(
            "zip archive comment exceeds 65535 bytes".into(),
        ));
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let mut writer = ZipWriter::new_append(file)
        .map_err(|e| CoreError::Other(format!("zip append for comment: {e}")))?;
    writer.set_comment(comment);
    writer
        .finish()
        .map_err(|e| CoreError::Other(format!("zip finish comment: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    #[test]
    fn roundtrip_comment() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("c.zip");
        {
            let f = File::create(&path).unwrap();
            let mut w = ZipWriter::new(f);
            w.set_comment("hello-oxizip");
            w.start_file("a.txt", SimpleFileOptions::default()).unwrap();
            w.write_all(b"data").unwrap();
            w.finish().unwrap();
        }
        assert_eq!(zip_read_comment(&path).unwrap(), "hello-oxizip");
        zip_write_comment(&path, "updated").unwrap();
        assert_eq!(zip_read_comment(&path).unwrap(), "updated");
    }
}
