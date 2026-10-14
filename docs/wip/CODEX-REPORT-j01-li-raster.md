# J1 — shared raster backend

Status: complete; checks, strict clippy and all focused tests passed. Changes are uncommitted. The pre-existing task-file edit was retained.

## Merged

- **D1:** new `li-raster` owns the resampling kernels, Pixel arithmetic, dense/half-size filtering, sparse streamed surface filtering, and RGB/RGBA/Gray u8 adapters. `lc-raster` and `pc-algo` retain their public entry points. Editor enum serde names/defaults and the Preserve Details post-sharpening remain unchanged. Clamp, default-pixel, negative document coordinates, sparse tile skipping and the editor's per-read budget survive the move. One kernel definition evaluates at the required precision: f64 editor taps; compatibility f32 Library/GPU tap tables. The inherited AI resizer's image-rs arithmetic is preserved for opaque bytes, with premultiplied filtering for alpha.
- **D1 AI:** migrated all 30 resize sites in the six assigned files (29 production calls and one existing ignored integration-test resize): 22 RGBA, six RGB, two Gray. The earlier builder's `li-ai::resize::lanczos3` becomes a power-scheduler adapter to the shared implementation, so restoration callers already using it benefit too. Filters, dimensions, image colour metadata and PNG/wire boundaries remain unchanged. No float-frame migration from J28 was attempted.
- **D2:** shared exact sampled Gaussian through sigma 4, Kovesi three-box filtering above it, #706 guard, premultiply/unpremultiply, running-sum streaming lines, clipped box means and min/max filters. Library/editor blur, fx helpers, matting, pyramid/content-aware boxes, mask feather and burst-test blur delegate. CF matting reaches shared means through matting; seam has no plain blur duplicate to replace.
- **D2 specialized policies retained:** variance-corrected `gaussian_fine` keeps its GPU tap/order contract and box fallback at sigma 2; fx fields retain their halo-compatible boxes; sparse selection Feather keeps its measured cosine-fit error bound and zero border, with Smooth's repeated border. Their implementations now live in the shared backend. Provenance-pinned darktable kernels remain intact.
- **D4:** shared `RgbHistogram` keeps exact endpoint clipping, under/overflow, transparency and invalid-sample counts; gains a luma channel, validated working-space coefficients (Rec.709/Rec.2020 or custom) and stride sampling. PC reexports it. LC adapts back to its existing serialized Vec/u32 fields, integer encoded-luma bins and ~1 MP sampling contract. The new luma storage adds 2,048 bytes, with no image-sized histogram temporary.
- **GPU dispatch integration:** Develop now uploads shared sampled taps to existing `conv_h`/`conv_v` shaders for small default Gaussian radii; large Gaussian and fine-policy fallback still use boxes. Editor GPU mask planning already consumes `pc-compose::masks::combined_mask`, so it receives the shared CPU feather automatically. No shader/device/VRAM policy changed.

## Measured quality and compatibility

Small deterministic samples, as allowed by the task's minimum-testing override:

| Gate / metric | Before | After |
|---|---:|---:|
| LC resize adapter, four current filters | Baseline byte hashes | All four identical |
| PC resize adapter, all seven methods including Preserve Details | Baseline byte hashes | All seven identical |
| Opaque AI u8 filters, five image-rs methods, 80 small cases | image 0.25.10 | All bytes identical |
| Prior AI Lanczos opaque RGB/RGBA, one/four threads, 40 size/thread cases per pixel type | image-rs reference | All bytes identical |
| CC0 disc cutout, enlarged to 257×193: over-white edge MAE (u8 levels) | 7.881239 | 0.017785 |
| Same cutout, reduced to 65×49: over-white edge MAE | 7.792271 | 0.024155 |
| 0.5 sigma impulse, default blur variance (px²) | 0 (no blur) | 0.21501245 |
| 0.5 sigma fine blur variance / desired sigma² | 0.25 / 0.25 | 0.25 / 0.25 |
| Default/fine impulse mass | default 1 | 0.9999998 / 1 |
| Hidden blue at middle of transparent blur fringe | 0.89328533 | 0 |
| Field box max absolute error against direct f64 mean | 9.673833847e-7 | 4.730814573e-8 |

