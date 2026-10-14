# Pending English diagnostics — J11 shared file publication

Translations remain deferred. No UI labels or controls were added. Existing save-error wording moved unchanged to `li-fs`.

| Source | English diagnostic | Context |
|---|---|---|
| `crates/li-fs/src/lib.rs` | `filesystem does not support exclusive hard-link publication` | Default injectable filesystem capability. |
| `crates/li-fs/src/lib.rs` | `filesystem does not support explicit installation permissions` | Default injectable filesystem capability. |
| `crates/li-fs/src/lib.rs` | `new-file candidates must share a directory` | Reject invalid unique-name candidates. |
| `crates/li-seg/src/restoration.rs` | `Restore model destination must not be a symlink` | Reject installation into a symlink before changing its referent. |
| `crates/li-ai/src/trash.rs` | `trash copy differs from original` | Reject failed read-back verification before publishing a copy. |
| `crates/li-fs/src/lib.rs` | `{path} was published, but staging cleanup or permission synchronization failed: {error}` | Windows hard-link scratch cleanup restores and syncs final readonly permissions; post-publication failures are reported truthfully. |
| `crates/li-fs/src/lib.rs` | `atomic no-replace rename is unavailable` | Default/platform syscall capability; selects the safe link/copy fallback. |
| `crates/li-fs/src/lib.rs` | `exclusive verified copy is unavailable` | Custom injected filesystem adapter lacks this operation; RealFs implements it. |
| `crates/li-fs/src/lib.rs` | `exclusive copy is unavailable` | Internal missing owned handle. |
| `crates/li-fs/src/lib.rs` | `the published copy reads back different` | Reject and remove a newly created fallback destination if verification fails. |
