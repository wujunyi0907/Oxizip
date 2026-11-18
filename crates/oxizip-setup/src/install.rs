//! Windows install / uninstall helpers (clean-room).

use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_ID: &str = "cn.wujunyi.oxizip";
pub const APP_NAME: &str = "Oxizip";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn default_install_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let pf = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        pf.join(APP_NAME)
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/opt/Oxizip")
    }
}

pub fn payload_bytes() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/payload.zip"))
}

pub fn has_payload() -> bool {
    payload_bytes().len() > 22 // empty zip / placeholder is tiny
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub dir: PathBuf,
    pub associate: bool,
    pub lang: String,
}

#[derive(Debug)]
pub enum InstallError {
    Io(io::Error),
    Msg(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Msg(m) => write!(f, "{m}"),
        }
    }
}

impl From<io::Error> for InstallError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub type InstallResult<T> = Result<T, InstallError>;

pub fn extract_payload_to(dir: &Path) -> InstallResult<()> {
    if !has_payload() {
        return Err(InstallError::Msg(
            "setup payload missing — rebuild with scripts/package-windows.ps1".into(),
        ));
    }
    fs::create_dir_all(dir)?;
    let cursor = std::io::Cursor::new(payload_bytes());
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| InstallError::Msg(format!("read payload zip: {e}")))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| InstallError::Msg(format!("zip entry: {e}")))?;
        let name = file
            .enclosed_name()
            .ok_or_else(|| InstallError::Msg("illegal path in payload".into()))?
            .to_path_buf();
        let out = dir.join(&name);
        if file.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut dest = fs::File::create(&out)?;
        io::copy(&mut file, &mut dest)?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use std::mem::size_of;
    use std::ptr::null_mut;
    #[repr(C)]
    struct TokenElevation {
        token_is_elevated: u32,
    }
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: *mut u8, access: u32, token: *mut *mut u8) -> i32;
        fn GetTokenInformation(
            token: *mut u8,
            class: u32,
            info: *mut u8,
            len: u32,
            returned: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: *mut u8) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut u8;
    }
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION: u32 = 20;
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TokenElevation {
            token_is_elevated: 0,
        };
        let mut ret = 0u32;
        let ok = GetTokenInformation(
            token,
            TOKEN_ELEVATION,
            &mut elevation as *mut _ as *mut u8,
            size_of::<TokenElevation>() as u32,
            &mut ret,
        );
        CloseHandle(token);
        ok != 0 && elevation.token_is_elevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Relaunch this executable elevated (UAC). Returns Ok(()) if relaunch was requested.
