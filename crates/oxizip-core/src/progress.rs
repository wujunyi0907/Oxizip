/// High-level stage of an archive job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStage {
    Detect,
    List,
    Extract,
    Create,
    Test,
}

/// Progress payload reported by the engine/formats layer.
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub stage: JobStage,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
    pub entries_done: u64,
    pub entries_total: Option<u64>,
    pub current_entry: Option<String>,
    pub message: Option<String>,
}

impl ProgressUpdate {
    pub fn new(stage: JobStage) -> Self {
        Self {
            stage,
            bytes_done: 0,
            bytes_total: None,
            entries_done: 0,
            entries_total: None,
            current_entry: None,
            message: None,
        }
    }
}

/// Consumer of progress events. Implementations must be cheap and non-blocking.
pub trait Progress: Send + Sync {
    fn report(&self, update: ProgressUpdate);
}

/// No-op progress sink.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullProgress;

impl Progress for NullProgress {
    fn report(&self, _update: ProgressUpdate) {}
}

/// Progress that forwards to a function (handy for CLI).
pub struct FnProgress<F>(pub F)
where
    F: Fn(ProgressUpdate) + Send + Sync;

impl<F> Progress for FnProgress<F>
where
    F: Fn(ProgressUpdate) + Send + Sync,
{
    fn report(&self, update: ProgressUpdate) {
        (self.0)(update);
    }
}
