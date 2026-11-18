use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{CoreError, CoreResult};

/// Shared cancellation flag for long-running archive jobs.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    inner: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.inner.store(true, Ordering::SeqCst);
    }

    pub fn is_canceled(&self) -> bool {
        self.inner.load(Ordering::SeqCst)
    }

    /// Returns [`CoreError::Canceled`] when cancellation was requested.
    pub fn check(&self) -> CoreResult<()> {
        if self.is_canceled() {
            Err(CoreError::Canceled)
        } else {
            Ok(())
        }
    }
}
