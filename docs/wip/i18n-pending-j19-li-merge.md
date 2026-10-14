# J19 HDR batch strings pending translation

Translations are deferred by the owner. The new user-visible controls use the existing Library
translation helpers and shared widgets. New formatted messages use English `format!` fallbacks
because `tr_format!` only accepts existing catalog entries; their translations remain pending. Source: `crates/lc-ui-egui/src/merge.rs`.

| String | Location / purpose |
| --- | --- |
| `Automatically Group Brackets` | `body`: opt-in checkbox for shoot-wide detection and batch HDR |
| `Merge each detected set in the background. Preview shows the first set.` | `body`: explains the batch and preview behavior |
| `{groups} bracket sets · {skipped} ungrouped photos` | `body`: detected-set preview summary |
| `HDR Batch Merge` | `poll_batch`: nonmodal progress window title |
| `{} HDR merges added` | `batch_progress`: completed imports |
| `Cancelling…` | `batch_progress`: disabled cancel action while cancellation finishes |
| `HDR batch cancelled because the library changed` | `poll_batch`: protects the current library from an old worker result |
| `HDR batch: {completed} added · {failed} failed · {skipped} ungrouped photos` | `poll_batch`: completion, failure and skipped counts |
| `HDR batch cancelled: {completed} added` | `poll_batch`: partial results retained after cancellation |
| `HDR batch failed: {e}` | `poll_batch`: worker/import failure |

Existing `Cancel`, `Auto Align`, `Show Deghost Overlay`, `Auto Settings`, `Create Stack`, and the
stage/progress message retain their existing translation helpers and keys.

Engine diagnostics surfaced through the existing merge error UI also need structured keys in the
later translation pass:

- `automatic brackets require HDR` (`spawn`, defensive option validation)
- `no complete HDR bracket sets found` (`batch_preview`, detection produced no previewable set)
- `batch HDR requires desktop background workers` (`spawn`, browser targets cannot run the native threaded batch worker)

The engine batch worker adds dynamic stage text `HDR set {index} of {total}: {stage}`; the future
translation pass should translate structured components rather than each generated string.

Worker stage strings: `Preparing HDR result`, `Recovering camera response`, `Merging radiance`, `Removing ghosts`, `Normalising radiance`, `HDR merge complete`.

Raw/HDR stages pending: `Checking raw brackets`, `Preparing sensor registration`, `Detecting sensor ghosts`, `Fusing sensor observations`, `Developing merged sensor`, `Raw HDR ready`, `Detecting HDR brackets`, `Detected HDR brackets`, `Delivering DNG`, `Batch HDR complete`, `Reading raw brackets`, `Waiting for HDR memory`, `Reading aligned raw brackets`.

New merge validation diagnostics pending: nonempty/valid HDR dimensions; invalid pixels/exposure/clipping; finite exposure-ratio range; positive/finite rendered exposure factors; 256 finite response values; raw matching camera calibration/layout/crop/orientation; unsupported aligned irregular CFA (tripod fallback advice); minimum raw dimensions/aspect ratio; positive finite analog balance; finite fused samples; mismatched sensor extents; hardware memory need/headroom; HDR memory estimate overflow. Source literals are in `lc-merge/src/{hdr,raw_hdr,editor/hdr}.rs` and `lc-engine/src/merge.rs`; keep structured numeric fields when localizing.

Exact new backend validation/stage literals for the later translation pass:

`crates/lc-merge/src/raw_hdr.rs`:
- `Checking raw brackets`
- `Detecting sensor ghosts`
- `Developing merged sensor`
- `Fusing sensor observations`
- `Preparing sensor registration`
- `Raw HDR fusion produced non-finite sensor samples`
- `Raw HDR ready`
- `Raw HDR requires finite positive camera analog balance`
- `Raw HDR requires matching camera calibration, sensor layout, crop and orientation`
- `Raw HDR sensor aspect ratio is unsupported`
- `Raw HDR sources must be at least four pixels per side`
- `Raw bracket exposure ratio is not finite`
- `Subpixel raw HDR alignment currently requires a Bayer sensor; disable alignment for tripod brackets on other CFA layouts`

`crates/lc-merge/src/hdr.rs`:
- `HDR exposure ratio exceeds the supported finite range`
- `HDR input has invalid pixels, exposure or clipping level`
- `HDR merge needs nonempty images with valid dimensions`

`crates/lc-merge/src/editor/hdr.rs`:
- `HDR exposures must be positive and finite`
- `HDR merge complete`
- `HDR merge needs at least one image`
- `HDR pixels must be finite`
- `HDR response needs 256 finite values per channel`
- `Merging radiance`
- `Normalising radiance`
- `Recovering camera response`
- `Removing ghosts`
