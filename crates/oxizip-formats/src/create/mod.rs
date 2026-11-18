mod iso;
mod lzh;
mod sevenz;
mod sfx;
mod single;
mod tar_write;
mod zpaq;

use std::path::Path;

use oxizip_core::{CoreError, CoreResult};

use crate::traits::ArchiveWriter;
use crate::types::FormatId;
use crate::zip_format::ZipWriterBackend;

pub use iso::IsoWriter;
pub use lzh::LzhWriter;
pub use sevenz::SevenZWriter;
pub use sfx::ExeSfxWriter;
pub use single::{GzWriter, XzWriter};
pub use tar_write::{TarGzWriter, TarWriter};
pub use zpaq::ZpaqWriter;

pub fn open_writer(path: &Path, format: FormatId) -> CoreResult<Box<dyn ArchiveWriter>> {
    match format {
        FormatId::Zip => Ok(Box::new(ZipWriterBackend::create(path)?)),
        FormatId::Zipx => Ok(Box::new(ZipWriterBackend::create_zipx(path)?)),
        FormatId::SevenZ => Ok(Box::new(SevenZWriter::create(path)?)),
        FormatId::ExeSfx => Ok(Box::new(ExeSfxWriter::create(path)?)),
        FormatId::Tar => Ok(Box::new(TarWriter::create(path)?)),
        FormatId::TarGz => Ok(Box::new(TarGzWriter::create(path)?)),
        FormatId::Lzh => Ok(Box::new(LzhWriter::create(path)?)),
        FormatId::Iso => Ok(Box::new(IsoWriter::create(path)?)),
        FormatId::Gz => Ok(Box::new(GzWriter::create(path)?)),
        FormatId::Xz => Ok(Box::new(XzWriter::create(path)?)),
        FormatId::Zpaq => Ok(Box::new(ZpaqWriter::create(path)?)),
        _ => Err(CoreError::UnsupportedFormat),
    }
}
