# RAW decoding report — N1 / Q1 / G1

Implemented with the owner's revised design: **unmodified crates.io rawler 0.8.0**, pinned normally
with default features disabled. No vendoring, fork, path dependency, patch, copied decoder source,
new C build dependency or new `-sys` package. The unfinished hand-port attempt was discarded.
All production glue is in `crates/lc-raw/src/rawler_adapter.rs`. Changes are uncommitted.

The existing native decoders remain first. Every native Unsupported result now tries rawler; native
Corrupt/Limit results remain errors. No model list gates fallback. Existing camera matrices and
file calibration win; missing calibration uses rawler's public runtime camera data and aliases.
New camera data therefore follows a rawler version bump. The adapter maps integer sensor data,
sensor-relative CFA, active-relative black levels/crop, white levels, WB, matrices and file orientation.

## Verified camera and variant support

Twenty new adapter samples from 13 bodies decode successfully, develop finite images, and have
**complete sensor fingerprints identical to independent LibRaw 0.22.2 unpacked buffers**.
Every header-only result agrees with full decode. Files are CC0 raw.pixls.us samples, stored only
under `target/rawdecode`, with source SHA-256 checked. Source URLs, source hashes, complete sensor
hashes and mapping goldens are in `crates/lc-raw/tests/fixtures/rawler-contract.tsv`.

| Body | Verified variants | raw.pixls.us IDs | Result |
|---|---|---|---|
| Canon EOS R | RAW; RAW 1.6× crop with dual-pixel | 4515, 4667 | Exact sensor reference; normal RAW development |
| Canon EOS R5 | RAW and C-RAW, each with and without dual-pixel | 4693, 4694, 4697, 4698 | Exact sensor references |
| Canon EOS R6 | RAW and C-RAW | 4662, 4660 | Exact sensor references |
| Canon EOS R7 | RAW and C-RAW | 5633, 5634 | Exact sensor references |
| Canon EOS R8 | Full-frame RAW | 6585 | Exact sensor reference |
| Fujifilm X-T20 | Lossless compressed X-Trans | 1178 | Exact sensor reference; compressed-mode header correction |
| Fujifilm X-T3 | Lossless compressed X-Trans | 2783 | Exact sensor reference |
| Fujifilm X-T4 | Lossless and lossy compressed X-Trans | 3914, 3918 | Exact sensor references |
| Fujifilm X-T5 | Lossless compressed X-Trans | 6122 | Exact sensor reference |
| Fujifilm X-H2S | Lossless compressed X-Trans | 6007 | Exact sensor reference |
| Fujifilm GFX 50S | Lossless compressed Bayer | 1435 | Exact sensor reference |
| Olympus E-M1 Mark II | Standard compressed ORF | 1998 | Exact sensor reference |
| OM System OM-1 | Standard compressed ORF | 5283 | Exact sensor reference |

Two additional native baselines are pinned: Nikon D750 lossless NEF (898) and Sony ILCE-7M3 ARW2
(2414). The D750 sensor agrees with LibRaw. The existing Sony decoder's ARW2 linearization differs
from LibRaw; its existing output is retained and locked, not replaced. No native pixel codec changed.

Other cameras in rawler's [supported-camera list](https://github.com/dnglab/dnglab/blob/v0.8.0/SUPPORTED_CAMERAS.md)
are attempted automatically, but were not all tested. Unknown bodies remain Unsupported/preview-only.
Dual-pixel files yield the main sensor image; dual-pixel corrections are not implemented. Filmrolls
select the first image; the app does not expose image-index selection. Four-colour/linear RGB and
rotated SuperCCD output remain Unsupported at this adapter boundary. Files over 512 MiB or 128
million sensor pixels are explicitly limited. These are the remaining documented boundaries, not
claims that untested bodies have been verified.

## Metadata, development and compatibility

A rawler 0.8.0 issue was found on X-T20: dummy/header decoding does not select the compressed camera
mode, while full decoding does. The adapter applies the same **public compressed-mode camera data**
to header results without decompressing the sensor. No upstream patch or model-specific workaround.
File EXIF orientation wins because rawler's output often defaults to Normal. An odd-origin synthetic
contract checks black-level reanchoring, crop offsets, WB and rotated orientation explicitly.

