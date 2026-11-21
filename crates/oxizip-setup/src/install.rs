//! Windows install / uninstall helpers (clean-room).

use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_ID: &str = "cn.wujunyi.oxizip";
pub const APP_NAME: &str = "Oxizip";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Avoid flashing a console window when spawning CLI helpers from the iced UI.
#[cfg(windows)]
fn hide_console(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
fn hide_console(cmd: &mut Command) -> &mut Command {
    cmd
}

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

/// Progress callback: fraction in `0.0..=1.0` plus a short status detail.
pub type ProgressCb<'a> = dyn FnMut(f32, &str) + Send + 'a;

fn report(progress: &mut ProgressCb<'_>, fraction: f32, detail: &str) {
    progress(fraction.clamp(0.0, 1.0), detail);
}

pub fn extract_payload_to(dir: &Path, progress: &mut ProgressCb<'_>) -> InstallResult<()> {
    if !has_payload() {
        return Err(InstallError::Msg(
            "setup payload missing — rebuild with scripts/package-windows.ps1".into(),
        ));
    }
    fs::create_dir_all(dir)?;
    let cursor = std::io::Cursor::new(payload_bytes());
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| InstallError::Msg(format!("read payload zip: {e}")))?;
    let total = archive.len().max(1) as f32;
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
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut dest = fs::File::create(&out)?;
            io::copy(&mut file, &mut dest)?;
        }
        report(progress, (i + 1) as f32 / total, "extract");
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

