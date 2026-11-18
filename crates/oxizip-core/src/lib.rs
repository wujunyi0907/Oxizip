//! Shared primitives for Oxizip: errors, progress, cancellation, and path safety.

#[cfg(not(target_pointer_width = "64"))]
compile_error!("Oxizip only supports 64-bit targets");

mod cancel;
mod error;
mod path_safe;
mod progress;

pub use cancel::CancellationToken;
pub use error::{path_display, CoreError, CoreResult, ErrorCode};
pub use path_safe::{safe_dest_path, validate_entry_name};
pub use progress::{FnProgress, JobStage, NullProgress, Progress, ProgressUpdate};
