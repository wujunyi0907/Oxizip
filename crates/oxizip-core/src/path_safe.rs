use std::path::{Component, Path, PathBuf};

use crate::error::{CoreError, CoreResult};

/// Reject absolute paths, parent segments, and Windows prefix escapes in archive entry names.
pub fn validate_entry_name(name: &str) -> CoreResult<()> {
    if name.is_empty() {
        return Err(CoreError::PathUnsafe("empty entry name".into()));
    }

    let path = Path::new(name);

    if path.is_absolute() {
        return Err(CoreError::PathUnsafe(format!(
            "absolute path not allowed: {name}"
        )));
    }

    #[cfg(windows)]
    {
        let trimmed = name.trim_start_matches(['/', '\\']);
        if trimmed.len() >= 2 {
            let bytes = trimmed.as_bytes();
            if bytes[1] == b':' {
                return Err(CoreError::PathUnsafe(format!(
                    "drive letter path not allowed: {name}"
                )));
            }
        }
        if trimmed.starts_with("\\\\") || name.starts_with("//") {
            return Err(CoreError::PathUnsafe(format!(
                "UNC path not allowed: {name}"
            )));
        }
    }

    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(CoreError::PathUnsafe(format!(
                    "parent directory segment not allowed: {name}"
                )));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::PathUnsafe(format!(
                    "rooted or prefixed path not allowed: {name}"
                )));
            }
        }
    }

    Ok(())
}

/// Resolve `dest_root / entry_name` and ensure the result stays under `dest_root`.
///
/// Creates missing parent directories for the destination file when `create_parents` is true.
pub fn safe_dest_path(dest_root: &Path, entry_name: &str) -> CoreResult<PathBuf> {
    validate_entry_name(entry_name)?;

    let dest_root = dest_root.canonicalize().map_err(CoreError::Io)?;

    let candidate = dest_root.join(entry_name);
    // Normalize without requiring the path to exist yet.
    let normalized = normalize_lexically(&candidate);

    if !normalized.starts_with(&dest_root) {
        return Err(CoreError::PathUnsafe(format!(
            "path escapes destination root: {entry_name}"
        )));
    }

    Ok(normalized)
}

/// Lexical normalization similar to `Path::components` join (no filesystem access).
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_parent_segments() {
        assert!(validate_entry_name("../etc/passwd").is_err());
        assert!(validate_entry_name("foo/../../x").is_err());
    }

    #[test]
    fn accepts_nested_relative() {
        assert!(validate_entry_name("a/b/c.txt").is_ok());
        assert!(validate_entry_name("./a/b").is_ok());
    }

    #[test]
    fn safe_dest_stays_in_root() {
        let dir = tempfile_dir();
        let dest = safe_dest_path(&dir, "ok/file.txt").unwrap();
        assert!(dest.starts_with(dir.canonicalize().unwrap()));
    }

    #[test]
    fn safe_dest_rejects_escape() {
        let dir = tempfile_dir();
        let err = safe_dest_path(&dir, "../outside.txt").unwrap_err();
        assert!(matches!(err, CoreError::PathUnsafe(_)));
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxizip-path-safe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
