//! Format probing and archive readers/writers.

mod create;
mod iso_read;
mod lzh_read;
mod rar_unrar;
mod sevenz_read;
mod sniff;
mod tar_family;
mod traits;
mod types;
mod util;
mod volume;
mod zip_format;
mod zpaq_ffi;
mod zpaq_read;

pub use create::open_writer;
pub use iso_read::IsoReader;
pub use lzh_read::LzhReader;
pub use rar_unrar::{find_unrar, RarUnrarReader};
pub use sevenz_read::SevenZReader;
pub use sniff::{sniff_bytes, sniff_path};
pub use tar_family::{is_tar_family, open_tar_family, TarReader};
pub use traits::{ArchiveReader, ArchiveWriter};
pub use types::{
    CreateOptions, EntryMeta, ExtractOptions, FormatId, ListOptions, MethodId, ZipCryptoKind,
};
pub use volume::{first_volume_path, is_volume_path, join_volumes, maybe_split_archive};
pub use zip_format::{ZipReader, ZipWriterBackend};
pub use zpaq_ffi::run_zpaq;
pub use zpaq_read::ZpaqReader;

use std::path::Path;

use oxizip_core::CoreResult;

/// Open a reader for the detected or given format.
pub fn open_reader(path: &Path, format: FormatId) -> CoreResult<Box<dyn ArchiveReader>> {
    match format {
        FormatId::Zip | FormatId::Zipx | FormatId::ExeSfx => Ok(Box::new(ZipReader::open(path)?)),
        FormatId::Tar
        | FormatId::TarGz
        | FormatId::TarXz
        | FormatId::TarZst
        | FormatId::TarBz2
        | FormatId::Gz
        | FormatId::Xz
        | FormatId::Zst
        | FormatId::Bz2 => Ok(Box::new(open_tar_family(path, format))),
        FormatId::Rar => Ok(Box::new(RarUnrarReader::open(path, None)?)),
        FormatId::SevenZ => Ok(Box::new(SevenZReader::open(path)?)),
        FormatId::Lzh => Ok(Box::new(LzhReader::open(path)?)),
        FormatId::Iso => Ok(Box::new(IsoReader::open(path)?)),
        FormatId::Zpaq => Ok(Box::new(ZpaqReader::open(path)?)),
    }
}

pub fn open_zip_writer(path: &Path) -> CoreResult<Box<dyn ArchiveWriter>> {
    open_writer(path, FormatId::Zip)
}