The CC0 cutout is an original synthetic antialiased constant-colour disc, explicitly dedicated to CC0-1.0 in the test. Its known colour gives an analytical composite reference. Error decreases by >99.6% in both directions, measured through the production `imaging::pad_to` path. No client/owner images were read or uploaded.

Byte goldens were captured from the branch's existing adapters before moving the code; the fixture/hash assertions remain in the adapters. The preserved byte contracts are deliberate edge/precision policies: kernel consolidation does not silently change opaque exports or GPU resize tables. The default Gaussian follows the existing editor's sampled reference; fine Gaussian retains the separate variance-corrected sharpening policy.

## Speed and memory sample

One warmed, synthetic 1024×768 → 640×480 opaque Lanczos sample on the real CPU, through the production power-scheduler entry and `imaging::pad_to`, in the normal optimized dev profile. Automatic Rayon concurrency was 16; no fixed worker count was set. Results describe this single sample on the shared machine, not a throughput benchmark or general hardware guarantee:

| Path | Before (ms) | After (ms) |
|---|---:|---:|
| Earlier builder's parallel AI resize → shared `li-ai::resize::lanczos3` | 4.384 | 3.335 |
| Earlier serial resize used by pad_to → actual `imaging::pad_to` (includes overlay) | 37.979 | 5.755 |

Both output buffers were asserted byte-identical. The sample records a 24% lower elapsed time for the inherited parallel entry and about 6.6× less time in pad_to. Only one timing sample was run, as requested; no benchmark suite or sweep. The temporary example was removed and its source/log retained under `target/j01-baseline/{measurement.rs,sample.log}`.

Allocation changes are verifiable from the implementation: the AI float intermediate changes from `source_width × target_height × 16` bytes to `source_width × 16` bytes per active worker (7,864,320 → 16,384 for this sample; at 16 active workers, at most 262,144 row-intermediate bytes), and dense LC resize replaces its full-height horizontal intermediate with one horizontal row per output band. Output buffers and tap tables remain required. The editor's existing 64 MB source-read budget is preserved; this is not a universal peak-memory cap.

All row work uses the current Rayon pool/default hardware concurrency; the legacy AI entry retains `lightcraft_sysmem::power::compute`. No processor-specific thread count or new allocator/dependency was introduced.

## Intentional rendered/test changes

- Default Library Gaussian and compositor feather now blur at sigma 0.5. Library sigma <=4 uses the editor's exact sampled kernel instead of three rounded boxes. Compositor large-radius feather uses the Kovesi widths instead of three equal-width boxes.
- RGBA dense resize/half/blur and AI cutout resizing interpolate premultiplied colour, preventing hidden RGB from contaminating visible edges. Identity and true nearest u8 operations preserve input bytes.
- Pyramid/content-aware plain box sums use f64 accumulation; the measured error reduction above is sub-8-bit and may change float-derived results slightly.
- `TEST-CHANGED: crates/lc-raster/src/blur.rs::fine_gaussian_keeps_variance_and_mass_at_small_sigma` — replaces the previous default Gaussian(0.5)==input assertion with an assertion that it blurs. Fine variance/mass checks remain.
- `TEST-CHANGED: crates/li-ai/src/resize.rs::enhance_parallel_lanczos_byte_identical_to_image_serial` — retained as `j01_shared_resize_parallel_lanczos_opaque_byte_identical_to_image_serial`, with opaque RGBA inputs. Transparent images intentionally improve and are tested separately against the analytical CC0 composite.

Old documents/settings remain compatible: no serialized settings or document shapes changed. UI controls, strings, Match Look, client files and ComfyUI services were untouched.

## Verification

