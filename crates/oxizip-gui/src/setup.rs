//! First-run / post-install setup helpers (file associations via CLI).

use std::collections::BTreeMap;
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
        Some(
            PathBuf::from(home)
                .join(".config")
                .join("cn.wujunyi.oxizip"),
        )
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
/// `exts` empty means all supported formats.
pub fn install_associations(lang_arg: &str, exts: &[&str]) -> Result<String, String> {
    let cli = cli_beside_gui().ok_or_else(|| {
        String::from(
            "oxizip-cli not found beside oxizip; reinstall or run: oxizip-cli shell install",
        )
    })?;
    let mut args = vec![
        "--lang".into(),
        lang_arg.into(),
        "shell".into(),
        "install".into(),
    ];
    if !exts.is_empty() {
        args.push("--ext".into());
        args.push(exts.join(","));
    }
    let mut cmd = Command::new(&cli);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .args(&args)
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

/// Association groups shown on the first-run / settings dialog (clean-room layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssocGroup {
    Archive,
    Unix,
    Other,
    Disk,
}

impl AssocGroup {
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Archive => "gui.setup.group.archive",
            Self::Unix => "gui.setup.group.unix",
            Self::Other => "gui.setup.group.other",
            Self::Disk => "gui.setup.group.disk",
        }
    }
}

/// Supported extensions for the association UI (aligned with CLI shell list).
pub fn assoc_catalog() -> &'static [(AssocGroup, &'static [&'static str])] {
    &[
        (
            AssocGroup::Archive,
            &["zip", "zipx", "7z", "rar", "zpaq", "alz", "egg", "cab"],
        ),
        (
            AssocGroup::Unix,
            &["tar", "tgz", "gz", "xz", "zst", "bz2", "lzh", "lha"],
        ),
        (AssocGroup::Other, &["001"]),
        // Present but unchecked by default — keep Windows mount / default handlers.
        (AssocGroup::Disk, &["iso", "wim", "esd", "swm"]),
    ]
}

pub fn default_assoc_selection() -> BTreeMap<&'static str, bool> {
    let mut map = BTreeMap::new();
    for (group, exts) in assoc_catalog() {
        let on = *group != AssocGroup::Disk;
        for ext in *exts {
            map.insert(*ext, on);
        }
    }
    map
}

pub fn selected_exts(selection: &BTreeMap<&'static str, bool>) -> Vec<&'static str> {
    selection
        .iter()
        .filter_map(|(ext, on)| (*on).then_some(*ext))
        .collect()
}
