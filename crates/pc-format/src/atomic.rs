//! Compatibility API for the shared Local Image file writer.
//! See [`li_fs`] for durability, cancellation and publication guarantees.
pub use li_fs::{AtomicFs, Policy, RealFs, RenameRetry, StagedWrite, atomic_write, atomic_write_with, retry_rename, stage_write, stage_write_with, temp_name};

#[cfg(test)]
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(test)]
mod staged_tests;
#[cfg(test)]
mod tests;
