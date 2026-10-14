# J06 — one image decoder backend

Status: complete. Offline check, strict clippy, formatting and all 52 focused new tests passed. Changes are uncommitted.

Continued the existing branch and its shared encoder/CMS work; did not replace or regenerate any existing export fixture. The only pre-existing uncommitted file was `docs/wip/CODEX-TASK.md`; it was left untouched.

## What merged

1. `photocraft-codecs` owns raster detection, decoding, limits, source precision and container colour hints. `lightcraft-codecs` retains its serialized `Format`, `DecodeOptions`, `Decoded`, thumbnail and colour APIs as adapters. Its separate PNG/TIFF/WebP/GIF/BMP/PSD/JXL pixel decoders are deleted; JPEG is a short adapter. Library still interprets ICC/TRCs into linear source RGB and converts to working Rec.2020 exactly as before.
2. One shared format/extension registry declares 33 raster aliases and 27 camera-raw aliases, including the existing Sony `.arq`. Library's `sniff` adapts shared `identify`; raster-only `detect` preserves the editor's TIFF-container classification for TIFF raws. The actual TIFF decoder still refuses sensor data. Raw recognition includes bounded SubIFD traversal, CR3 and vendor signatures. Open filters consult read capabilities; export filters remain separate. AVIF remains write-only.
3. HEIC opens through the Library decoder by forwarding the existing pure-Rust HEIF backend. HEIC/HEIF/HIF folder imports and pickers work. JPEG XL moves to the shared backend and is available in the editor, including File › Open. Library also gains the shared PNM/QOI/TGA/ICO/EXR/HDR support.
4. JPEG's metadata and entropy-data scanner is shared, including EXIF and MPF preview selection, truncation checks, coherent multipart ICC assembly and source dimensions. The DCT-scaled `jpeg-decoder` path moves into the shared backend, retaining fractional fit dimensions, source-pixel limits and optional rayon. Full-resolution CMYK/YCCK keeps the measured superior existing editor result; lossless high-bit JPEG and non-interleaved sequential fallback use `jpeg-decoder`.
5. `lc-tiff` owns bounded borrowed directory views in addition to its existing owned metadata/writer APIs. All pixel-codec TIFF, raw classification, orientation, Photoshop private-tag, JPEG thumbnail and EXIF hint reads use these views. The compatibility page-description APIs remain in `pc-codecs`. `lc-codecs::exif` is a facade.
6. EXIF thumbnail unlinking and dimension updates also move to `lc-tiff`; `pc-io` delegates. Patches preserve MakerNote offsets, unused inline padding and opaque payload bytes. All child directories and patch ranges are validated before modifying anything. This keeps the established in-place behavior; rebuilding a TIFF tree through the writer would relocate opaque MakerNotes.
7. PSD merged composites now use the existing hardened `photocraft-psd::PsdFile` / `ImageData::decode`, retaining 8/16/32-bit precision and metadata, and gaining ZIP/prediction support and proper bitmap/indexed handling. Editor layered PSD import/export continues using that same PSD backend.
8. TIFF's shared colour-interpreter entry point preserves associated device samples. Library removes integer association before TRC/CMS without quantizing, and retains float/HDR interpretation. Ordinary editor decode still returns straight samples. JPEG XL explicitly handles primary alpha/black channel selection, associated alpha, frame warnings and codestream orientation.
9. PNG cICP/sRGB/cHRM/gAMA interpretation moved with its decoder. `pc-io` carries container colour hints into the document as a matching generated ICC when no embedded ICC exists; otherwise the original profile is kept verbatim. This is necessary for linear Rec.2020 JXL to display correctly in Compositing. The Library adapter adopts the shared U8 buffer without an extra full-image copy. TIFF captures orientation during its existing directory selection instead of traversing the page tree again.

Explicit Library `max_pixels` overrides remain useful for large captures: its shared allocation budget is `max(512 MiB, max_pixels × 32)`, matching its existing TIFF request-based budget rather than imposing a new fixed ceiling. Pixel and checked-allocation guards still apply, independent of this PC's hardware. A no-allocation regression checks admission of a 100 MP RGB16 frame with an explicit 100 MP limit and rejection above that limit.

## Measured quality and speed

Small synthetic/reference samples only; no benchmark, long quality sweep, full corpus run, GPU acceptance run or GUI launch was performed.

