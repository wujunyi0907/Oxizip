use std::path::Path;

use oxizip_core::{CancellationToken, CoreResult, Progress};

use crate::types::{CreateOptions, EntryMeta, ExtractOptions, FormatId, ListOptions};

pub trait ArchiveReader {
    fn format(&self) -> FormatId;

    fn list(
        &mut self,
        opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<Vec<EntryMeta>>;

    fn extract(
        &mut self,
        opts: &ExtractOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()>;

    fn test(
        &mut self,
        opts: &ListOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()> {
        // Default: list succeeds as a weak integrity check; formats may override.
        let _ = self.list(opts, progress, cancel)?;
        Ok(())
    }
}

pub trait ArchiveWriter {
    fn format(&self) -> FormatId;

    fn add_path(
        &mut self,
        path: &Path,
        opts: &CreateOptions,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()>;

    fn finish(
        self: Box<Self>,
        progress: &dyn Progress,
        cancel: &CancellationToken,
    ) -> CoreResult<()>;
}
