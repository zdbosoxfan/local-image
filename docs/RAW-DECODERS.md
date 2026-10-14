# RAW decoder boundary and updates

`lc-raw` keeps its native decoders. A native `RawError::Unsupported` is delegated to **unmodified
crates.io rawler**, pinned to `=0.8.0` with default features disabled. Corrupt input and resource-limit
errors are retained. There are no model lists, vendored sources, forks, path dependencies or patches.
All integration code lives in `crates/lc-raw/src/rawler_adapter.rs`.

The adapter carries sensor samples, CFA, black/white levels, as-shot WB, matrices, active area,
crop and file orientation into `RawImage`. CFA remains sensor-relative; black levels are reanchored
at the active-area origin, and the default crop is relative to the active area. Our existing camera
matrix table wins, including file-provided calibration. Missing table entries use rawler's public
runtime camera database and aliases. A rawler version bump therefore brings new camera calibration
without regenerating that table. An unknown camera still reports Unsupported and retains the
engine's preview-only behavior. Successful files use the existing Develop path. Catalog entries previously marked preview-only
can use Reload/Relink to refresh their stored status; there is no automatic catalog migration.

rawler 0.8.0's RAF dummy decoder omits the compressed camera mode (notably X-T20). The adapter
applies the public compressed-mode camera data to headers, without decompressing them. File EXIF
orientation takes precedence because rawler 0.8.0 normally reports Normal. These corrections stay
at the boundary and do not patch rawler.

Container preflight limits files to 512 MiB, sensor images to 128 million pixels and 65535 per
axis, metadata to 32 MiB per TIFF graph, nesting to 10 TIFF / 12 BMFF levels, and sample tables
to 65536 entries. It validates TIFF/maker-note values, box and sample ranges, and the CRX
tile/plane/subband allocation graph before calling rawler. The public rawler decode API contains
codec panics and maps them to errors. Limits cover the tested 80/100 MP still-image families;
very large files remain explicitly limited. The adapter supports integer single-plane Bayer,
X-Trans and monochrome sensors. Four-colour/linear RGB and rotated SuperCCD output remain
Unsupported; CR3 filmrolls select the first image, and dual-pixel correction is not exposed.

## Exact update procedure

1. Bump the normal pinned version in `crates/lc-raw/Cargo.toml`. Do not add a `[patch]` or path dependency.
2. With network available, run `PATH=~/.cargo/bin:$PATH cargo +1.98.1 update -p rawler --precise VERSION`
   and `cargo +1.98.1 fetch`. Subsequent builds must use `--offline`.
3. Audit `cargo +1.98.1 tree --offline -p rawler --edges normal,build` and
   `cargo +1.98.1 tree --offline -p rawler --target all --edges normal,build,features` against Cargo.lock.
   Keep rawdb (ureq/TLS/network), clap and inspector disabled. Check all new build scripts and `links`:
   no new C/C++ compilation or C decoder / `-sys` libraries. `backtrace` and `memmap2` are approved
   Rust libraries using platform FFI. Do not replace the crate with a local fork to trim dependencies.
4. Download the pinned CC0 corpus under this worktree:
   `python3 scripts/rawdecode_samples.py`. The script verifies every source SHA-256. Set
   `RAWLER_CONTRACT_DIR` if using a different existing CC0 test-data directory.
5. Run the adapter contract and raw quality checks:

   ```sh
   PATH=~/.cargo/bin:$PATH cargo +1.98.1 test --offline -p lightcraft-raw --features testgen rawler_contract -- --include-ignored --nocapture
   PATH=~/.cargo/bin:$PATH cargo +1.98.1 test --offline -p lightcraft-raw quality
   PATH=~/.cargo/bin:$PATH cargo +1.98.1 run --offline -p lightcraft-raw --example quality
   ```

   The checked TSV pins complete sensor fingerprints and mapping fields for CR3 RAW/C-RAW,
   dual-pixel and crop variants, compressed Bayer/X-Trans RAF, and Olympus/OM ORF. NEF/ARW rows
   retain native decoder baselines. Header/full results must agree. Never blindly regenerate goldens:
   investigate changes and cross-check sensor output with LibRaw/RawSpeed before accepting them.
6. Run `cargo +1.98.1 fmt -p lightcraft-raw`, clippy (`--offline --all-targets -- -D warnings`), and the
   full `lightcraft-raw` test suite once. Keep `CARGO_BUILD_JOBS=4` and one compile lane per job.
7. Verify the engine's existing preview-only regression and real app path: import must no longer
   mark the supported CC0 files preview-only; `files::load_bytes` must return raw sources. Existing
   `render_bench` exercises loading and interactive Develop rendering on the real GPU without a window:
   `N=1 ONLY=drag cargo +1.98.1 run --offline -p lightcraft-engine --example render_bench -- CC0.CR3`.
8. Update this document, the rawler dependency row in `docs/PORTS.md`,
   `licenses/rawler-NOTICE.md`, any changed upstream licence texts, and regenerate app attributions:
   `cargo +1.98.1 run --offline -p xtask -- attributions`. Record camera/variant results and test counts.

## 0.8.0 dependency audit

The rawler feature set is empty. The enabled Linux normal/build tree has no C build tool,
C decoder library or `-sys` crate. Twenty-two packages were added to the existing lock, all Rust:
addr2line, adler32, backtrace, dary_heap, gimli, glob, libflate, libflate_lz77, md5, multiversion,
multiversion-macros, object, rawler, rle-decode-fast, rustc-demangle, serde_spanned, target-features,
toml 0.8, toml_datetime 0.6, toml_edit 0.22, toml_write and winnow 0.7.

The all-platform tree also reaches **existing** `js-sys` (WebAssembly bindings) and
`core-foundation-sys` (Apple OS framework declarations via chrono/iana-time-zone); neither adds
C source compilation, and neither is new in Cargo.lock. `libc` is Rust platform ABI declarations.
No ureq, TLS or network dependency is enabled by rawler. Its mandatory JPEG XL and deflate
libraries (`jxl-oxide`, `libflate`) are Rust. Its build script joins camera/lens TOML data in Rust.
Existing unrelated workspace dependencies are outside this addition; do not mistake the whole
workspace tree for the new rawler closure.

Camera data calibration is applied after this decoder boundary. The `testgen` contract uses
decoder-only samples/headers so per-camera calibration does not silently regenerate sensor
goldens. The calibrated header/full geometry and levels are audited separately by
[`CAMERA-DATA.md`](CAMERA-DATA.md).