| Gate/sample | Before | After |
|---|---|---|
| 11 existing synthetic JPEG/PNG/TIFF/WebP fixtures, including ProPhoto RGBA16, all four TIFF compression modes and float TIFF | Recorded Library linear RGB + alpha bit hashes before switching the adapter | All 11 hashes identical; fixtures unchanged |
| Synthetic 40×32 CMYK JPEG, device-sample MSE | 0.171875 | 0.171875; byte-identical to old editor output |
| Synthetic 40×32 YCCK JPEG, device-sample MSE | Library/jpeg-decoder 0.403711; editor/zune 0.402734 | Shared 0.402734, byte-identical to old editor output; alternatives differ by at most 2/255 |
| Synthetic lossless 12-bit JPEG sample, expected 2048/4095 | Library read native-endian bytes as big-endian: 8/65535; absolute error 0.500000028 | 32776/65535; absolute error 0.000007602 |
| HEIC 64×64 checker vs Apple's reference decode | Library unsupported | Maximum display-sample error **0/255**, including thumbnail path |
| Official 12-bit associated-alpha JXL conformance frame | Editor unsupported; Library assumed first extra channel, did not explicitly unassociate | Both decode; Library/editor linear samples agree within **1e-6**, matching alpha; HDR helper retains 3.0 and zero-alpha colour becomes zero |
| PSD ZIP/prediction | Library unsupported | Independent U16 and F32 expected sample arrays exact, including -0.25 and 2.0 |

Diagnostic single-call CPU timings at the actual Library codec entry point (microseconds; tiny fixtures, includes scheduling/cache effects, **not a production speed claim**):

| Fixture | Original Library | Shared adapter |
|---|---:|---:|
| RGB8 JPEG 4:4:4, 17×11 | 419.466 | 403.676 |
| RGB8 PNG, 17×11 | 141.338 | 81.739 |
| RGB8 TIFF, 17×11 | 84.519 | 276.317 |
| RGBA16 TIFF Deflate, 7×5 | 239.787 | 438.006 |
| Float TIFF, 5×3 | 16.610 | 67.949 |

The tiny TIFF samples expose added checked directory/page handling overhead, not an improvement in TIFF throughput. No broad speedup or measured peak-memory claim is made. DCT scaling remains in the thumbnail path, U8 adoption removes one adapter copy, borrowed IFD values avoid payload materialization, and all worker policies remain hardware-independent. HEIC's 64×64 Library decode took 204.818 µs in the final codec smoke run; it was previously unavailable. The real Library folder import of a `.HIF` plus preview load completed in **1.191408 ms** for that 64×64 fixture; it imported one catalog photo and loaded finite 64×64 working pixels.

## Validation

Followed the task's final **ABSOLUTE MINIMUM TESTING** override: offline `cargo +1.98.1 check`, strict `clippy --all-targets -- -D warnings`, and only specifically added tests, filtered by name. One cargo process at a time; launcher `CARGO_BUILD_JOBS=4` retained; all commands use this worktree's `target/`.

- `cargo check --offline` passed for all nine touched crates: `lightcraft-codecs`, `photocraft-codecs`, `lightcraft-tiff`, `photocraft-io`, `lightcraft-engine`, `photocraft-engine`, `lightcraft-mcp`, `photocraft-ui-egui`, `local-image`.
- Final strict clippy passed on those nine crates with all targets and `-D warnings`; final offline check and `cargo fmt -- --check` also passed. Logs: `target/j06-validation/{clippy-final,check-final,format-final}.log`.
- **52 specifically added tests passed**, filtered as listed below. The full crate/workspace/corpus suites were not run.

- The new PSD fixture initially failed because its builder was still configured as 8-bit; setting the fixture's declared depth to 16 fixed it. The original failure log remains `target/j06-validation/pc-psd.log`; no assertion was weakened.
- No GPU tests added or run: the changed paths are CPU decoding, metadata and extension selection. No UI widgets or layout changed; filter/filename tests exercise the existing shared dialog helper.
- The existing build-script advisory about unavailable `craft-fonts` remains; no font assets/configuration were changed.
- `git diff --check` passes for authored changes. The existing task file has an untouched EOF blank-line warning.

| Changed crate | Focused filters | Passed |
|---|---|---:|
| `lightcraft-codecs` | `--test shared_decode shared_decode`; `--lib j6_` | 6 |
| `lightcraft-tiff` | `--test shared_directory shared_directory` (13); final `shared_directory_camera_raw_unifies_vendor_sony_bigtiff_and_next_ifds` (1) | 14 |
| `photocraft-codecs` | `--lib j06`; `--lib shared_directory`; `--lib shared_psd`; `--lib j6_` | 22 |
| `lightcraft-engine` | `--lib shared_decode_` | 2 |
| `photocraft-io` | `--lib shared_decode_` | 1 |
| `lightcraft-mcp` | `--lib j6_` | 1 |
| `photocraft-engine` | `--lib j6_` | 1 |
| `photocraft-ui-egui` | `--lib j6_` | 3 |
| `local-image` | `--bin local-image shared_decode_` | 2 |
| **Total** | | **52** |

The final raw-policy case checks the one shared policy across classic/BigTIFF, both byte orders, Sony's linear YCbCr marker, vendor compression, next-IFD sensor data and ordinary JPEG-compressed colour TIFFs. Registry-consumer tests completed in 0.00 seconds per binary after compilation; the long waits were test-binary compilation, not broad suites or GPU work.