Commands use `PATH=~/.cargo/bin:$PATH`, `cargo +1.98.1`, `--offline`, the retained worktree `target/`, and the launcher-provided `CARGO_BUILD_JOBS=4`. One cargo process for this job at a time.

- Format: all seven changed crates.
- Check: seven changed crates passed; `lightcraft-raster --no-default-features` also passed.
- Focused new/rewritten tests: `cargo +1.98.1 test --offline -p li-raster -p lightcraft-raster -p photocraft-algo -p photocraft-compose -p li-ai -p lightcraft-engine -p lightcraft-gpu j01_ -- --nocapture` — **24 passed, zero failed**: shared 12, LC raster 4, PC algo 3, AI 2, compose 1, engine 1, GPU host 1. Other tests were filtered out.
- Strict clippy `--all-targets -- -D warnings`: all seven changed crates passed. The pre-existing craft-fonts build-script message is unchanged.
- No full crate/workspace suites, GPU acceptance runs, ComfyUI jobs or long measurements were run, per the newest task rule. The added GPU-host test verifies shared sampled tap packing/mass without creating a GPU device. Real-device parity remains for the coordinator's merged acceptance run; no GPU performance/equivalence claim is made here.
- A test-build error in a newly added byte histogram fixture (exclusive `0..256` inferred as u8) was fixed to `0..=255`; final focused pass is green. Failure log retained in `target/j01-baseline/tests.log`. Strict clippy initially identified a constant `chunks_exact(4)` in a new test; changed it to `as_chunks::<4>()` and the final strict pass succeeded.
- Final nearest-tap test expansion was verified independently with the same `j01_final_dense_alpha_half_nearest_and_empty` filter (one passed); the total above counts unique focused tests.
- Local logs and before-source snapshots: `target/j01-baseline/`; no source data left in tracked fixtures.

## Unavoidable outside edits and scope boundaries

- Five consumer `Cargo.toml` files plus `Cargo.lock`: wire the new path crate and preserve LC's no-default-feature serial behavior. Lockfile adds only the local `li-raster` package and edges to existing `image`, `rayon`, `serde`; no new registry versions or C/-sys dependencies.
- `crates/li-ai/src/resize.rs`: necessary to continue the prior builder's implementation without leaving another resampling engine behind; existing restoration callers retain the API.
- `crates/lc-gpu/src/render.rs`: necessary to make Develop's GPU default-blur dispatch follow the new shared CPU policy. Existing convolution shaders are reused; fine's boxes fallback remains compatible.
- UI histogram/painter migration is J18; export kernel lists are J27; unified dense Frame/AI float-wire work is J28; segmentation resizes outside assigned AI files belong to subsequent migrations. No edits to those scopes. `li-ai::mock` keeps its nearest-only test-server fixture resize.

## Sources followed

- `docs/wip/AUDIT-duplicate-systems-fable.md` §1, D1/D2/D4 and J1; current branch resample/blur/histogram/selection implementations, including #706, #713 and the earlier builder's AI resize.
- Installed, locked image-rs 0.25.10 `src/imageops/sample.rs` (MIT OR Apache-2.0): tap bounds, vertical/horizontal order, Catmull-Rom/Gaussian/Lanczos arithmetic and u8 rounding; inherited prior-builder reference provenance remains in the shared u8 source.
- `lc-pipeline/src/output.rs::OutputSpace::luma`: working-space luma coefficients; the shared histogram accepts those coefficients without converting sample domains.
- Locally sampled darktable `~/.local/share/local-image-dev/engine-sources/dt-src/src/common/gaussian.c`: its faithful recursive-port border/order differ from these plain Gaussian helpers, so those ports are preserved.
- [Adobe's official resampling options](https://helpx.adobe.com/photoshop/desktop/crop-resize-transform/resize-adjust-resolution/resampling-options.html): preserves the established enlargement/reduction roles of Bicubic Smoother/Sharper. Mathematical coefficients come from the existing implementation; this task does not establish Photoshop pixel parity.
