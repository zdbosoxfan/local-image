# Shared core — N5, Q2, S2 CMS

Implementation continues the branch's existing Library import / Develop-layer work. Changes are
uncommitted. The task brief was already modified at start and was left untouched.
`docs/wip/REVIEW-holistic-fable.md` is absent from this checkout, so its executive summary, findings
and roadmap could not be read; this job follows the detailed N5/Q2/S2 requirements in CODEX-TASK.md.

## N5 — one encoder stack

Library's stable `lightcraft-codecs::encode` API now adapts to `photocraft-codecs`, the existing
codec crate with broader format/depth/metadata support. Both Library export and Compositing
Export As therefore use the same JPEG, PNG, TIFF, WebP and native AVIF implementations. The
Library parallel baseline JPEG implementation was moved into that shared crate; parallel PNG,
CMYK JPEG/TIFF, BigTIFF, layered TIFF, progressive JPEG and lossy WebP remain available. JPEG's
ICC APP2 chunker is shared by baseline, 4:2:2, CMYK and progressive/web exports. PNG/TIFF/WebP
ICC embedding comes from the same format writers. AVIF retains Library's higher-precision
10-bit encoder, now also used by the editor; its muxer supports nclx/EXIF, not ICC.

Six cross-path regressions include 13 fixed byte goldens (JPEG 4:4:4/4:2:0, RGB8 PNG/TIFF/WebP,
RGBA16 PNG/TIFF at every compression, float TIFF, RGBA8/16 AVIF), multi-segment ICC/4:2:2,
shared named profiles, the CMYK CMS fallback, and both stored/upright EXIF/XMP orientation policies. Full pixels, metadata and ICC settings agree
at the codec boundary. Document rendering, resizing and per-format capabilities remain separate
from this boundary. Library PNG now uses the shared balanced default rather than its former
fast default; Library gray+alpha TIFF preserves that layout instead of expanding it to RGBA.
The shared encoder has an explicit `keep_orientation` metadata policy: Library's low-level
borrowed API preserves camera-order metadata, and editor document exports normalize it after
orienting pixels. Equal pixels/settings/metadata policy give equal bytes. Integer TIFF DPI is
written canonically as n/1; fractional resolutions retain 0.001-dpi precision.

## Q2 — one raw developer

Deleted `pc-raw`, its manifest, examples, fuzz package and obsolete decoder-specific tests.
Removed every runtime/test dependency on it. Its independent synthetic file generators moved
unchanged to `lc-raw`'s opt-in `testgen` feature so IO/engine/PSD/live-Develop tests survive.

`pc-io::raw::is_raw` uses Library's probe. Default raw import, explicit Develop settings and smart
object refresh decode through `lc-engine::files` and render through its CPU/GPU Develop path.
Default import uses Library's camera white balance, capture sharpening, colour NR and embedded
lens defaults. Unsupported variants use Library's upright camera preview with a clear warning;
malformed data remains an error. The bounded one-source decode cache is retained and reused.

The existing Compositing open, canvas-drop/Place and standalone `LOCAL_IMAGE_NO_LIBRARY` paths
already call these IO/engine entry points, so no UI fork remains. Embedded smart objects keep
the raw bytes; linked smart objects keep their source path; DevelopLink/settings/PSD records
are unchanged. New Nikon TIFF/EP route tests cover import, open, Place and linked/embedded
refresh; existing PSD and Develop-layer tests cover serialization and editable links.

## S2 CMS — implemented

Library ICC parsing, custom-profile writing and LUT/CMYK conversion now delegate to the same
pure-Rust `photocraft-cms` parser/writer/Transform used by Compositing. Named output profiles
are the exact shared built-in bytes, including sRGB/P3/Adobe RGB/ProPhoto/Rec.2020. Both halves
use one cached linear Rec.2020 working profile. Existing Library IccInfo/TRC serialization is
preserved by adapters. Old document/catalog/settings formats still load; no serialized fields
were added or removed.

Remaining S2 slices: shared i18n, shared metadata services, and a widgets crate. They were not
started. New/retained raw diagnostics are listed in [i18n-pending-shared-core.md](i18n-pending-shared-core.md). No UI controls
changed, so no widget/style or click/drag test changes were needed.

## Dependencies and ownership

No new external package/version was added to Cargo.lock and no new C/C++ or -sys dependency
was introduced. Existing workspace dependencies were reused: photocraft-codecs/cms, ravif for
the moved AVIF writer, jpeg-decoder for the moved independent JPEG tests, jpeg-encoder as the existing Library benchmark oracle (dev-only), and lc-gpu for explicit
hardware parity validation. The lock removes jpeg-encoder 0.6.1, moxcms 0.9.1 and photocraft-raw;
only jpeg-encoder 0.7.1 remains. moxcms 0.8.1 remains for unrelated existing dependencies and the
CMS's independent test oracle. No new third-party code was ported.

Edits outside the core implementation: IO/engine test imports/manifests for the fixture move;
two codec README updates; shared profile helper in pc-io's Develop-filter adapter; root
workspace/lock cleanup. No Match Look, UI, GPU-engine, catalog or application files changed.
No GUI windows opened; no running app or service stopped; no Desktop paths accessed.