## Intentional changes / TEST-CHANGED

- **TEST-CHANGED J06:** no existing golden was regenerated and no existing test assertion was loosened. Removed private decoder unit tests are superseded by shared backend/adapter regressions; old public integration tests remain intact and were not run under the testing-budget override.
- **TEST-CHANGED J06:** lossless JPEG native-endian/source-range handling is corrected, and Library YCCK chooses the measured better shared result (up to 2/255 device-code change on the small comparison).
- **TEST-CHANGED J06:** HEIF/JXL container transforms are applied once and embedded orientation is normalized. JXL alpha/CMYK channel selection is corrected; associated alpha becomes straight for the editor.
- **TEST-CHANGED J06:** PSD flattened extra transparency and ZIP composites now decode through the established PSD backend. Damaged RLE is rejected rather than silently padded by the deleted private decoder.
- **TEST-CHANGED J06:** shared strict TIFF/EXIF directory handling rejects incomplete tables/header-overlapping directories, honors the first Orientation duplicate with valid scalar type/count, and supports BigTIFF in EXIF queries. Library now selects the first full-resolution TIFF page, matching the editor, rather than opening a reduced IFD0 preview. Invalid UTF-8 TIFF XMP is omitted, following the existing editor policy, rather than Library's former lossy text replacement. Valid XMP stays unchanged.
- Unsupported AVIF is excluded from opening filters; it remains available for export. All raw aliases share save protection and edited-copy behavior.

## Necessary edits outside the initial file list

- `pc-codecs/Cargo.toml`, `lc-codecs/Cargo.toml`, `pc-io/Cargo.toml`, `Cargo.lock`: connect existing workspace backends/features. **Zero added/removed locked package versions**, no new C/-sys package, no new third-party crate. Decoder dependencies no longer used by Library moved out of its runtime dependencies; existing example/test oracles remain dev dependencies.
- `pc-codecs/src/lib.rs`, `codecs/mod.rs`, new `codecs/jxl.rs` and `codecs/psd.rs`: expose and dispatch the shared decoders and preserve default orientation behavior.
- `pc-codecs/src/codecs/png.rs`, `codecs/tiff.rs`, `orientation.rs`: attach container hints and preserve source precision/association; remove remaining IFD scanners and redundant orientation traversal.
- `pc-io/src/flat.rs`: the necessary colour-hint-to-document ICC bridge plus shared EXIF patch adapters.
- `lc-engine/src/import.rs`, `apps/local-image/src/{services,library_host}.rs`: actual import/picker gates, otherwise `.heif`/`.hif` folder import and File › Open JXL would remain blocked. These are narrow registry substitutions plus focused tests.
- New focused tests and small licensed synthetic/conformance fixtures under `lc-codecs/tests`, `lc-tiff/tests`, `pc-codecs/tests/fixtures`; the requested report itself.

No Match Look files, desktop/client photos, persisted settings formats, UI tokens/widgets, running app, ComfyUI service or owner files were changed. No commit was made.

## Sources followed

- [Adobe Photoshop File Formats Specification](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/): big-endian planar samples, 1/8/16/32-bit depths, PSD/PSB sections, indexed palette and resources. Reused this repository's established `pc-psd` implementation rather than inventing a reader.
- [W3C PNG Third Edition, colour-space information](https://www.w3.org/TR/png-3/#11ColourSpaceInfo): container colour metadata; retained Library's established interpretation/priority rather than approximating tagged colours.
- TIFF 6.0 / BigTIFF structural rules already cited and implemented by `lc-tiff`; borrowed views reuse its byte order, field types and header validation, preserving existing page/metadata policies.
- Installed `jpeg-decoder` 0.3.2 source (`decoder/lossless.rs::convert_to_u8`, buffer-limit and scaling APIs): native-endian high-bit samples and bounded DCT scaling.
- [jxl-oxide 0.12.6 Render::stream](https://docs.rs/jxl-oxide/0.12.6/jxl_oxide/struct.Render.html#method.stream): oriented samples and primary alpha/black channel selection. [Official libjxl conformance](https://github.com/libjxl/conformance), BSD-3-Clause, pinned fixture commit/license/hash in its fixture README.
- [heic-rs synthetic fixtures](https://github.com/tbraun96/heic-rs/tree/999470f514c8e9a1a7c37ea44ba70cc32b1308fe/tests/fixtures), MIT OR Apache-2.0: macOS-encoded checker and Apple's PNG reference. Downloaded to `target/refvec/j06-heic`, then copied as tiny test fixtures with license/provenance. No photographs used.

The full corpus, full crate/workspace suites and GPU acceptance remain for the coordinator's single merged validation, as explicitly required by the task's testing override.
