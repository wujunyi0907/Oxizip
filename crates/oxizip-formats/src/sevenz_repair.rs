//! Limited 7z repair: strip junk before the 7z signature when present, then
//! try to extract salvageable entries and rewrite a fresh archive.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};
use tempfile::TempDir;

use crate::create::open_writer;
use crate::traits::ArchiveReader;
use crate::types::{CreateOptions, ExtractOptions, FormatId, ListOptions};
use crate::zip_repair::default_repaired_path;

const SIG_7Z: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];

/// Attempt a best-effort repair of a 7z archive.
pub fn repair_sevenz(
    src: &Path,
    dest: Option<&Path>,
    password: Option<&str>,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<PathBuf> {
    cancel.check()?;
    let out = dest
        .map(PathBuf::from)
        .unwrap_or_else(|| default_repaired_path(src, "7z"));
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    progress.report(ProgressUpdate {
        stage: JobStage::Repair,
        bytes_done: 0,
        bytes_total: None,
        entries_done: 0,
        entries_total: None,
        current_entry: None,
        message: Some("locate-signature".into()),
    });

    let cleaned = maybe_strip_prefix(src, progress, cancel)?;
    let work_path = cleaned
        .as_ref()
        .map(|t| t.path().join("stripped.7z"))
        .unwrap_or_else(|| src.to_path_buf());

    let tmp = TempDir::new().map_err(CoreError::Io)?;
    let extract_dir = tmp.path().join("files");
    fs::create_dir_all(&extract_dir)?;

    let pw = password.map(str::to_string);
    let list_opts = ListOptions {
        password: pw.clone(),
    };
    let mut reader = crate::SevenZReader::open(&work_path)?;
    let entries = match reader.list(&list_opts, progress, cancel) {
        Ok(e) => e,
        Err(e) => {
            if cleaned.is_some() {
                fs::copy(&work_path, &out)?;
                return Ok(out);
            }
            return Err(CoreError::Other(format!(
                "7z repair: archive metadata unreadable ({e})"
            )));
        }
    };

    let mut recovered = 0u64;
    let total = entries.len() as u64;
    for (i, entry) in entries.iter().enumerate() {
        cancel.check()?;
        if entry.is_dir {
            continue;
        }
        progress.report(ProgressUpdate {
            stage: JobStage::Repair,
            bytes_done: 0,
            bytes_total: None,
            entries_done: (i as u64) + 1,
            entries_total: Some(total),
            current_entry: Some(entry.name.clone()),
            message: Some("salvage".into()),
        });
        let mut one = ExtractOptions::to_dir(&extract_dir);
        one.password = pw.clone();
        one.entries = Some(vec![entry.name.clone()]);
        one.overwrite = true;
        if reader.extract(&one, progress, cancel).is_ok() {
            recovered += 1;
        }
    }

    if recovered == 0 {
        if cleaned.is_some() {
            fs::copy(&work_path, &out)?;
            return Ok(out);
        }
        return Err(CoreError::Other(
            "7z repair: no entries could be salvaged".into(),
        ));
    }

    let mut inputs = Vec::new();
    collect_files(&extract_dir, &mut inputs);
    if inputs.is_empty() {
        return Err(CoreError::Other(
            "7z repair: salvaged entries produced no files".into(),
        ));
    }

    let mut writer = open_writer(&out, FormatId::SevenZ)?;
    let opts = CreateOptions {
        password: pw,
        ..CreateOptions::default()
    };
    // Add relative to extract_dir so member names stay meaningful.
    for path in &inputs {
        writer.add_path(path, &opts, progress, cancel)?;
    }
    writer.finish(progress, cancel)?;
    Ok(out)
}

fn maybe_strip_prefix(
    src: &Path,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<Option<TempDir>> {
    let mut file = File::open(src)?;
    let mut head = [0u8; 6];
    let n = file.read(&mut head)?;
    if n >= 6 && head[..6] == SIG_7Z {
        return Ok(None);
    }

    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(0))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    cancel.check()?;

    let Some(idx) = data.windows(6).position(|w| w == SIG_7Z) else {
        return Err(CoreError::Other(
            "7z repair: no 7z signature found in file".into(),
        ));
    };

    progress.report(ProgressUpdate {
        stage: JobStage::Repair,
        bytes_done: idx as u64,
        bytes_total: Some(len),
        entries_done: 0,
        entries_total: None,
        current_entry: None,
        message: Some(format!("signature-at-{idx}")),
    });

    let tmp = TempDir::new().map_err(CoreError::Io)?;
    let out_path = tmp.path().join("stripped.7z");
    let mut out = File::create(&out_path)?;
    out.write_all(&data[idx..])?;
    Ok(Some(tmp))
}

fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            out.push(entry.path().to_path_buf());
        }
    }
}
