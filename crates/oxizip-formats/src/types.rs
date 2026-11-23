use std::path::PathBuf;
use std::time::SystemTime;

/// Identifiers for supported archive formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatId {
    Zip,
    Zipx,
    SevenZ,
    /// Self-extracting archive (ZIP payload + stub), written as `.exe`.
    ExeSfx,
    Tar,
    TarGz,
    TarXz,
    TarZst,
    TarBz2,
    Lzh,
    Iso,
    Gz,
    Xz,
    Zst,
    Bz2,
    Zpaq,
    Rar,
    /// Microsoft Cabinet (read-only).
    Cab,
    /// Microsoft WIM (read-only via wimlib-imagex CLI).
    Wim,
    /// ALZip ALZ (read-only).
    Alz,
    /// ESTsoft EGG (read-only).
    Egg,
}

impl FormatId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Zipx => "zipx",
            Self::SevenZ => "7z",
            Self::ExeSfx => "exe",
            Self::Tar => "tar",
            Self::TarGz => "tgz",
            Self::TarXz => "tar.xz",
            Self::TarZst => "tar.zst",
            Self::TarBz2 => "tar.bz2",
            Self::Lzh => "lzh",
            Self::Iso => "iso",
            Self::Gz => "gz",
            Self::Xz => "xz",
            Self::Zst => "zst",
            Self::Bz2 => "bz2",
            Self::Zpaq => "zpaq",
            Self::Rar => "rar",
            Self::Cab => "cab",
            Self::Wim => "wim",
            Self::Alz => "alz",
            Self::Egg => "egg",
        }
    }

    /// Formats that Oxizip can create (compress into).
    pub fn supports_create(self) -> bool {
        matches!(
            self,
            Self::Zip
                | Self::Zipx
                | Self::SevenZ
                | Self::ExeSfx
                | Self::Tar
                | Self::TarGz
                | Self::Lzh
                | Self::Iso
                | Self::Gz
                | Self::Xz
                | Self::Zpaq
        )
    }

    pub fn is_rar(self) -> bool {
        matches!(self, Self::Rar)
    }

    /// Parse a user-facing format name (CLI / GUI).
    pub fn parse_create_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "zip" => Some(Self::Zip),
            "7z" | "7zip" => Some(Self::SevenZ),
            "zipx" => Some(Self::Zipx),
            "exe" | "sfx" => Some(Self::ExeSfx),
            "tar" => Some(Self::Tar),
            "tgz" | "tar.gz" => Some(Self::TarGz),
            "lzh" | "lha" => Some(Self::Lzh),
            "iso" => Some(Self::Iso),
            "gz" | "gzip" => Some(Self::Gz),
            "xz" => Some(Self::Xz),
            "zpaq" => Some(Self::Zpaq),
            _ => None,
        }
    }

    pub fn create_choices() -> &'static [Self] {
        &[
            Self::Zip,
            Self::SevenZ,
            Self::Zipx,
            Self::ExeSfx,
            Self::Tar,
            Self::TarGz,
            Self::Lzh,
            Self::Iso,
            Self::Gz,
            Self::Xz,
            Self::Zpaq,
        ]
    }
}

/// Compression / codec selection for create (ZIP/ZIPX/7z).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodId {
    Store,
    Deflate,
    Bzip2,
    Lzma,
    Xz,
    Zstd,
    /// 7z default solid codec.
    Lzma2,
    Ppmd,
}

impl MethodId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::Deflate => "deflate",
            Self::Bzip2 => "bzip2",
            Self::Lzma => "lzma",
            Self::Xz => "xz",
            Self::Zstd => "zstd",
            Self::Lzma2 => "lzma2",
            Self::Ppmd => "ppmd",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "store" | "copy" | "none" => Some(Self::Store),
            "deflate" => Some(Self::Deflate),
            "bzip2" | "bz2" => Some(Self::Bzip2),
            "lzma" => Some(Self::Lzma),
            "xz" => Some(Self::Xz),
            "lzma2" => Some(Self::Lzma2),
            "zstd" | "zst" => Some(Self::Zstd),
            "ppmd" => Some(Self::Ppmd),
            _ => None,
        }
    }
}