#[cfg(windows)]
pub fn relaunch_elevated(extra_args: &[String]) -> InstallResult<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut u8,
            op: *const u16,
            file: *const u16,
            params: *const u16,
            dir: *const u16,
            show: i32,
        ) -> isize;
    }
    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    let exe = std::env::current_exe().map_err(InstallError::Io)?;
    let exe_s = exe.to_string_lossy().to_string();
    let params = extra_args.join(" ");
    let op = wide("runas");
    let file = wide(&exe_s);
    let par = wide(&params);
    let rc = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            par.as_ptr(),
            std::ptr::null(),
            1,
        )
    };
    if rc <= 32 {
        return Err(InstallError::Msg(format!(
            "UAC elevation failed (ShellExecute={rc})"
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn relaunch_elevated(_extra_args: &[String]) -> InstallResult<()> {
    Err(InstallError::Msg(
        "elevation is only implemented on Windows".into(),
    ))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> InstallResult<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn write_uninstall_registry(dir: &Path) -> InstallResult<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let uninstall = hklm
        .create_subkey(format!(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_ID}"
        ))
        .map_err(|e| InstallError::Msg(format!("create uninstall key: {e}")))?
        .0;
    let uninstaller = dir.join("Uninstall.exe");
    let uninstall_cmd = format!("\"{}\"", uninstaller.display());
    uninstall
        .set_value("DisplayName", &APP_NAME)
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("UninstallString", &uninstall_cmd)
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("QuietUninstallString", &format!("{uninstall_cmd} --uninstall --quiet"))
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("DisplayIcon", &dir.join("oxizip.exe").display().to_string())
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("Publisher", &"wujunyi.cn")
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("DisplayVersion", &VERSION)
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("InstallLocation", &dir.display().to_string())
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("URLInfoAbout", &"https://wujunyi.cn")
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("NoModify", &1u32)
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    uninstall
        .set_value("NoRepair", &1u32)
        .map_err(|e| InstallError::Msg(e.to_string()))?;

    let app = hklm
        .create_subkey(format!("Software\\{APP_ID}"))
        .map_err(|e| InstallError::Msg(e.to_string()))?
        .0;
    app.set_value("InstallDir", &dir.display().to_string())
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    Ok(())
}

#[cfg(not(windows))]
fn write_uninstall_registry(_dir: &Path) -> InstallResult<()> {
    Ok(())
}

#[cfg(windows)]
fn create_start_menu_shortcut(dir: &Path) -> InstallResult<()> {
    let program_data = std::env::var_os("ProgramData").map(PathBuf::from);
    let Some(pd) = program_data else {
        return Ok(());
    };
    let menu = pd.join("Microsoft\\Windows\\Start Menu\\Programs").join(APP_NAME);
    fs::create_dir_all(&menu)?;
    // Prefer a .lnk via PowerShell for reliability without extra crates.
    let target = dir.join("oxizip.exe");
    let link = menu.join("Oxizip.lnk");
    let uninstall_link = menu.join("Uninstall Oxizip.lnk");
    let ps = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{}'); $s.TargetPath = '{}'; $s.IconLocation = '{}'; $s.Save(); \
         $u = $ws.CreateShortcut('{}'); $u.TargetPath = '{}'; $u.Save();",
        link.display().to_string().replace('\'', "''"),
        target.display().to_string().replace('\'', "''"),
        target.display().to_string().replace('\'', "''"),
        uninstall_link.display().to_string().replace('\'', "''"),
        dir.join("Uninstall.exe")
            .display()
            .to_string()
            .replace('\'', "''"),
    );
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps])
        .status();
    Ok(())
}

#[cfg(not(windows))]
fn create_start_menu_shortcut(_dir: &Path) -> InstallResult<()> {
    Ok(())
}

pub fn perform_install(opts: &InstallOptions) -> InstallResult<()> {
    let staging = tempfile::tempdir().map_err(InstallError::Io)?;
    extract_payload_to(staging.path())?;

    fs::create_dir_all(&opts.dir)?;
    copy_dir_recursive(staging.path(), &opts.dir)?;

    // Installer copy becomes Uninstall.exe (same binary, uninstall mode).
    let self_exe = std::env::current_exe().map_err(InstallError::Io)?;
    fs::copy(&self_exe, opts.dir.join("Uninstall.exe"))?;

    write_uninstall_registry(&opts.dir)?;
    create_start_menu_shortcut(&opts.dir)?;

    if opts.associate {
        let cli = opts.dir.join(if cfg!(windows) {
            "oxizip-cli.exe"
        } else {
            "oxizip-cli"
        });
        if cli.is_file() {
            let status = Command::new(&cli)
                .args(["--lang", &opts.lang, "shell", "install"])
                .status()
                .map_err(InstallError::Io)?;
            if !status.success() {
                return Err(InstallError::Msg(format!(
                    "shell install failed with {status}"
                )));
            }
        }
    }
    Ok(())
}

pub fn resolve_install_dir(dir: Option<PathBuf>) -> PathBuf {
    dir.unwrap_or_else(|| {
        #[cfg(windows)]
        {
            use winreg::enums::*;
            use winreg::RegKey;
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            hklm.open_subkey(format!("Software\\{APP_ID}"))
                .ok()
                .and_then(|k| k.get_value::<String, _>("InstallDir").ok())
                .map(PathBuf::from)
                .unwrap_or_else(default_install_dir)
        }
        #[cfg(not(windows))]
        {
            default_install_dir()
        }
    })
}

