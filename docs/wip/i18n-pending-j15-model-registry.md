# Pending English strings — J15 model registry

Translations remain deferred. No control labels were added. These backend validation errors
can reach the existing model-download error display when an embedded descriptor is malformed.

| Source | English string | Context |
|---|---|---|
| `crates/li-ai/src/inventory.rs` | `Model file {} has no valid size/SHA-256 pin` | Reject an unpinned model descriptor before download or filesystem changes. |
| `crates/li-ai/src/inventory.rs` | `Invalid model filename: {}` | Reject a model descriptor whose filename could escape the segmentation folder. |

The host-refusal wording and the downloader's existing transport/verification errors are reused.
