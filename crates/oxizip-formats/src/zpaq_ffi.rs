//! In-process official zpaq (embedded `third_party/zpaq`, no external exe).

use std::ffi::CString;
use std::fs;
use std::os::raw::c_char;
use std::path::Path;
use std::sync::Mutex;

use oxizip_core::{CoreError, CoreResult};
use tempfile::NamedTempFile;

extern "C" {
    fn oxizip_zpaq_run(argc: i32, argv: *const *const c_char) -> i32;
    fn oxizip_zpaq_run_capture(
        argc: i32,
        argv: *const *const c_char,
        stdout_path: *const c_char,
        stderr_path: *const c_char,
    ) -> i32;
}

/// Serialize embedded zpaq calls (cwd changes + stdout capture are process-global).
static ZPAQ_LOCK: Mutex<()> = Mutex::new(());

fn build_argv(args: &[&str]) -> CoreResult<(Vec<CString>, Vec<*const c_char>)> {
    let mut c_strings = Vec::with_capacity(args.len() + 1);
    c_strings.push(CString::new("oxizip-zpaq").map_err(|e| CoreError::Other(e.to_string()))?);
    for arg in args {
        c_strings.push(
            CString::new(*arg)
                .map_err(|e| CoreError::Other(format!("zpaq arg contains interior NUL: {e}")))?,
        );
    }
    let ptrs: Vec<*const c_char> = c_strings.iter().map(|s| s.as_ptr()).collect();
    Ok((c_strings, ptrs))
}

fn map_code(code: i32) -> CoreResult<()> {
    if code >= 2 {
        Err(CoreError::Other(format!("zpaq failed (exit {code})")))
    } else {
        Ok(())
    }
}

/// Run embedded zpaq with CLI-compatible UTF-8 arguments (without argv[0]).
pub fn run_zpaq(args: &[&str]) -> CoreResult<()> {
    let _guard = ZPAQ_LOCK
        .lock()
        .map_err(|_| CoreError::Other("zpaq lock poisoned".into()))?;
    let (_keep, ptrs) = build_argv(args)?;
    let code = unsafe { oxizip_zpaq_run(ptrs.len() as i32, ptrs.as_ptr()) };
    map_code(code)
}

/// Run embedded zpaq and capture stdout (stderr discarded to a temp file).
pub fn run_zpaq_capture_stdout(args: &[&str]) -> CoreResult<String> {
    let _guard = ZPAQ_LOCK
        .lock()
        .map_err(|_| CoreError::Other("zpaq lock poisoned".into()))?;
    let out = NamedTempFile::new().map_err(CoreError::Io)?;
    let err = NamedTempFile::new().map_err(CoreError::Io)?;
    let out_path = path_arg(out.path())?;
    let err_path = path_arg(err.path())?;
    let out_c = CString::new(out_path).map_err(|e| CoreError::Other(e.to_string()))?;
    let err_c = CString::new(err_path).map_err(|e| CoreError::Other(e.to_string()))?;

    let (_keep, ptrs) = build_argv(args)?;
    let code = unsafe {
        oxizip_zpaq_run_capture(
            ptrs.len() as i32,
            ptrs.as_ptr(),
            out_c.as_ptr(),
            err_c.as_ptr(),
        )
    };
    map_code(code)?;
    fs::read_to_string(out.path()).map_err(CoreError::Io)
}

/// Temporarily set process CWD for relative archive member names.
pub fn run_zpaq_in_dir(dir: &Path, args: &[&str]) -> CoreResult<()> {
    let _guard = ZPAQ_LOCK
        .lock()
        .map_err(|_| CoreError::Other("zpaq lock poisoned".into()))?;
    let old = std::env::current_dir().map_err(CoreError::Io)?;
    std::env::set_current_dir(dir).map_err(CoreError::Io)?;
    let result = (|| {
        let (_keep, ptrs) = build_argv(args)?;
        let code = unsafe { oxizip_zpaq_run(ptrs.len() as i32, ptrs.as_ptr()) };
        map_code(code)
    })();
    let _ = std::env::set_current_dir(old);
    result
}

pub fn path_arg(path: &Path) -> CoreResult<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Other(format!("non-UTF8 path for zpaq: {}", path.display())))
}