/// Quote one argv token for `ShellExecuteW` / `CreateProcess` command lines.
/// Without this, paths like `C:\Program Files\Oxizip` split on the space.
fn quote_cmd_arg(arg: &str) -> String {
    let needs_quotes = arg.is_empty()
        || arg
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\u{0000}');
    if !needs_quotes {
        return arg.to_string();
    }
    let mut out = String::from('"');
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                if backslashes > 0 {
                    out.push_str(&"\\".repeat(backslashes));
                    backslashes = 0;
                }
                out.push(c);
            }
        }
    }
    // Trailing backslashes before the closing quote must be doubled.
    if backslashes > 0 {
        out.push_str(&"\\".repeat(backslashes * 2));
    }
    out.push('"');
    out
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
    let params = extra_args
        .iter()
        .map(|a| quote_cmd_arg(a))
        .collect::<Vec<_>>()
        .join(" ");
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
    // ShellExecute errors are normally <= 32. UAC "No"/cancel often returns
    // ERROR_CANCELLED (1223), which is > 32 and must not be treated as success.
    const ERROR_CANCELLED: isize = 1223;
    if rc <= 32 || rc == ERROR_CANCELLED {
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

fn count_files(dir: &Path) -> io::Result<usize> {
    let mut n = 0usize;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            n += count_files(&entry.path())?;
        } else {
            n += 1;
        }
    }
    Ok(n)
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    progress: &mut ProgressCb<'_>,
    done: &mut usize,
    total: usize,
) -> InstallResult<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to, progress, done, total)?;
        } else {
            fs::copy(entry.path(), &to)?;
            *done += 1;
            let frac = if total == 0 {
                1.0
            } else {
                *done as f32 / total as f32
            };
            report(progress, frac, "copy");
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
        .set_value(
            "QuietUninstallString",
            &format!("{uninstall_cmd} --uninstall --quiet"),
        )
        .map_err(|e| InstallError::Msg(e.to_string()))?;
    // Prefer the stand-alone .ico — PE resource icons sometimes fail to render
    // in Start Menu / Apps & features until the icon cache is rebuilt.
    let display_icon = app_icon_path(dir);
    uninstall
        .set_value("DisplayIcon", &format!("{},0", display_icon.display()))
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

/// Best icon path for shortcuts / ARP: packaged `.ico`, else the GUI exe.
fn app_icon_path(dir: &Path) -> PathBuf {
    let ico = dir.join("assets").join("icons").join("oxizip.ico");
    if ico.is_file() {
        ico
    } else {
        dir.join(if cfg!(windows) {
            "oxizip.exe"
        } else {
            "oxizip"
        })
    }
}

#[cfg(windows)]
fn ps_quote(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}

#[cfg(windows)]
fn start_menu_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(pd) = std::env::var_os("ProgramData").map(PathBuf::from) {
        roots.push(
            pd.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    // Also refresh the installing user's Start Menu so a stale per-user .lnk
    // (from an older install) does not keep a broken / blank icon.
    if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
        roots.push(
            appdata
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    roots
}

#[cfg(windows)]
fn create_shortcut_lnk(
    link: &Path,
    target: &Path,
    workdir: &Path,
    icon_loc: &str,
    description: &str,
) -> bool {
    let ps = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{}'); \
         $s.TargetPath = '{}'; \
         $s.WorkingDirectory = '{}'; \
         $s.IconLocation = '{}'; \
         $s.Description = '{}'; \
         $s.Save();",
        ps_quote(link),
        ps_quote(target),
        ps_quote(workdir),
        icon_loc.replace('\'', "''"),
        description.replace('\'', "''"),
    );
    let mut cmd = Command::new("powershell");
    hide_console(&mut cmd)
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .status()
        .map(|s| s.success() && link.is_file())
        .unwrap_or(false)
}

#[cfg(windows)]
fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(pd) = std::env::var_os("PUBLIC").map(PathBuf::from) {
        dirs.push(pd.join("Desktop"));
    }
    if let Some(userprofile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        dirs.push(userprofile.join("Desktop"));
    }
    dirs
}

/// One desktop only: Public when elevated, else the current user.
/// Writing both makes Explorer show two Oxizip icons.
#[cfg(windows)]
fn preferred_desktop_dir() -> Option<PathBuf> {
    if is_elevated() {
        if let Some(pd) = std::env::var_os("PUBLIC").map(PathBuf::from) {
            let desk = pd.join("Desktop");
            if desk.is_dir() {
                return Some(desk);
            }
        }
    }
    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Desktop"))
}

#[cfg(windows)]
fn create_desktop_shortcut(dir: &Path) -> InstallResult<()> {
    let target = dir.join("oxizip.exe");
    let icon_loc = format!("{},0", app_icon_path(dir).display());
    // Drop leftovers from older builds that wrote Public + user desktop.
    for desktop in desktop_dirs() {
        let _ = fs::remove_file(desktop.join("Oxizip.lnk"));
    }
    if let Some(desktop) = preferred_desktop_dir() {
        if desktop.is_dir() {
            let link = desktop.join("Oxizip.lnk");
            let _ = create_shortcut_lnk(&link, &target, dir, &icon_loc, "Oxizip");
        }
    }
    // Desktop shortcut is best-effort; do not fail the whole install.
    Ok(())
}

#[cfg(not(windows))]
fn create_desktop_shortcut(_dir: &Path) -> InstallResult<()> {
    Ok(())
}

#[cfg(windows)]
fn create_start_menu_shortcut(dir: &Path) -> InstallResult<()> {
    let target = dir.join("oxizip.exe");
    let uninstaller = dir.join("Uninstall.exe");
    let icon = app_icon_path(dir);
    // IconLocation must be "path,index" — use the .ico so Start Menu does not
    // depend on PE resource extraction / icon cache for oxizip.exe.
    let icon_loc = format!("{},0", icon.display());

    let mut wrote = false;
    for programs in start_menu_roots() {
        let menu = programs.join(APP_NAME);
        if fs::create_dir_all(&menu).is_err() {
            continue;
        }
        let link = menu.join("Oxizip.lnk");
        let uninstall_link = menu.join("Uninstall Oxizip.lnk");
        let _ = fs::remove_file(menu.join("Uninstall.lnk"));

        let ok_app = create_shortcut_lnk(&link, &target, dir, &icon_loc, "Oxizip");
        let ok_un = create_shortcut_lnk(
            &uninstall_link,
            &uninstaller,
            dir,
            &icon_loc,
            "Uninstall Oxizip",
        );
        if ok_app && ok_un {
            wrote = true;
        }
    }
    create_desktop_shortcut(dir)?;
    notify_shell_assoc_changed();
    if !wrote {
        return Err(InstallError::Msg(
            "failed to create Start Menu shortcut".into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn notify_shell_assoc_changed() {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHChangeNotify(event: u32, flags: u32, item1: isize, item2: isize);
    }
    const SHCNE_ASSOCCHANGED: u32 = 0x0800_0000;
    const SHCNF_IDLIST: u32 = 0x0000;
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, 0, 0);
    }
}

/// Register Win11 Sparse Package for the modern Explorer context menu.
/// Best-effort: classic flat verbs remain available if this fails.
/// `register-sparse.ps1` appends step logs to `sparse-register.log`.
///
/// Important: do **not** wait forever — a hung `regsvr32` / `Add-AppxPackage` used to
/// leave the installer stuck on "processing" with only the first log line written.
#[cfg(windows)]
fn register_sparse_package(install_dir: &Path) -> InstallResult<()> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let dll = install_dir.join("oxizip-shellext.dll");
    if !dll.is_file() {
        return Ok(());
    }
    let script = install_dir.join("scripts").join("register-sparse.ps1");
    let msix = install_dir.join("sparse").join("oxizip-shellext.msix");
    let manifest = install_dir.join("sparse").join("AppxManifest.xml");
    if !script.is_file() && !msix.is_file() && !manifest.is_file() {
        return Ok(());
    }

    // Script timeout 60s + margin for process teardown.
    const SPARSE_WAIT: Duration = Duration::from_secs(75);

    let mut ps = Command::new("powershell.exe");
    // Do not capture stdout/stderr pipes — a blocked reader can deadlock the child.
    // The script writes its own sparse-register.log.
    let child = if script.is_file() {
        hide_console(&mut ps)
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script.display().to_string(),
                "-InstallDir",
                &install_dir.display().to_string(),
                "-TimeoutSec",
                "60",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(InstallError::Io)?
    } else {
        let cmd = if msix.is_file() {
            let cer = install_dir.join("sparse").join("Oxizip.cer");
            format!(
                "$ErrorActionPreference='Stop'; \
                 New-Item 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AppModelUnlock' -Force | Out-Null; \
                 Set-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AppModelUnlock' AllowAllTrustedApps 1 -Type DWord -Force; \
                 if(Test-Path '{cer}'){{ Import-Certificate -FilePath '{cer}' -CertStoreLocation Cert:\\LocalMachine\\TrustedPeople | Out-Null }}; \
                 $p=Get-AppxPackage -Name 'cn.wujunyi.oxizip.shellext' -EA SilentlyContinue; \
                 if($p){{Remove-AppxPackage -Package $p.PackageFullName -EA SilentlyContinue}}; \
                 Add-AppxPackage -Path '{msix}' -ExternalLocation '{dir}'",
                cer = cer.display(),
                msix = msix.display(),
                dir = install_dir.display()
            )
        } else {
            format!(
                "$ErrorActionPreference='Stop'; \
                 New-Item 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AppModelUnlock' -Force | Out-Null; \
                 Set-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AppModelUnlock' AllowAllTrustedApps 1 -Type DWord -Force; \
                 Set-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AppModelUnlock' AllowDevelopmentWithoutDevLicense 1 -Type DWord -Force; \
                 $p=Get-AppxPackage -Name 'cn.wujunyi.oxizip.shellext' -EA SilentlyContinue; \
                 if($p){{Remove-AppxPackage -Package $p.PackageFullName -EA SilentlyContinue}}; \
                 Add-AppxPackage -Path '{manifest}' -Register -ExternalLocation '{dir}'",
                manifest = manifest.display(),
                dir = install_dir.display()
            )
        };
        hide_console(&mut ps)
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &cmd])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(InstallError::Io)?
    };

    let mut child = child;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= SPARSE_WAIT {
                    let _ = child.kill();
                    let _ = child.wait();
                    let log = install_dir.join("sparse-register.log");
                    let note = format!(
                        "\n[{}] ERROR: installer killed register-sparse.ps1 after {}s timeout\n",
                        chrono_like_now(),
                        SPARSE_WAIT.as_secs()
                    );
                    let _ = append_log(&log, &note);
                    // Classic verbs still work; do not fail the install.
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(InstallError::Io(e)),
        }
    };

    if status.success() {
        notify_shell_assoc_changed();
    }
    Ok(())
}

