//! The shared file writer ([`li_fs`]) as editor documents and exports use it: injected file-system
//! faults, rename retries, staged (cancellable) writes. See [`li_fs`] for the guarantees.
use li_fs::{AtomicFs, RealFs, RenameRetry, atomic_write, atomic_write_with, stage_write, stage_write_with};

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
mod staged_tests;
mod tests;