Mapping goldens follow rawler metadata. Differences from LibRaw's unpack-stage metadata are recorded,
not disguised as sensor mismatches: Canon specular whites 14008/12735 versus nominal 16383; ORF
white 4000 versus nominal 4095; slightly different crops; Fuji black-level data. The adapter's `bits`
field follows rawler's informational storage depth (16 here); the actual tested codec payloads are
14-bit CRX/RAF and 12-bit ORF. Existing engine DCP/profile handling remains active.

The actual engine `files::probe_bytes` and `files::load_bytes` paths were exercised on all **22** files:
all reported `preview_only=None`, all returned `SourceInfo.raw=true`, and all developed pixels were
finite. Thus these files take the normal RAW Develop path instead of embedded-JPEG development.
The existing preview-only import/reopen/reload compatibility regression still passes. Previously
preview-only catalog entries retain their stored status until Reload/Relink refreshes their file
information; no automatic catalog migration was added. No catalog,
settings or document schema changed. No UI, Match Look, SAM, icon, translation or AI workflow files
were changed, and no desktop/client photographs were read or used.

## Hardening

Preflight bounds file size, sensor dimensions, TIFF/maker-note counts and metadata bytes, nesting,
BMFF boxes and sample-table counts. It validates CRX tile/plane/subband shape and byte-size totals
before rawler constructs allocation-driving codec structures. Rawler's public decode API contains
codec panics and returns errors. No panic escaped the RAW API during 128 random recognized-container
cases, 27 real-file truncations, 24 metadata mutations and three damaged entropy payloads.

The damaged RAF and ORF entropy tests triggered **two internally caught upstream panics**, visible
in the retained log, which became Corrupt errors. This is panic containment, not a claim that rawler's
internal decoder code never panics. No process-global panic hook was changed. Limits are fixed safety
bounds independent of the owner's machine, and parallelism remains rawler/rayon's normal adaptive
behavior. Nothing was tuned to this PC.

## Validation and measurements

Commands used `cargo +1.98.1`, `--offline`, the same worktree target directory and the launcher's
unchanged `CARGO_BUILD_JOBS=4`, with one Cargo compile lane at a time.

| Check | Outcome |
|---|---|
| `cargo check --offline -p lightcraft-raw` | Pass |
| `cargo fmt -p lightcraft-raw` | Pass |
| `cargo clippy --offline -p lightcraft-raw --all-targets -- -D warnings` | Pass on final source |
| Full `lightcraft-raw` suite | **230 passed**, 9 ignored, 0 failed (155 unit, 15 coverage, 60 integration; no doc tests) |
| `rawler_contract -- --include-ignored --nocapture` | **11 passed** (4 unit + 7 integration); all 22 goldens exercised |
| Native/raw quality checks | Existing quality unit tests passed in full suite; standalone `quality` example completed |
| Engine preview-only import/reopen/reload regression | **1 passed**, 0 failed |
| Generated app attribution consistency test | **1 passed**, 0 failed |
| Actual engine probe/source harness | **22/22** import and source checks passed |
| Independent LibRaw 0.22.2 harness | **20/20** new adapter sensor hashes exact; native NEF exact, native ARW baseline retained |
| Real-GPU existing `render_bench`, EOS R6 RAW | RTX 5090 Vulkan; CPU/GPU image max difference 1 LSB, mean 0.0001 LSB |

The first full RAW run found five old tests asserting the public entry point stopped at native
Unsupported. They now assert the native decoder's original classification and separately verify
that the malformed synthetic public inputs still fail. The initial failure log is retained, and
the suite was rerun after this justified test-contract correction. No tests were removed or skipped
to obtain the pass. A final allocation review added guards for older-RAF record dimensions and TIFF sample/tile counts;
the affected contracts and full RAW suite were verified again on the final source. Only the changed
crate's full suite was run; engine and attribution checks were filtered.

Observed CPU decode timings from independent one/four-worker pools (one run, shared machine load;
not universal performance targets):

| File/path | 1 worker | 4 workers |
|---|---:|---:|
| EOS R6 RAW, 20.6 MP | 695 ms | 268 ms |
| EOS R6 C-RAW, 20.6 MP | 705 ms | 196 ms |
| X-T5 compressed RAF, 40.9 MP | 1972 ms | 560 ms |
| E-M1 II compressed ORF, 20.5 MP | 549 ms | 278 ms |
| Existing D750 NEF, 24.3 MP | 263 ms | 279 ms |
| Existing ILCE-7M3 ARW2, 24.3 MP | 102 ms | 27 ms |