#[cfg(windows)]
fn chrono_like_now() -> String {
    // Local wall clock for log append (avoid extra chrono dep).
    use std::time::SystemTime;
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => format!("unix+{}s", d.as_secs()),
        Err(_) => "unknown-time".into(),
    }
}

#[cfg(windows)]
fn append_log(path: &Path, text: &str) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(text.as_bytes());
    }
}

#[cfg(not(windows))]
fn create_start_menu_shortcut(_dir: &Path) -> InstallResult<()> {
    Ok(())
}

pub fn perform_install(opts: &InstallOptions, progress: &mut ProgressCb<'_>) -> InstallResult<()> {
    report(progress, 0.02, "prepare");
    let staging = tempfile::tempdir().map_err(InstallError::Io)?;

    extract_payload_to(staging.path(), &mut |f, _| {
        report(progress, 0.05 + f * 0.50, "extract");
    })?;

    report(progress, 0.56, "copy");
    fs::create_dir_all(&opts.dir)?;
    let total_files = count_files(staging.path()).unwrap_or(1).max(1);
    let mut copied = 0usize;
    copy_dir_recursive(
        staging.path(),
        &opts.dir,
        &mut |f, _| {
            report(progress, 0.56 + f * 0.28, "copy");
        },
        &mut copied,
        total_files,
    )?;

    report(progress, 0.86, "uninstall_stub");
    // Installer copy becomes Uninstall.exe (same binary, uninstall mode).
    let self_exe = std::env::current_exe().map_err(InstallError::Io)?;
    fs::copy(&self_exe, opts.dir.join("Uninstall.exe"))?;

    report(progress, 0.90, "registry");
    write_uninstall_registry(&opts.dir)?;
    report(progress, 0.94, "shortcut");
    create_start_menu_shortcut(&opts.dir)?;

    // Context menus first (before the GUI association page claims default open).
    report(progress, 0.96, "menus");
    let cli = opts.dir.join(if cfg!(windows) {
        "oxizip-cli.exe"
    } else {
        "oxizip-cli"
    });
    if cli.is_file() {
        let mut cmd = Command::new(&cli);
        let status = hide_console(&mut cmd)
            .args([
                "--lang",
                &opts.lang,
                "shell",
                "install",
                "--menus-only",
            ])
            .status()
            .map_err(InstallError::Io)?;
        if !status.success() {
            return Err(InstallError::Msg(format!(
                "shell menus install failed with {status}"
            )));
        }
    }

    // Optional full associations during silent/elevated flows that pass --associate.
    if opts.associate {
        report(progress, 0.97, "associate");
        if cli.is_file() {
            let mut cmd = Command::new(&cli);
            let status = hide_console(&mut cmd)
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

    // Win11 Sparse backup — `shell install --menus-only` already tries; this is idempotent.
    #[cfg(windows)]
    {
        report(progress, 0.985, "sparse");
        let _ = register_sparse_package(&opts.dir);
    }

    report(progress, 1.0, "done");
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

#[cfg(windows)]
fn norm_path_key(path: &Path) -> String {
    let canon = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let s = canon.to_string_lossy();
    // Strip Windows `\\?\` prefix from canonicalize for stable compares.
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches(['/', '\\'])
        .to_ascii_lowercase()
}

#[cfg(windows)]
fn path_is_inside(child: &Path, root: &Path) -> bool {
    let c = norm_path_key(child);
    let r = norm_path_key(root);
    c == r || c.starts_with(&(r.clone() + "\\"))
}

/// Delete everything under `dir` except `keep` (typically the running Uninstall.exe).
#[cfg(windows)]
fn delete_tree_except(dir: &Path, keep: Option<&Path>) -> io::Result<()> {
    let keep_key = keep.map(norm_path_key);
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if keep_key
            .as_ref()
            .is_some_and(|k| norm_path_key(&path) == *k)
        {
            continue;
        }
        let ty = entry.file_type()?;
        if ty.is_dir() {
            // If keep is nested under this subdir, recurse; else wipe.
            if keep.is_some_and(|k| path_is_inside(k, &path)) {
                delete_tree_except(&path, keep)?;
                // Leave dir if keep still inside; otherwise remove if empty.
                let _ = fs::remove_dir(&path);
            } else {
                let _ = fs::remove_dir_all(&path);
            }
        } else {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

/// Best-effort: mark path for deletion after reboot (handles stubborn locks).
#[cfg(windows)]
fn mark_delete_on_reboot(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_DELAY_UNTIL_REBOOT: u32 = 0x4;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let _ = MoveFileExW(wide.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT);
    }
}

/// Bandizip-style final wipe (`launch_in_temp` / `~xinst~temp~.exe`):
/// copy this EXE to `%TEMP%\~oxizip~uninst~…~.exe`, start it, then caller must
/// `ExitProcess` so the helper can delete the install folder.
pub fn schedule_install_dir_cleanup(dir: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Stdio;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // Do **not** use DETACHED_PROCESS — it can tear down the child with the UI
        // process on some Windows job objects. CREATE_NO_WINDOW is enough (no flash).

        let self_exe = std::env::current_exe()?;
        let pid = std::process::id();
        let temp_dir = std::env::temp_dir();
        let temp = temp_dir.join(format!("~oxizip~uninst~{pid}~.exe"));
        let _ = fs::remove_file(&temp);
        fs::copy(&self_exe, &temp)?;
        clear_readonly(&temp);

        let mut child = Command::new(&temp);
        child
            .args([
                "--cleanup-dir",
                &dir.display().to_string(),
                "--wait-pid",
                &pid.to_string(),
            ])
            .current_dir(&temp_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        let _child = child.spawn()?;
        // Intentionally leak/detach: do not wait; UI will ExitProcess next.
        std::mem::forget(_child);
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
    }
    Ok(())
}

#[cfg(windows)]
fn clear_readonly(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        let _ = fs::set_permissions(path, perms);
    }
}

/// Recursively delete like Bandizip `DeleteOrMoveFile` / `RemoveDirectory`.
#[cfg(windows)]
fn force_remove_tree(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    clear_readonly(path);
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            force_remove_tree(&p)?;
        }
        match fs::remove_dir(path) {
            Ok(()) => Ok(()),
            Err(e) => {
                mark_delete_on_reboot(path);
                Err(e)
            }
        }
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(_) => {
                // Locked: rename aside then mark reboot delete (Bandizip DeleteOrMoveFile).
                let alt = {
                    let mut p = path.as_os_str().to_owned();
                    p.push(".pending_delete");
                    PathBuf::from(p)
                };
                let _ = fs::rename(path, &alt);
                mark_delete_on_reboot(path);
                mark_delete_on_reboot(&alt);
                let _ = fs::remove_file(&alt);
                if path.exists() {
                    Err(io::Error::other(format!(
                        "still locked: {}",
                        path.display()
                    )))
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[cfg(windows)]
fn wait_for_pid(pid: u32, timeout_ms: u32) {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut u8;
        fn WaitForSingleObject(handle: *mut u8, ms: u32) -> u32;
        fn CloseHandle(handle: *mut u8) -> i32;
    }
    const SYNCHRONIZE: u32 = 0x0010_0000;
    unsafe {
        let h = OpenProcess(SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return;
        }
        let _ = WaitForSingleObject(h, timeout_ms);
        let _ = CloseHandle(h);
    }
}

/// Bandizip closes `explorer.exe` before wiping (shell ext DLLs lock the folder).
#[cfg(windows)]
fn close_explorer() {
    let mut kill = Command::new("taskkill");
    let _ = hide_console(&mut kill)
        .args(["/F", "/IM", "explorer.exe"])
        .status();
    std::thread::sleep(std::time::Duration::from_millis(600));
}

#[cfg(windows)]
fn restart_explorer() {
    let _ = Command::new("explorer.exe").spawn();
}

/// SHFileOperation FO_DELETE — Bandizip `DeleteFolderRec` equivalent.
#[cfg(windows)]
fn sh_delete_folder(dir: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    #[repr(C)]
    struct ShFileOpStructW {
        hwnd: *mut u8,
        w_func: u32,
        p_from: *const u16,
        p_to: *const u16,
        f_flags: u16,
        f_any_operations_aborted: i32,
        h_name_mappings: *mut u8,
        lpsz_progress_title: *const u16,
    }
    #[link(name = "shell32")]
    extern "system" {
        fn SHFileOperationW(op: *mut ShFileOpStructW) -> i32;
    }
    const FO_DELETE: u32 = 3;
    const FOF_SILENT: u16 = 0x0004;
    const FOF_NOCONFIRMATION: u16 = 0x0010;
    const FOF_NOERRORUI: u16 = 0x0400;
    const FOF_NOCONFIRMMKDIR: u16 = 0x0200;

    // pFrom is a double-null-terminated list of paths.
    let mut from: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .chain(std::iter::once(0))
        .collect();
    let mut op = ShFileOpStructW {
        hwnd: std::ptr::null_mut(),
        w_func: FO_DELETE,
        p_from: from.as_mut_ptr(),
        p_to: std::ptr::null(),
        f_flags: FOF_SILENT | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_NOCONFIRMMKDIR,
        f_any_operations_aborted: 0,
        h_name_mappings: std::ptr::null_mut(),
        lpsz_progress_title: std::ptr::null(),
    };
    unsafe { SHFileOperationW(&mut op) == 0 && op.f_any_operations_aborted == 0 }
}

/// Temp-helper: wait for UI PID, close Explorer (unlock shellext), wipe install folder.
pub fn perform_cleanup_dir(dir: PathBuf, wait_pid: u32) {
    #[cfg(windows)]
    {
        if wait_pid != 0 {
            wait_for_pid(wait_pid, 120_000);
        }
        std::thread::sleep(std::time::Duration::from_millis(800));

        // Bandizip `CloseApp explorer.exe` — releases locks on shellext / icons.
        close_explorer();

        for _ in 0..120 {
            if !dir.exists() {
                break;
            }
            clear_readonly(&dir);
            let _ = sh_delete_folder(&dir);
            let _ = force_remove_tree(&dir);
            let _ = fs::remove_dir_all(&dir);
            if !dir.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        if dir.exists() {
            clear_readonly(&dir);
            let _ = sh_delete_folder(&dir);
            let _ = fs::remove_dir_all(&dir);
            mark_delete_on_reboot(&dir);
        }

        restart_explorer();

        if let Ok(exe) = std::env::current_exe() {
            clear_readonly(&exe);
            mark_delete_on_reboot(&exe);
            let doomed = {
                let mut p = exe.as_os_str().to_owned();
                p.push(".old");
                PathBuf::from(p)
            };
            let _ = fs::rename(&exe, &doomed);
            clear_readonly(&doomed);
            mark_delete_on_reboot(&doomed);
            let _ = fs::remove_file(&doomed);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (dir, wait_pid);
    }
}

pub fn perform_uninstall(dir: Option<PathBuf>, progress: &mut ProgressCb<'_>) -> InstallResult<()> {
    let dir = resolve_install_dir(dir);
    report(progress, 0.05, "prepare");

    let cli = dir.join(if cfg!(windows) {
        "oxizip-cli.exe"
    } else {
        "oxizip-cli"
    });
    if cli.is_file() {
        report(progress, 0.15, "shell");
        let mut cmd = Command::new(&cli);
        let _ = hide_console(&mut cmd)
            .args(["--lang", "zh-cn", "shell", "uninstall"])
            .status();
    }

    #[cfg(windows)]
    {
        // Best-effort Sparse / COM cleanup before files vanish.
        let unreg = dir.join("scripts").join("unregister-sparse.ps1");
        if unreg.is_file() {
            let mut ps = Command::new("powershell.exe");
            let _ = hide_console(&mut ps)
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &unreg.display().to_string(),
                    "-InstallDir",
                    &dir.display().to_string(),
                ])
                .status();
        }
    }

    #[cfg(windows)]
    {
        report(progress, 0.30, "stop");
        for image in ["oxizip.exe", "oxizip-cli.exe"] {
            let mut kill = Command::new("taskkill");
            let _ = hide_console(&mut kill).args(["/F", "/IM", image]).status();
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    #[cfg(windows)]
    {
        report(progress, 0.50, "registry");
        use winreg::enums::*;
        use winreg::RegKey;
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let _ = hklm.delete_subkey_all(format!(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_ID}"
        ));
        let _ = hklm.delete_subkey_all(format!("Software\\{APP_ID}"));
        report(progress, 0.65, "start_menu");
        for programs in start_menu_roots() {
            let menu = programs.join(APP_NAME);
            let _ = fs::remove_dir_all(menu);
        }
        for desktop in desktop_dirs() {
            let _ = fs::remove_file(desktop.join("Oxizip.lnk"));
        }
        notify_shell_assoc_changed();
    }

    report(progress, 0.85, "delete");
    #[cfg(windows)]
    {
        let self_exe = std::env::current_exe().ok();
        let self_in_dir = self_exe
            .as_ref()
            .is_some_and(|p| path_is_inside(p, &dir));
        if self_in_dir {
            // Leave only this Uninstall.exe; final wipe runs when user clicks「完成」.
            let keep = self_exe.as_deref();
            let _ = delete_tree_except(&dir, keep);
            report(progress, 1.0, "done");
            return Ok(());
        }
        // Not running from install dir — delete immediately; fall back to temp helper.
        if dir.is_dir() {
            if force_remove_tree(&dir).is_err() || dir.exists() {
                let _ = schedule_install_dir_cleanup(&dir);
            }
        }
        report(progress, 1.0, "done");
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        if dir.is_dir() {
            let _ = fs::remove_dir_all(&dir);
        }
        report(progress, 1.0, "done");
        Ok(())
    }
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

/// Encode install options for elevated relaunch that keeps the UI (progress bar).
pub fn encode_install_args(opts: &InstallOptions) -> Vec<String> {
    let mut args = vec![
        "--auto-install".into(),
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

/// Encode uninstall args for elevated relaunch that keeps the UI (progress bar).
pub fn encode_uninstall_args(dir: Option<&Path>) -> Vec<String> {
    let mut args = vec!["--uninstall".into(), "--auto-uninstall".into()];
    if let Some(d) = dir {
        args.push("--dir".into());
        args.push(d.display().to_string());
    }
    args
}

/// Append `--pos-x` / `--pos-y` so an elevated child can reopen at the same place.
pub fn append_window_pos(args: &mut Vec<String>, pos: Option<(f32, f32)>) {
    if let Some((x, y)) = pos {
        args.push("--pos-x".into());
        args.push(x.to_string());
        args.push("--pos-y".into());
        args.push(y.to_string());
    }
}

pub fn parse_cli() -> CliMode {
    let mut uninstall = false;
    let mut quiet = false;
    let mut elevated_install = false;
    let mut elevated_uninstall = false;
    let mut auto_install = false;
    let mut auto_uninstall = false;
    let mut associate = false;
    let mut dir = None;
    let mut cleanup_dir = None;
    let mut wait_pid = 0u32;
    let mut lang = "zh-cn".to_string();
    let mut pos_x = None;
    let mut pos_y = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--uninstall" | "/uninstall" => uninstall = true,
            "--quiet" | "/S" => quiet = true,
            "--elevated-install" => elevated_install = true,
            "--elevated-uninstall" => elevated_uninstall = true,
            "--auto-install" => auto_install = true,
            "--auto-uninstall" => auto_uninstall = true,
            "--associate" => associate = true,
            "--cleanup-dir" => cleanup_dir = args.next().map(PathBuf::from),
            "--wait-pid" => {
                wait_pid = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            "--dir" => dir = args.next().map(PathBuf::from),
            "--lang" => {
                if let Some(v) = args.next() {
                    lang = v;
                }
            }
            "--pos-x" => {
                pos_x = args.next().and_then(|v| v.parse().ok());
            }
            "--pos-y" => {
                pos_y = args.next().and_then(|v| v.parse().ok());
            }
            _ => {}
        }
    }
    let window_pos = match (pos_x, pos_y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    // Temp helper copied out of the install dir (Bandizip `~xinst~temp~.exe`).
    if let Some(d) = cleanup_dir {
        return CliMode::CleanupDir {
            dir: d,
            wait_pid,
        };
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
        // Quiet/script path (no UI).
        CliMode::ElevatedInstall(InstallOptions {
            dir: dir.clone().unwrap_or_else(default_install_dir),
            associate,
            lang: lang.clone(),
        })
    } else if elevated_uninstall || (uninstall && quiet) {
        CliMode::ElevatedUninstall { dir: dir.clone() }
    } else if uninstall {
        CliMode::UninstallUi {
            dir,
            auto_start: auto_uninstall,
            window_pos,
        }
    } else {
        let auto = if auto_install {
            Some(InstallOptions {
                dir: dir.unwrap_or_else(default_install_dir),
                associate,
                lang,
            })
        } else {
            None
        };
        CliMode::InstallerUi {
            auto_install: auto,
            window_pos,
        }
    }
}

pub enum CliMode {
    InstallerUi {
        auto_install: Option<InstallOptions>,
        window_pos: Option<(f32, f32)>,
    },
    ElevatedInstall(InstallOptions),
    UninstallUi {
        dir: Option<PathBuf>,
        auto_start: bool,
        window_pos: Option<(f32, f32)>,
    },
    ElevatedUninstall {
        dir: Option<PathBuf>,
    },
    /// Headless wipe launched from `%TEMP%` after「完成」(no UI).
    CleanupDir {
        dir: PathBuf,
        wait_pid: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::quote_cmd_arg;

    #[test]
    fn quote_preserves_program_files_path() {
        let path = r"C:\Program Files\Oxizip";
        let quoted = quote_cmd_arg(path);
        assert_eq!(quoted, r#""C:\Program Files\Oxizip""#);
        let line = format!("--auto-install --dir {quoted} --lang zh-cn");
        assert!(line.contains(r#""C:\Program Files\Oxizip""#));
    }

    #[test]
    fn quote_skips_simple_tokens() {
        assert_eq!(quote_cmd_arg("--dir"), "--dir");
        assert_eq!(quote_cmd_arg(r"C:\Oxizip"), r"C:\Oxizip");
    }
}
