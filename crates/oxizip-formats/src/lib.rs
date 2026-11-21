//! Format probing and archive readers/writers.

mod alz_read;
mod cab_read;
mod create;
mod egg_read;
mod iso_read;
mod lzh_read;
mod password_recover;
mod rar_unrar;
mod sevenz_edit;
mod sevenz_read;
mod sevenz_repair;
mod sniff;
mod tar_family;
mod traits;
mod types;
mod util;
mod volume;
mod wim_read;
mod zip_comment;
mod zip_edit;
mod zip_format;
mod zip_repair;
mod zpaq_ffi;
mod zpaq_read;

pub use alz_read::AlzReader;
pub use cab_read::CabReader;
pub use create::open_writer;
pub use egg_read::EggReader;
pub use iso_read::IsoReader;
pub use lzh_read::LzhReader;
pub use password_recover::{recover_password, RecoverPasswordOptions};
pub use rar_unrar::{find_unrar, RarUnrarReader};
pub use sevenz_edit::{sevenz_add_paths, sevenz_delete_entries, sevenz_rename_entry};
pub use sevenz_read::SevenZReader;
pub use sevenz_repair::repair_sevenz;
pub use sniff::{sniff_bytes, sniff_path};
pub use tar_family::{is_tar_family, open_tar_family, TarReader};
pub use traits::{ArchiveReader, ArchiveWriter};
pub use types::{
    ConflictPolicy, CreateOptions, EntryMeta, ExtractOptions, FormatId, ListOptions, MethodId,
    ZipCryptoKind,
};
pub use util::{resolve_conflict_path, unique_extract_path};
pub use volume::{first_volume_path, is_volume_path, join_volumes, maybe_split_archive};
pub use wim_read::{find_wimlib, WimReader};
pub use zip_comment::{zip_read_comment, zip_write_comment};
pub use zip_edit::{zip_add_paths, zip_delete_entries, zip_rename_entry};
pub use zip_format::{ZipReader, ZipWriterBackend};
pub use zip_repair::repair_zip;
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
        FormatId::Cab => Ok(Box::new(CabReader::open(path)?)),
        FormatId::Wim => Ok(Box::new(WimReader::open(path)?)),
        FormatId::Alz => Ok(Box::new(AlzReader::open(path)?)),
        FormatId::Egg => Ok(Box::new(EggReader::open(path)?)),
    }
}

pub fn open_zip_writer(path: &Path) -> CoreResult<Box<dyn ArchiveWriter>> {
    open_writer(path, FormatId::Zip)
}