Actual engine imports were 3.7–34.3 ms for the new adapter cases; 512-pixel source development was
167–735 ms. The existing NEF header path still decodes samples (186 ms import in this run).
Real RTX 5090 interactive measurements used the existing app Develop implementation, not a surrogate:
EOS R6 full-source loading 1115 ms; warm exposure change at 1920×1280 **43.9 ms**; draft highlights
17.5 ms; draft clarity 14.8 ms; draft NR 51.0 ms; full loupe NR 73.2 ms. First device/kernel setup
was 3338 ms. No new GPU tests or shaders were added: this is CPU codec work, verified with an existing
real-GPU benchmark and CPU/GPU comparison. No GUI window was opened and no app/service was killed.

Logs, CC0 data, independent harness source/headers/executable and benchmark artifacts remain in
`target/rawdecode` and `target/refvec`. The engine source/probe harness was temporary (retained under
`target/refvec/engine_rawdecode.rs`); no engine source file remains modified.

## Dependency, licensing and maintenance

The crates.io registry pin is `=0.8.0`; Cargo.lock checksum is
`51cb9d46a5b34946878eb236d1180adec0806eee260738ce299517b083a232e8`.
The upstream tag resolves to commit `ae01bcb2d0f8a74f9dfbb9f7b5c7e315c8e668b7`.
The rawler feature set is empty: rawdb/ureq/TLS, clap and inspector are disabled. The normal/build
Linux tree has no C compiler/build dependency, C codec or `-sys` crate. Exactly **22 new Rust
packages** were added to Cargo.lock, listed in `docs/RAW-DECODERS.md`. Mandatory rawler libraries
such as libflate, jxl-oxide, backtrace and memmap2 are Rust; no C compilation is introduced.

The all-platform tree includes **pre-existing** js-sys (WebAssembly) and core-foundation-sys (Apple
OS framework declarations via chrono), plus libc ABI declarations. No new `-sys` package is in the
lockfile diff. These are explicitly disclosed rather than claiming the complete all-platform
workspace contains no OS bindings. Full normal/build and all-platform feature trees are retained
in `target/rawdecode/dependency-tree-*.txt`.

Rawler's LGPL-2.1-only licence is elected GPL under LGPL 2.1 section 3 for this GPL-3.0-or-later app.
The PORTS entry clearly records an **unmodified runtime dependency**, not a decoder hand port.
The upstream LGPL text and updated notice are included; app attributions are regenerated.
`docs/RAW-DECODERS.md` gives the exact one-version-bump procedure, dependency audit, CC0 contract and
raw-quality checks, engine verification and PORTS/attributions update steps.

Sources followed: [rawler 0.8.0 source/API](https://github.com/dnglab/dnglab/tree/v0.8.0/rawler),
[CR3 support state](https://github.com/dnglab/dnglab/blob/v0.8.0/CR3_STATE.md),
[supported cameras](https://github.com/dnglab/dnglab/blob/v0.8.0/SUPPORTED_CAMERAS.md),
[raw.pixls.us CC0 repository](https://raw.pixls.us/), and the system
[LibRaw 0.22.2 public API](https://github.com/LibRaw/LibRaw/tree/0.22.2/libraw). Existing lc-meta CR3,
TIFF/maker-note helpers and engine development paths were reused. The holistic review and RAW
research arrived late in the worktree; its executive summary, Q1/G1 findings and N1 roadmap row were read. This change resolves that decoder blocker. The research
claim that rawler lacks Fuji lossy RAF is contradicted by the verified X-T4 fixture above; broader
per-ISO sensor tables, demosaic/denoise and lens quality work remain separate roadmap items.
No product/UI convention needed to be inferred from proprietary-app help.

Changes outside lc-raw are limited to Cargo.lock, the sample-fetch script, documentation, licence
notice/text and generated app attributions. The owner's CODEX-TASK.md, holistic-review and RAW-research
files were left untouched. Everything remains uncommitted.


## Follow-up correction (2026-10-14)

The permanent head-to-head comparison found an adapter CFA transposition: rawler takes
(row,column), not (x,y). This was corrected with an asymmetric Bayer/X-Trans regression; six
Fuji pattern goldens changed intentionally, while sensor fingerprints did not. Broader malformed
TIFF replay also exposed an allocation gap in the original preflight, now closed. These findings
supersede the original report's limited metadata/hardening claims. Current per-format/compression
routing and the permanent reference harness are in `RAW-DECODER-COMPARISON.md`. Follow-up
median-of-five speed measurements remain deferred under the owner's explicit no-benchmark rule;
no earlier single-run timings have been relabeled as medians.