/// ZIP password cipher when `CreateOptions::password` is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ZipCryptoKind {
    /// WinZip AES-256 (default).
    #[default]
    Aes256,
    Aes128,
    Aes192,
    /// Legacy ZipCrypto (weak; compatibility only).
    ZipCrypto,
}

impl ZipCryptoKind {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "aes" | "aes256" | "aes-256" => Some(Self::Aes256),
            "aes128" | "aes-128" => Some(Self::Aes128),
            "aes192" | "aes-192" => Some(Self::Aes192),
            "zipcrypto" | "legacy" => Some(Self::ZipCrypto),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntryMeta {
    pub name: String,
    pub is_dir: bool,
    pub uncompressed_size: Option<u64>,
    pub compressed_size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub method: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub password: Option<String>,
}

/// How to handle an output path that already exists on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    /// Replace the existing file.
    #[default]
    Overwrite,
    /// Leave the existing file; skip writing this entry.
    Skip,
    /// Write as `name (1).ext`, `name (2).ext`, …
    AutoRename,
}

#[derive(Debug, Clone)]
pub struct ExtractOptions {
    pub dest_dir: PathBuf,
    pub password: Option<String>,
    /// When set, only extract matching entry names (exact).
    pub entries: Option<Vec<String>>,
    pub conflict: ConflictPolicy,
    /// Copy the archive Zone.Identifier ADS onto extracted files (Windows).
    pub copy_zone_identifier: bool,
}

impl ExtractOptions {
    pub fn to_dir(dest_dir: impl Into<PathBuf>) -> Self {
        Self {
            dest_dir: dest_dir.into(),
            password: None,
            entries: None,
            conflict: ConflictPolicy::Overwrite,
            copy_zone_identifier: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateOptions {
    pub password: Option<String>,
    /// Compression level hint (format-specific; e.g. 0–9).
    pub level: Option<u32>,
    /// Codec override (ZIP/ZIPX/7z).
    pub method: Option<MethodId>,
    /// When set, split the finished archive into volumes of this many bytes.
    pub volume_size: Option<u64>,
    /// ZIP encryption algorithm when password is set.
    pub zip_crypto: ZipCryptoKind,
    /// Skip `Thumbs.db` when walking folders.
    pub exclude_thumbs_db: bool,
    /// When compressing one folder, store entries without that folder as root.
    pub no_root_for_single_folder: bool,
    /// Prefer UTF-8 ZIP name flag for Unicode names (default on).
    pub utf8_names: bool,
    /// Extra exclude patterns (e.g. `*.tmp`, `desktop.ini`); matched against file name.
    pub exclude_patterns: Vec<String>,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            password: None,
            level: None,
            method: None,
            volume_size: None,
            zip_crypto: ZipCryptoKind::default(),
            exclude_thumbs_db: true,
            no_root_for_single_folder: false,
            utf8_names: true,
            exclude_patterns: Vec::new(),
        }
    }
}

impl CreateOptions {
    /// True when `path`'s file name should be skipped by thumbs / exclude_patterns.
    pub fn should_exclude_file(&self, path: &std::path::Path) -> bool {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            return false;
        };
        if self.exclude_thumbs_db && name.eq_ignore_ascii_case("Thumbs.db") {
            return true;
        }
        for pat in &self.exclude_patterns {
            let pat = pat.trim();
            if pat.is_empty() {
                continue;
            }
            if pattern_matches(pat, name) {
                return true;
            }
        }
        false
    }
}

fn pattern_matches(pat: &str, name: &str) -> bool {
    if let Some(rest) = pat.strip_prefix("*.") {
        return name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(rest));
    }
    if let Some(prefix) = pat.strip_suffix('*') {
        return name
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase());
    }
    name.eq_ignore_ascii_case(pat)
}
