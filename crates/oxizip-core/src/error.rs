use std::path::PathBuf;

use thiserror::Error;

/// Stable error codes for i18n mapping (`error.<code>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    Io,
    Canceled,
    PathUnsafe,
    UnsupportedFormat,
    NotAnArchive,
    PasswordRequired,
    InvalidPassword,
    UnrarMissing,
    UnrarFailed,
    WimlibMissing,
    WimlibFailed,
    EntryNotFound,
    Other,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Canceled => "canceled",
            Self::PathUnsafe => "path_unsafe",
            Self::UnsupportedFormat => "unsupported_format",
            Self::NotAnArchive => "not_an_archive",
            Self::PasswordRequired => "password_required",
            Self::InvalidPassword => "invalid_password",
            Self::UnrarMissing => "unrar_missing",
            Self::UnrarFailed => "unrar_failed",
            Self::WimlibMissing => "wimlib_missing",
            Self::WimlibFailed => "wimlib_failed",
            Self::EntryNotFound => "entry_not_found",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("operation canceled")]
    Canceled,

    #[error("unsafe archive path rejected: {0}")]
    PathUnsafe(String),

    #[error("unsupported format")]
    UnsupportedFormat,

    #[error("not an archive")]
    NotAnArchive,

    #[error("password required")]
    PasswordRequired,

    #[error("invalid password")]
    InvalidPassword,

    #[error("official UnRAR CLI not found")]
    UnrarMissing,

    #[error("UnRAR failed: {0}")]
    UnrarFailed(String),

    #[error("wimlib-imagex CLI not found")]
    WimlibMissing,

    #[error("wimlib failed: {0}")]
    WimlibFailed(String),

    #[error("entry not found: {0}")]
    EntryNotFound(String),

    #[error("{0}")]
    Other(String),
}

impl CoreError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Io(_) => ErrorCode::Io,
            Self::Canceled => ErrorCode::Canceled,
            Self::PathUnsafe(_) => ErrorCode::PathUnsafe,
            Self::UnsupportedFormat => ErrorCode::UnsupportedFormat,
            Self::NotAnArchive => ErrorCode::NotAnArchive,
            Self::PasswordRequired => ErrorCode::PasswordRequired,
            Self::InvalidPassword => ErrorCode::InvalidPassword,
            Self::UnrarMissing => ErrorCode::UnrarMissing,
            Self::UnrarFailed(_) => ErrorCode::UnrarFailed,
            Self::WimlibMissing => ErrorCode::WimlibMissing,
            Self::WimlibFailed(_) => ErrorCode::WimlibFailed,
            Self::EntryNotFound(_) => ErrorCode::EntryNotFound,
            Self::Other(_) => ErrorCode::Other,
        }
    }

    pub fn i18n_key(&self) -> String {
        format!("error.{}", self.code().as_str())
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

/// Join a destination root with a relative display path for error messages.
pub fn path_display(root: &std::path::Path, rel: &str) -> PathBuf {
    root.join(rel)
}
