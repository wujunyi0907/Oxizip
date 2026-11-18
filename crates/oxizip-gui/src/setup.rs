//! First-run / post-install setup helpers (file associations via CLI).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// App config directory. Display name stays "Oxizip"; on-disk id uses reverse-DNS.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("cn.wujunyi.oxizip"))
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".config").join("cn.wujunyi.oxizip"))
    }
}

pub fn setup_done_marker() -> Option<PathBuf> {
    Some(config_dir()?.join("setup_done"))
}

pub fn needs_first_run_setup(force: bool) -> bool {
    if force {
        return true;
    }
    match setup_done_marker() {
        Some(p) => !p.is_file(),
        None => false,
    }
}

pub fn mark_setup_done() {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        if let Some(marker) = setup_done_marker() {
            let _ = fs::write(marker, b"1\n");
        }
    }
}

fn cli_beside_gui() -> Option<PathBuf> {
    let gui = std::env::current_exe().ok()?;
    let dir = gui.parent()?;
    #[cfg(windows)]
    let cli = dir.join("oxizip-cli.exe");
    #[cfg(not(windows))]
    let cli = dir.join("oxizip-cli");
    cli.is_file().then_some(cli)
}

/// Run `oxizip-cli shell install` using the CLI next to this GUI binary.
pub fn install_associations(lang_arg: &str) -> Result<String, String> {
    let cli = cli_beside_gui().ok_or_else(|| {
        String::from(
            "oxizip-cli not found beside oxizip; reinstall or run: oxizip-cli shell install",
        )
    })?;
    let output = Command::new(&cli)
        .args(["--lang", lang_arg, "shell", "install"])
        .output()
        .map_err(|e| format!("spawn {}: {e}", cli.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(if stdout.is_empty() {
            "ok".into()
        } else {
            stdout
        })
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

pub fn parse_launch_args() -> (bool, Option<PathBuf>) {
    let mut force_setup = false;
    let mut open_path = None;
    for arg in std::env::args().skip(1) {
        if arg == "--setup" || arg == "-setup" {
            force_setup = true;
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        let p = PathBuf::from(&arg);
        if p.is_file() {
            open_path = Some(p);
        }
    }
    (force_setup, open_path)
}
