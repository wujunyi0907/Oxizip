//! Password recovery for ZIP / 7z via wordlist and optional short charset brute.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use oxizip_core::{CancellationToken, CoreError, CoreResult, JobStage, Progress, ProgressUpdate};
use tempfile::TempDir;

use crate::sniff::sniff_path;
use crate::traits::ArchiveReader;
use crate::types::{ExtractOptions, FormatId, ListOptions};
use crate::{SevenZReader, ZipReader};

#[derive(Debug, Clone)]
pub struct RecoverPasswordOptions {
    /// Newline-delimited candidates (UTF-8). Tried first.
    pub wordlist: Option<std::path::PathBuf>,
    /// Character set for brute force (default: `abcdefghijklmnopqrstuvwxyz0123456789`).
    pub charset: String,
    /// Max brute-force length inclusive (default 4; 0 disables brute).
    pub max_len: u32,
}

impl Default for RecoverPasswordOptions {
    fn default() -> Self {
        Self {
            wordlist: None,
            charset: "abcdefghijklmnopqrstuvwxyz0123456789".into(),
            max_len: 4,
        }
    }
}

/// Try passwords until one unlocks the archive. Returns the matching password.
pub fn recover_password(
    archive: &Path,
    opts: &RecoverPasswordOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<String> {
    cancel.check()?;
    let format = sniff_path(archive)?;
    match format {
        FormatId::Zip | FormatId::Zipx | FormatId::ExeSfx | FormatId::SevenZ => {}
        _ => {
            return Err(CoreError::Other(
                "password recovery supports ZIP/ZIPX/7z only".into(),
            ));
        }
    }

    if try_password(archive, format, "", progress, cancel)? {
        return Ok(String::new());
    }

    let mut tried = 0u64;
    if let Some(list) = &opts.wordlist {
        let file = File::open(list)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            cancel.check()?;
            let pw = line?.trim_end_matches(['\r', '\n']).to_string();
            if pw.is_empty() {
                continue;
            }
            tried += 1;
            progress.report(ProgressUpdate {
                stage: JobStage::RecoverPassword,
                bytes_done: tried,
                bytes_total: None,
                entries_done: tried,
                entries_total: None,
                current_entry: Some(format!("wordlist:{tried}")),
                message: Some("wordlist".into()),
            });
            if try_password(archive, format, &pw, progress, cancel)? {
                return Ok(pw);
            }
        }
    }

    if opts.max_len > 0 {
        if opts.charset.is_empty() {
            return Err(CoreError::Other(
                "password recovery: charset is empty".into(),
            ));
        }
        let chars: Vec<char> = opts.charset.chars().collect();
        for len in 1..=opts.max_len {
            cancel.check()?;
            if let Some(pw) = brute_len(archive, format, &chars, len, &mut tried, progress, cancel)?
            {
                return Ok(pw);
            }
        }
    }

    Err(CoreError::Other(
        "password recovery: no matching password found".into(),
    ))
}

fn brute_len(
    archive: &Path,
    format: FormatId,
    chars: &[char],
    len: u32,
    tried: &mut u64,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<Option<String>> {
    let n = chars.len();
    if n == 0 {
        return Ok(None);
    }
    let mut idx = vec![0usize; len as usize];
    loop {
        cancel.check()?;
        let pw: String = idx.iter().map(|&i| chars[i]).collect();
        *tried += 1;
        if (*tried).is_multiple_of(64) {
            progress.report(ProgressUpdate {
                stage: JobStage::RecoverPassword,
                bytes_done: *tried,
                bytes_total: None,
                entries_done: *tried,
                entries_total: None,
                current_entry: Some(format!("brute:{len}:{tried}")),
                message: Some("brute".into()),
            });
        }
        if try_password(archive, format, &pw, progress, cancel)? {
            return Ok(Some(pw));
        }
        let mut carry = true;
        for slot in idx.iter_mut().rev() {
            if !carry {
                break;
            }
            *slot += 1;
            if *slot < n {
                carry = false;
            } else {
                *slot = 0;
            }
        }
        if carry {
            break;
        }
    }
    Ok(None)
}

fn try_password(
    archive: &Path,
    format: FormatId,
    password: &str,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<bool> {
    cancel.check()?;
    let opts = ListOptions {
        password: if password.is_empty() {
            None
        } else {
            Some(password.to_string())
        },
    };
    match format {
        FormatId::Zip | FormatId::Zipx | FormatId::ExeSfx => {
            let mut reader = ZipReader::open(archive)?;
            match reader.test(&opts, progress, cancel) {
                Ok(()) => Ok(true),
                Err(CoreError::InvalidPassword) | Err(CoreError::PasswordRequired) => Ok(false),
                Err(CoreError::Canceled) => Err(CoreError::Canceled),
                Err(_) => Ok(false),
            }
        }
        FormatId::SevenZ => verify_sevenz(archive, &opts, progress, cancel),
        _ => Ok(false),
    }
}

fn verify_sevenz(
    archive: &Path,
    opts: &ListOptions,
    progress: &dyn Progress,
    cancel: &CancellationToken,
) -> CoreResult<bool> {
    let mut reader = SevenZReader::open(archive)?;
    let entries = match reader.list(opts, progress, cancel) {
        Ok(e) => e,
        Err(CoreError::InvalidPassword) | Err(CoreError::PasswordRequired) => return Ok(false),
        Err(CoreError::Canceled) => return Err(CoreError::Canceled),
        Err(_) => return Ok(false),
    };
    let Some(first) = entries.iter().find(|e| !e.is_dir) else {
        // Directory-only archive: list success is enough.
        return Ok(true);
    };
    let tmp = TempDir::new().map_err(CoreError::Io)?;
    let mut xo = ExtractOptions::to_dir(tmp.path());
    xo.password = opts.password.clone();
    xo.entries = Some(vec![first.name.clone()]);
    match reader.extract(&xo, progress, cancel) {
        Ok(()) => Ok(true),
        Err(CoreError::InvalidPassword) | Err(CoreError::PasswordRequired) => Ok(false),
        Err(CoreError::Canceled) => Err(CoreError::Canceled),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::open_writer;
    use crate::types::CreateOptions;
    use oxizip_core::NullProgress;
    use std::io::Write;

    #[test]
    fn recover_zip_aes_from_wordlist() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("p.zip");
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"secret-data").unwrap();
        {
            let mut w = open_writer(&archive, FormatId::Zip).unwrap();
            let opts = CreateOptions {
                password: Some("abc".into()),
                ..CreateOptions::default()
            };
            w.add_path(&src, &opts, &NullProgress, &CancellationToken::new())
                .unwrap();
            w.finish(&NullProgress, &CancellationToken::new()).unwrap();
        }
        let wl = dir.path().join("words.txt");
        {
            let mut f = File::create(&wl).unwrap();
            writeln!(f, "wrong").unwrap();
            writeln!(f, "abc").unwrap();
            writeln!(f, "nope").unwrap();
        }
        let opts = RecoverPasswordOptions {
            wordlist: Some(wl),
            max_len: 0,
            ..RecoverPasswordOptions::default()
        };
        let found =
            recover_password(&archive, &opts, &NullProgress, &CancellationToken::new()).unwrap();
        assert_eq!(found, "abc");
    }
}