pub fn perform_uninstall(dir: Option<PathBuf>) -> InstallResult<()> {
    let dir = resolve_install_dir(dir);

    let cli = dir.join(if cfg!(windows) {
        "oxizip-cli.exe"
    } else {
        "oxizip-cli"
    });
    if cli.is_file() {
        let _ = Command::new(&cli)
            .args(["--lang", "zh-cn", "shell", "uninstall"])
            .status();
    }

    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "oxizip.exe"])
            .status();
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "oxizip-cli.exe"])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let _ = hklm.delete_subkey_all(format!(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_ID}"
        ));
        let _ = hklm.delete_subkey_all(format!("Software\\{APP_ID}"));
        if let Some(pd) = std::env::var_os("ProgramData").map(PathBuf::from) {
            let menu = pd
                .join("Microsoft\\Windows\\Start Menu\\Programs")
                .join(APP_NAME);
            let _ = fs::remove_dir_all(menu);
        }
    }

    // Delete files. If this process is Uninstall.exe inside the dir, schedule
    // a delayed rmdir so the running binary can exit first.
    #[cfg(windows)]
    {
        let self_exe = std::env::current_exe().ok();
        let self_in_dir = self_exe
            .as_ref()
            .and_then(|p| p.parent().map(|parent| parent == dir))
            .unwrap_or(false);
        if self_in_dir {
            let script = format!(
                "ping 127.0.0.1 -n 3 >nul & rmdir /S /Q \"{}\"",
                dir.display()
            );
            let _ = Command::new("cmd").args(["/C", &script]).spawn();
            return Ok(());
        }
    }

    if dir.is_dir() {
        let _ = fs::remove_dir_all(&dir);
    }
    Ok(())
}

pub fn launch_app(dir: &Path) -> InstallResult<()> {
    let gui = dir.join(if cfg!(windows) {
        "oxizip.exe"
    } else {
        "oxizip"
    });
    Command::new(gui)
        .arg("--setup")
        .spawn()
        .map_err(InstallError::Io)?;
    Ok(())
}

/// Encode install options into CLI args for elevated relaunch.
pub fn encode_install_args(opts: &InstallOptions) -> Vec<String> {
    let mut args = vec![
        "--elevated-install".into(),
        "--dir".into(),
        opts.dir.display().to_string(),
        "--lang".into(),
        opts.lang.clone(),
    ];
    if opts.associate {
        args.push("--associate".into());
    }
    args
}

pub fn encode_uninstall_args(dir: Option<&Path>) -> Vec<String> {
    let mut args = vec!["--elevated-uninstall".into()];
    if let Some(d) = dir {
        args.push("--dir".into());
        args.push(d.display().to_string());
    }
    args
}

pub fn parse_cli() -> CliMode {
    let mut uninstall = false;
    let mut quiet = false;
    let mut elevated_install = false;
    let mut elevated_uninstall = false;
    let mut associate = false;
    let mut dir = None;
    let mut lang = "zh-cn".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--uninstall" | "/uninstall" => uninstall = true,
            "--quiet" | "/S" => quiet = true,
            "--elevated-install" => elevated_install = true,
            "--elevated-uninstall" => elevated_uninstall = true,
            "--associate" => associate = true,
            "--dir" => dir = args.next().map(PathBuf::from),
            "--lang" => {
                if let Some(v) = args.next() {
                    lang = v;
                }
            }
            _ => {}
        }
    }
    // If this binary was installed as Uninstall.exe, default to uninstall UI.
    if let Ok(exe) = std::env::current_exe() {
        if exe
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("Uninstall"))
            .unwrap_or(false)
        {
            uninstall = true;
        }
    }
    if elevated_install {
        CliMode::ElevatedInstall(InstallOptions {
            dir: dir.unwrap_or_else(default_install_dir),
            associate,
            lang,
        })
    } else if elevated_uninstall || (uninstall && quiet) {
        CliMode::ElevatedUninstall { dir }
    } else if uninstall {
        CliMode::UninstallUi { dir }
    } else {
        CliMode::InstallerUi
    }
}

pub enum CliMode {
    InstallerUi,
    ElevatedInstall(InstallOptions),
    UninstallUi {
        dir: Option<PathBuf>,
    },
    ElevatedUninstall {
        dir: Option<PathBuf>,
    },
}