## Validation

Full suite totals (unit/integration/doc tests), run offline with Rust 1.98.1 and inherited
`CARGO_BUILD_JOBS=4`, one Cargo process at a time:

| Crate | Passed | Ignored | Features |
|---|---:|---:|---|
| photocraft-cms | 165 | 1 | default |
| photocraft-codecs | 589 | 3 | avif |
| lightcraft-codecs | 117 | 1 | default |
| lightcraft-raw | 224 | 4 | testgen |
| photocraft-io | 511 | 4 | default |
| photocraft-engine | 2116 | 34 | default |
| **Total** | **3722** | **47** | |

Zero remaining failures. Full suites ran once for each changed crate; the two codec suites were
rerun after the Library compatibility failures below. No whole-workspace suite was run. Ignored
benchmarks/corpus/hardware cases are not credited as hardware evidence. Two of those ignored
cases (the new GPU and CC0 NEF tests) passed explicitly, as described below.

Offline check passed for
Library codecs/IO/engine and for Library codecs without default features. `cargo +1.98.1 fmt
--check` passed for all six changed crates. Clippy `--offline --all-targets -- -D warnings` passed
for all six. The existing build-script warning about absent optional craft-fonts remains.

Explicit hardware test `raw_shared_real_gpu_parity` passed on **NVIDIA GeForce RTX 5090
(Vulkan)**, with an actual `lc-gpu::render` result required; no software adapter or skipped result
counts. Shared import pixels matched that direct Library GPU render exactly. The 96x64 synthetic
case's import took 3.78 ms after GPU initialization. This test is ignored in ordinary suites and
was run explicitly with `--ignored --nocapture` (one pass).

The explicitly run `raw_cc0_nef_editor_routes` passed on the verified Nikon D70 file (3040x2014,
6.12 MP). Actual editor IO/command paths measured initial import **5364.00 ms**, cached file open
**309.18 ms**, embedded Place **643.11 ms**, linked Place **692.20 ms**. It also reloaded both raw
smart sources and compared their pixels. These are one-machine observations, not cross-hardware
promises; timings include CPU decode/development and rendering, and exclude reading the test file.
The app's canvas-drop handler calls this Place path. No desktop window or GUI latency claim is
made. Cache limits and parallelism remain hardware-independent.

The first full Library codec suite found five compatibility regressions: JPEG/PNG/WebP and
embedded-thumbnail EXIF orientation, plus TIFF's integer-resolution fraction. The shared
metadata policy and canonical DPI writer restore all original assertions; no tests were
weakened. A sixth cross-path regression now checks both EXIF and XMP orientation policies.
Only the five DPI-bearing TIFF goldens were regenerated for the canonical n/1 representation.
The two affected codec full suites and all-six-crate clippy then passed. The downstream IO
orientation filter subsequently passed 7 tests (zero failures); the no-default-feature
codec check and all-six-crate formatting check also passed again after that fix.

The preserved focused-test log caught a CR3 diagnostic case mismatch (`Cr3` vs `CR3`), which was
fixed without weakening the test. The first clippy run caught the existing JPEG comparison
example's missing dev-only dependency after moving the runtime encoder; its benchmark oracle
now uses the same locked 0.7.1 version. Logs are retained under target/shared-core-*.log and
shared-*-tests*.log.

## Established sources followed

- Existing branch APIs/implementations: lc-engine files/media, lc-catalog Photo::camera_defaults,
  pc-codecs writers/parallel PNG, lc-codecs parallel JPEG/10-bit AVIF, pc-cms ICC implementation,
  pc-io DevelopLink serialization and pc-engine Place/source-refresh paths.
- [ICC profile embedding technical note](https://archive.color.org/files/technotes/ICC-Technote-ProfileEmbedding.pdf)
  and [ICC.1:2022](https://www.color.org/specification/ICC.1-2022-05.pdf): JPEG APP2 sequencing,
  65,519-byte ICC payloads, 4-byte ICC tag alignment, profiles/PCS/TRCs.
- [PNG specification](https://www.w3.org/TR/png-3/): iCCP/eXIf/pHYs chunks and 16-bit sample order.
- [Adobe Photoshop Place help](https://helpx.adobe.com/photoshop/desktop/create-open-import-images/import-files/place-a-file-in-photoshop.html)
  and [linked Smart Objects help](https://helpx.adobe.com/photoshop/desktop/create-manage-layers/smart-objects/create-linked-smart-objects.html):
  preserve original data and live source links through placement/refresh.
- Installed jpeg-encoder 0.7.1 source/docs for its owned APP payload and PixelDensity API.
- [raw.pixls.us](https://raw.pixls.us/), CC0 Nikon D70 repository sample 2060, downloaded only to
  `target/shared-core-data/nikon-d70-cc0.nef`; SHA256
  `dd6405aeb33b0cd5bf66c98ba98ccbb478a765450cfd130810e470dab8d1f4b4` verified against its repository index.
