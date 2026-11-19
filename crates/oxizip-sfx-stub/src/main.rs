//! Self-extractor stub: treat this executable as a ZIP with a prepended stub.

use std::env;
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use zip::ZipArchive;

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("oxizip-sfx-stub: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run() -> io::Result<()> {
    let exe = env::current_exe()?;
    let dest = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        exe.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    });
    fs::create_dir_all(&dest)?;

    let file = File::open(&exe)?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("not a valid Oxizip SFX (zip payload missing): {e}"),
        )
    })?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(io::Error::other)?;
        let Some(enclosed) = file.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(enclosed);
        if file.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)?;
        io::copy(&mut file, &mut out)?;
    }

    println!("Extracted to {}", dest.display());
    Ok(())
}
