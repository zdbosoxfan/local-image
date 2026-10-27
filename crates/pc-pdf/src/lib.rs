#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![expect(clippy::let_underscore_must_use, clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(test, expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed"))]

pub mod export;
pub mod import;
pub use import::{ImportError, PageBox, PageInfo, PdfInfo, PdfOptions, import, import_with, info};
