# Known failing tests report

Continued branch `wip/codex-known-failures` at `e698208d`, including the existing vector integration (`1974facf`, merged at `f73bc0ca`) and critical hotfix (`a027712b`). The only initial uncommitted change was the owner’s `docs/wip/CODEX-TASK.md`; it is preserved. No commits were created.

## Decisions and history

### Merge Layers undo

The test expectation is wrong. On the ds2-fix side (`c0652fae`), `Session::edit` saved the previous history state’s layer target, even if selection changed immediately before the edit. Vector integration `1974facf` explicitly captures `st.layer_target()` before publishing an edit. Thus merging selected `[a, c]` correctly restores both on undo, with `c` active. Keeping only `c` would discard the merge’s actual input selection. The merge implementation itself remains correct.

TEST-CHANGED: `layer_multi_cmds::tests::merge_selected_into_top_most` must restore the complete pre-merge selection and active layer on undo. Keep its pixel, bounds, placement, and depth coverage; explicitly select `c` for the separate single-layer Merge Down case, and check redo/undo round trips.

### Layers-panel reveal after undo

The test expectation is wrong for the same history change. The fixture selects `inner` immediately before creating Scratch, so undo must restore `inner`, not the old top-layer history target. `layer_reveal.rs` and its tests are identical on `c0652fae` and `1974facf`; the changed behavior comes from `Session::edit`, not scrolling or group expansion.

TEST-CHANGED: `layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view` checks the actual pre-edit target after undo, and separately creates a top-layer edit, scrolls to `inner`, then undoes to test revealing the top again. Preserve both themes, opening groups, manual collapse, visible-row stability, and scrolling in both directions.

### Borrowed legacy shape options

The test fixture is wrong. The regression originated in `28d67848`, carried by `c0652fae`: it protects borrowing `ShapeLayer` from an Arc snapshot while editing the legacy Corners popup. Vector package integration routes `finish_shape` through `shape.design.create`, whose `drawing::prepare` intentionally sets legacy `ShapeLayer::live` to None; native parameters live in `layer.vector.live_shape`. Production uses the shared native design toolbar (`vector_integration::context` → `shared_tools::State::context` → `drawing::controls`), not this legacy popup.

TEST-CHANGED: `vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing` explicitly creates a legacy rectangle with `shape.create` and retains the original popup drag and immutable-snapshot assertions. Add native design-shape coverage through the production context route in both personas, with detached preview, one undo step, and geometry/parameter undo-redo parity.

### Blend icon padding

The artwork is wrong relative to the existing icon contract. The padding and alpha assertions were introduced with vector toolbar package `e1e2a6ae`; `1974facf` adds its test lock without relaxing them. The shared `blend.svg` assets come from sourced-artwork commit `83a68b34`. The ds2-fix `c0652fae` only hardens generator error handling/destination creation. The outer 1.5-point inset gives a single pixel at 16px, allowing nested-image resampling to bleed into a corner. Adjust the blend wrapper’s inset in its generator and both generated styles. Preserve upstream paths, source hashes, colors, alpha parity, and all test thresholds.

## Photoshop/Affinity comparison

Checked official documentation on 2026-10-11. Photoshop/Affinity were not run; these comparisons use official documentation. Exact restoration of a multi-layer selection is a design inference supported by this repository’s explicit history contract, not a claimed observation of those applications.

- Adobe describes undo as reversing the last action, and Affinity describes history as restoring document edits. Restoring the immediate input of Merge/New Layer follows that model; undo must not choose an unrelated older selection. Sources: [Photoshop Undo/Redo](https://helpx.adobe.com/photoshop/desktop/get-started/set-up-toolbars-panels/use-undo-redo-commands.html), [Affinity Undo/Redo/History](https://affinity.help/designer2/en-US.lproj/pages/DesignAids/undo.html).
- Affinity defaults to auto-scrolling the selected object’s layer entry into view. Keep this behavior for the layer actually restored by undo. Adobe documents layer selection and organization, but the reviewed page does not specify this exact undo/scroll sequence. Sources: [Affinity Layers panel](https://affinity.help/designer2/en-US.lproj/pages/Panels/layersPanel.html), [Photoshop Layers panel](https://helpx.adobe.com/photoshop/desktop/create-manage-layers/get-started-layers/work-with-the-layers-panel.html).
- Photoshop exposes individual rectangle corner radii, and Affinity exposes editable radius controls. Retain corner editing for both stored shape representations; tests must use the matching UI and data. Sources: [Photoshop Rectangle tool](https://helpx.adobe.com/photoshop/desktop/draw-shapes-paths/create-shapes/draw-rectangles-with-the-rectangle-tool.html), [Affinity Corner Tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_corner.html).
- Those product docs do not specify raster alpha at icon corner pixels. The icon decision follows the repository’s existing 16/20/40px padding/style contract; no exact competitor icon metric is claimed.

## Required ancillary generator fix

The initial strict Clippy run exposed pre-existing unused `Art::tool`, `Art::label`, and `Art::recognition` fields in `xtask/src/tool_art_vector.rs`. Since the icon generator makes xtask a touched crate, the final gate must cover it too. Its generator now validates that these inventory descriptions are present before emitting each icon. This preserves the artwork and adds no lint allowances; the existing inventory/freshness suite covers this module.

## Validation

Rust and Cargo **1.98.1** (version output below). Every Cargo build, test, check, and Clippy invocation uses `--offline` and the real `li-rustc-gate` wrapper. `CARGO_BUILD_JOBS` is unset. This shell does not expose the rustup proxy; plain Cargo selects the required compiler. Cargo could not resolve the initial bare wrapper name, so subsequent invocations set its absolute path. The direct `rustfmt` executable was also unavailable on PATH; `cargo fmt` works and its final check passed.

- All four named failures reproduced before changes, then passed in the affected-module run.
- Engine: **24 passed**, including the complete `layer_multi_cmds` module and the existing general history-target test.
- UI: **27 passed**, including complete `fill_stroke_proxy`, `layer_reveal`, and `vector_ui` suites (including its nested path-selection/zero-axis tests). **2 existing GPU tests ignored** with their original annotations. No test was removed, newly ignored, or filtered out of the requested modules.
- Generator: **3 passed**, covering both changed xtask modules and generated asset/source freshness.
- Final suites total: **54 passed, 0 failed, 2 already ignored**. Each affected suite ran once after the final changes. The initial runs below are baseline reproductions, not repeated post-fix suites.
- `cargo check --offline --workspace`: **exit 0**.
- Strict Clippy, **all targets of all three touched crates**, with `-D warnings`: **exit 0** after the required inventory validation fix. No lint suppression was added.
- `cargo fmt --all -- --check` and final `git diff --check`: **exit 0**, no output.
- The existing missing embedded CJK-font build notice appears in the captured outputs. It is unrelated to this task; no compiler lint warning was tolerated in the strict Clippy gate.

The existing contact-sheet test regenerated `docs/design/vector-tool-icons.png`; it was visually inspected and is retained as the current generated artifact. Dimensions remain 1080 × 6528. Pixel comparison against HEAD finds changes only in zero-based rows 6 (Crop, a pre-existing asset/contact-sheet mismatch) and 21 (Gradient, which uses the fixed blend SVG). No Crop SVG or generator geometry was changed. Both icon styles still meet all original 16/20/40px alpha, occupancy, and padding assertions.

Changes remain uncommitted. The owner’s pre-existing `CODEX-TASK.md` edit is preserved. The external `land-known-failures.txt` is unchanged: its instruction says to remove entries after a fix lands. These four entries are ready to retire at landing.

## Exact validation commands and captured output

All commands ran from this worktree. Each build/test/check/Clippy command below was executed with the displayed wrapper environment, with stdout and stderr captured together. The capture paths live under `/tmp/codex-known-failures-logs/`; their complete contents are reproduced below. No successful test command uses the merge queue’s failure allowlist.

### Toolchain versions

```bash
rustc --version
cargo --version
```

Both exit codes: `0`.

```text
rustc 1.98.1 (48a229cea 2026-09-01) (Fedora 1.98.1-1.fc44)
cargo 1.98.1 (797e8a9bc 2026-08-05) (Fedora 1.98.1-1.fc44)
```

### Formatting application

```bash
cargo fmt --all
```

Applied after source edits and after the ancillary generator edit; exit code `0`, no output. Formatting performs no Rust compilation or dependency resolution. The final formatting check is separately recorded below.

### Initial wrapper lookup failure

```bash
RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-engine -p photocraft-ui-egui --lib -- merge_selected_into_top_most toolbar46_registered_icon_alpha_and_styles_at_16_20_40 a_new_active_layer_opens_its_groups_and_scrolls_into_view ds_finding_borrowed_shape_options_allow_corner_editing
```

Exit code: `101`.

<details>
<summary>Complete captured output</summary>

```text
error: could not execute process `li-rustc-gate rustc -vV` (never executed)

Caused by:
  No such file or directory (os error 2)
```

</details>

### Engine baseline (Cargo stops after this failure)

```bash
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate cargo test --offline -p photocraft-engine -p photocraft-ui-egui --lib -- merge_selected_into_top_most toolbar46_registered_icon_alpha_and_styles_at_16_20_40 a_new_active_layer_opens_its_groups_and_scrolls_into_view ds_finding_borrowed_shape_options_allow_corner_editing
```

Exit code: `101`.

<details>
<summary>Complete captured output</summary>

```text
   Compiling proc-macro2 v1.0.107
   Compiling unicode-ident v1.0.26
   Compiling quote v1.0.47
   Compiling cfg-if v1.0.5
   Compiling serde_core v1.0.229
   Compiling serde v1.0.229
   Compiling libc v0.2.189
   Compiling autocfg v1.5.1
   Compiling log v0.4.34
   Compiling libm v0.2.16
   Compiling bitflags v2.13.2
   Compiling memchr v2.8.3
   Compiling once_cell v1.21.4
   Compiling crossbeam-utils v0.8.23
   Compiling version_check v0.9.5
   Compiling either v1.18.0
   Compiling zerocopy v0.8.59
   Compiling equivalent v1.0.2
   Compiling foldhash v0.2.0
   Compiling rustix v1.1.5
   Compiling thiserror v2.0.21
   Compiling linux-raw-sys v0.12.1
   Compiling simd-adler32 v0.3.10
   Compiling crossbeam-epoch v0.9.21
   Compiling crossbeam-deque v0.8.8
   Compiling pin-project-lite v0.2.17
   Compiling rayon-core v1.13.0
   Compiling adler2 v2.0.1
   Compiling shlex v2.0.1
   Compiling find-msvc-tools v0.1.14
   Compiling generic-array v0.14.7
   Compiling itoa v1.0.18
   Compiling typenum v1.20.1
   Compiling num-traits v0.2.19
   Compiling zmij v1.0.23
   Compiling libloading v0.8.9
   Compiling cc v1.5.1
   Compiling syn v3.0.6
   Compiling syn v2.0.119
   Compiling crc32fast v1.5.2
   Compiling subtle v2.6.1
   Compiling pkg-config v0.3.34
   Compiling miniz_oxide v0.9.1
   Compiling tracing-core v0.1.36
   Compiling allocator-api2 v0.2.21
   Compiling serde_json v1.0.151
   Compiling dlib v0.5.3
   Compiling hashbrown v0.17.1
   Compiling parking_lot_core v0.9.12
   Compiling rand_core v0.10.1
   Compiling flate2 v1.1.10
   Compiling getrandom v0.4.3
   Compiling num-integer v0.1.47
   Compiling hashbrown v0.16.1
   Compiling miniz_oxide v0.8.9
   Compiling const-oid v0.9.6
   Compiling scopeguard v1.2.0
   Compiling rayon v1.12.0
   Compiling lock_api v0.4.14
   Compiling stable_deref_trait v1.2.1
   Compiling winnow v1.0.4
   Compiling cfg_aliases v0.2.2
   Compiling anyhow v1.0.104
   Compiling zune-core v0.5.3
   Compiling fdeflate v0.3.7
   Compiling toml_datetime v1.1.1+spec-1.1.0
   Compiling quick-error v2.0.1
   Compiling spin v0.9.9
   Compiling wayland-sys v0.31.11
   Compiling siphasher v1.0.4
   Compiling weezl v0.1.12
   Compiling synstructure v0.14.0
   Compiling pxfm v0.1.30
   Compiling cpufeatures v0.2.17
   Compiling slab v0.4.12
   Compiling zune-jpeg v0.5.15
   Compiling photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-cms)
   Compiling memmap2 v0.9.11
   Compiling itertools v0.15.0
   Compiling toml_parser v1.1.3+spec-1.1.0
   Compiling indexmap v2.14.2
   Compiling byteorder-lite v0.1.0
   Compiling color_quant v1.1.0
   Compiling polling v3.11.0
   Compiling lazy_static v1.5.0
   Compiling png v0.18.1
   Compiling nom v8.0.0
   Compiling static_assertions v1.1.0
   Compiling image-webp v0.2.4
   Compiling gif v0.14.2
   Compiling toml_edit v0.25.15+spec-1.1.0
   Compiling uuid v1.26.1
   Compiling getrandom v0.2.17
   Compiling moxcms v0.8.1
   Compiling wayland-backend v0.3.17
   Compiling proc-macro-crate v3.5.0
   Compiling downcast-rs v1.2.1
   Compiling scoped-tls v1.0.1
   Compiling parking v2.2.1
   Compiling rand_core v0.6.4
   Compiling quick-xml v0.41.0
   Compiling linebender_resource_handle v0.1.1
   Compiling futures-core v0.3.34
   Compiling getrandom v0.3.4
   Compiling endi v1.1.1
   Compiling wayland-client v0.31.15
   Compiling unicode-width v0.2.2
   Compiling matrixmultiply v0.3.11
   Compiling base64ct v1.8.3
   Compiling raw-window-handle v0.6.2
   Compiling same-file v1.0.6
   Compiling memo-map v0.3.4
   Compiling pem-rfc7468 v0.7.0
   Compiling fastrand v2.5.0
   Compiling rawpointer v0.2.1
   Compiling walkdir v2.5.0
   Compiling typeid v1.0.3
   Compiling writeable v0.6.4
   Compiling rustc-hash v1.1.0
   Compiling fearless_simd v0.4.1
   Compiling futures-io v0.3.34
   Compiling nom-language v0.1.0
   Compiling wayland-scanner v0.31.11
   Compiling event-listener v5.4.2
   Compiling naga v30.0.1
   Compiling concurrent-queue v2.5.0
   Compiling downcast-rs v2.0.2
   Compiling strength_reduce v0.2.4
   Compiling erased-serde v0.4.10
   Compiling dyn-hash v1.0.0
   Compiling inventory v0.3.25
   Compiling ash v0.38.0+1.3.281
   Compiling dyn-clone v1.0.20
   Compiling paste v1.0.15
   Compiling bit-vec v0.9.1
   Compiling maplit v1.0.2
   Compiling event-listener-strategy v0.5.4
   Compiling futures-lite v2.6.1
   Compiling transpose v0.2.3
   Compiling bit-set v0.10.0
   Compiling codespan-reporting v0.13.1
   Compiling primal-check v0.3.4
   Compiling num-bigint v0.4.8
   Compiling spirv v0.4.0+sdk-1.4.341.0
   Compiling khronos-egl v6.0.0
   Compiling rustc-hash v2.1.3
   Compiling byteorder v1.5.0
   Compiling pastey v0.2.3
   Compiling bit-vec v0.10.1
   Compiling litemap v0.8.3
   Compiling bit-set v0.11.1
   Compiling wgpu-hal v30.0.1
   Compiling async-io v2.6.0
   Compiling itertools v0.14.0
   Compiling cpufeatures v0.3.1
   Compiling anymap3 v1.1.0
   Compiling alloc-no-stdlib v2.0.4
   Compiling presser v0.3.1
   Compiling alloc-stdlib v0.2.4
   Compiling accesskit v0.24.1
   Compiling xattr v1.6.1
   Compiling phf_shared v0.13.1
   Compiling ordered-float v5.5.0
   Compiling filetime v0.2.29
   Compiling errno v0.3.14
   Compiling utf8_iter v1.0.4
   Compiling glow v0.17.0
   Compiling async-task v4.7.1
   Compiling renderdoc-sys v1.1.0
   Compiling bytes v1.12.1
   Compiling hex v0.4.3
   Compiling signal-hook-registry v1.4.8
   Compiling num-rational v0.4.2
   Compiling phf_generator v0.13.1
   Compiling brotli-decompressor v5.0.3
   Compiling tar v0.4.46
   Compiling async-channel v2.5.0
   Compiling wgpu-core v30.0.1
   Compiling float-cmp v0.9.0
   Compiling ahash v0.8.12
   Compiling litrs v1.0.0
   Compiling icu_properties_data v2.3.0
   Compiling atomic-waker v1.1.2
   Compiling icu_locale_fallback_data v2.3.0
   Compiling percent-encoding v2.3.2
   Compiling async-signal v0.2.14
   Compiling piper v0.2.5
   Compiling chacha20 v0.10.2
   Compiling async-lock v3.4.2
   Compiling wgpu v30.0.1
   Compiling strict-num v0.1.1
   Compiling euclid v0.22.14
   Compiling icu_segmenter_data v2.3.0
   Compiling icu_normalizer_data v2.3.0
   Compiling document-features v0.2.12
   Compiling thiserror v1.0.69
   Compiling arrayref v0.3.9
   Compiling rustix v0.38.44
   Compiling rand v0.10.3
   Compiling async-process v2.5.0
   Compiling blocking v1.7.0
   Compiling async-executor v1.14.0
   Compiling async-broadcast v0.7.2
   Compiling guillotiere v0.7.0
   Compiling ff v0.13.1
   Compiling ordered-stream v0.2.0
   Compiling as-slice v0.2.1
   Compiling built v0.8.1
   Compiling av-scenechange v0.14.1
   Compiling semver v1.0.28
   Compiling xkeysym v0.2.1
   Compiling linux-raw-sys v0.4.15
   Compiling unicode-general-category v1.1.0
   Compiling base16ct v0.2.0
   Compiling xcursor v0.3.11
   Compiling cursor-icon v1.2.0
   Compiling aligned v0.4.3
   Compiling rustc_version v0.4.1
   Compiling group v0.13.0
   Compiling rand_distr v0.6.0
   Compiling no_std_io2 v0.9.4
   Compiling y4m v0.8.0
   Compiling roxmltree v0.20.0
   Compiling web-time v1.1.0
   Compiling parlance v0.1.0
   Compiling opaque-debug v0.3.1
   Compiling float-ord v0.3.2
   Compiling pastey v0.1.1
   Compiling base64 v0.22.1
   Compiling curve25519-dalek v4.1.3
   Compiling bitstream-io v4.10.0
   Compiling gethostname v1.1.0
   Compiling maybe-rayon v0.1.1
   Compiling rav1e v0.8.1
   Compiling x11-dl v2.21.0
   Compiling simd_helpers v0.1.0
   Compiling pulp v0.22.3
   Compiling quick-xml v0.38.4
   Compiling untrusted v0.9.0
   Compiling nohash-hasher v0.2.0
   Compiling as-raw-xcb-connection v1.0.1
   Compiling smithay-client-toolkit v0.19.2
   Compiling new_debug_unreachable v1.0.6
   Compiling unicode-segmentation v1.13.3
   Compiling epaint_default_fonts v0.36.2
   Compiling imgref v1.12.3
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-text)
   Compiling x11rb-protocol v0.13.2
   Compiling self_cell v1.3.0
   Compiling httparse v1.10.1
   Compiling noop_proc_macro v0.3.0
   Compiling winit v0.30.13
   Compiling aho-corasick v1.1.5
   Compiling raw-cpuid v11.6.0
   Compiling num-bigint-dig v0.8.6
   Compiling regex-syntax v0.8.11
   Compiling pollster v1.0.1
   Compiling pulp-wasm-simd-flag v0.1.1
   Compiling loop9 v0.1.5
   Compiling reborrow v0.5.5
   Compiling rustls v0.23.45
   Compiling jpeg-encoder v0.7.1
   Compiling lensfun v0.7.0
   Compiling xkbcommon-dl v0.4.2
   Compiling http v1.5.0
   Compiling accesskit_consumer v0.36.0
   Compiling rand_core v0.9.5
   Compiling blake3 v1.8.7
   Compiling regex-automata v0.4.18
   Compiling moxcms v0.9.1
   Compiling jpeg-decoder v0.3.2
   Compiling num-iter v0.1.46
   Compiling zune-inflate v0.2.54
   Compiling serde_derive v1.0.229
   Compiling bytemuck_derive v1.12.1
   Compiling thiserror-impl v2.0.21
   Compiling zerocopy-derive v0.8.59
   Compiling zeroize_derive v1.5.0
   Compiling tracing-attributes v0.1.31
   Compiling zerofrom-derive v0.1.8
   Compiling profiling-procmacros v1.0.18
   Compiling fax_derive v0.2.0
   Compiling yoke-derive v0.8.3
   Compiling enumflags2_derive v0.7.12
   Compiling zerovec-derive v0.11.6
   Compiling profiling v1.0.18
   Compiling displaydoc v0.2.7
   Compiling zeroize v1.9.1
   Compiling crypto-common v0.1.7
   Compiling block-buffer v0.10.4
   Compiling fax v0.2.6
   Compiling der v0.7.10
   Compiling derive-new v0.7.0
   Compiling tracing v0.1.44
   Compiling equator-macro v0.4.2
   Compiling bytemuck v1.25.2
   Compiling jxl-bitstream v1.1.0
   Compiling digest v0.10.7
   Compiling jxl-grid v0.6.2
   Compiling spki v0.7.3
   Compiling num-complex v0.4.6
   Compiling qoi v0.4.1
   Compiling font-types v0.12.5
   Compiling ndarray v0.17.2
   Compiling pkcs5 v0.7.1
   Compiling rustfft v6.4.1
   Compiling read-fonts v0.41.0
   Compiling jxl-oxide-common v1.0.0
   Compiling pkcs8 v0.10.2
   Compiling jxl-coding v1.0.1
   Compiling jxl-threadpool v1.0.0
   Compiling jxl-image v0.13.0
   Compiling color v0.3.3
   Compiling phf_macros v0.13.1
   Compiling sha2 v0.10.9
   Compiling jxl-modular v0.11.3
   Compiling equator v0.4.2
   Compiling naga-types v30.0.1
   Compiling gpu-allocator v0.28.0
   Compiling aligned-vec v0.6.4
   Compiling hmac v0.12.1
   Compiling v_frame v0.3.9
   Compiling signature v2.2.0
   Compiling wgpu-types v30.0.1
   Compiling inout v0.1.4
   Compiling thiserror-impl v1.0.69
   Compiling async-recursion v1.1.1
   Compiling serde_repr v0.1.21
   Compiling jxl-vardct v0.11.1
   Compiling async-trait v0.1.92
   Compiling jxl-frame v0.13.3
   Compiling cipher v0.4.4
   Compiling smallvec v1.16.2
   Compiling arrayvec v0.7.8
   Compiling polycool v0.4.0
   Compiling parking_lot v0.12.5
   Compiling photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-color)
   Compiling photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-geom)
   Compiling zvariant_utils v4.2.0
   Compiling minijinja v2.24.0
   Compiling kurbo v0.13.1
   Compiling skrifa v0.44.0
   Compiling photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-raster)
   Compiling harfrust v0.12.0
   Compiling zerofrom v0.1.8
   Compiling yoke v0.8.3
   Compiling photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-doc)
   Compiling zerovec v0.11.8
   Compiling enumflags2 v0.7.12
   Compiling zcheapstr v1.1.0
   Compiling string-interner v0.20.0
   Compiling lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-geom)
   Compiling tinystr v0.8.4
   Compiling potential_utf v0.1.6
   Compiling icu_locale_core v2.3.0
   Compiling zerotrie v0.2.5
   Compiling lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-color)
   Compiling wayland-protocols v0.32.13
   Compiling half v2.7.1
   Compiling tiff v0.11.3
   Compiling tract-data v0.23.8
   Compiling icu_provider v2.3.1
   Compiling image v0.25.10
   Compiling icu_collections v2.3.0
   Compiling photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-vector)
   Compiling zvariant_derive v5.15.0
   Compiling ppv-lite86 v0.2.21
   Compiling lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-raster)
   Compiling peniko v0.6.1
   Compiling phf v0.13.1
   Compiling icu_properties v2.3.0
   Compiling icu_locale_fallback v2.3.0
   Compiling vello_common v0.1.0
   Compiling wayland-protocols-wlr v0.3.12
   Compiling wayland-cursor v0.31.14
   Compiling wayland-csd-frame v0.3.0
   Compiling hkdf v0.12.4
   Compiling zvariant v5.15.0
   Compiling sec1 v0.7.3
   Compiling rgb v0.8.53
   Compiling font-types v0.11.3
   Compiling emath v0.36.2
   Compiling universal-hash v0.5.1
   Compiling crypto-bigint v0.5.5
   Compiling zbus_names v4.3.4
   Compiling rustls-pki-types v1.15.1
   Compiling zbus_xml v5.2.1
   Compiling arg_enum_proc_macro v0.3.4
   Compiling prost-derive v0.14.4
   Compiling zbus-lockstep v0.5.2
   Compiling vello_cpu v0.1.0
   Compiling ecolor v0.36.2
   Compiling elliptic-curve v0.13.8
   Compiling read-fonts v0.39.2
   Compiling wgpu-naga-bridge v30.0.1
   Compiling calloop v0.13.0
   Compiling icu_segmenter v2.3.0
   Compiling parley_data v0.11.1
   Compiling icu_normalizer v2.3.0
   Compiling fontique v0.11.1
   Compiling av1-grain v0.2.5
   Compiling lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-tiff)
   Compiling jxl-color v0.11.0
   Compiling zbus_macros v5.19.0
   Compiling wgpu-core-deps-windows-linux-android v30.0.1
   Compiling tract-linalg v0.23.8
   Compiling num-derive v0.4.2
   Compiling lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-meta)
   Compiling prost v0.14.4
   Compiling jxl-render v0.12.4
   Compiling epaint v0.36.2
   Compiling zbus-lockstep-macros v0.5.2
   Compiling calloop-wayland-source v0.3.0
   Compiling rand_chacha v0.3.1
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-develop)
   Compiling jxl-jbr v0.2.1
   Compiling kurbo v0.11.3
   Compiling parley v0.11.1
   Compiling avif-serialize v0.8.9
   Compiling tract-core v0.23.8
   Compiling rfc6979 v0.4.0
   Compiling photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-psd)
   Compiling tiny-skia-path v0.11.4
   Compiling x11rb v0.13.2
   Compiling curve25519-dalek-derive v0.1.1
   Compiling jxl-oxide v0.12.6
   Compiling zbus v5.19.0
   Compiling ravif v0.13.0
   Compiling svgtypes v0.15.3
   Compiling rand v0.8.8
   Compiling ecdsa v0.16.9
   Compiling egui v0.36.2
   Compiling skrifa v0.42.1
   Compiling rustls-webpki v0.103.15
   Compiling primeorder v0.13.6
   Compiling polyval v0.6.2
   Compiling photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-compose)
   Compiling wayland-protocols-plasma v0.3.12
   Compiling linesweeper v0.4.0
   Compiling calloop v0.14.4
   Compiling aead v0.5.2
   Compiling futures-macro v0.3.34
   Compiling simplecss v0.2.2
   Compiling atspi-common v0.13.0
   Compiling hayro-ccitt v0.3.0
   Compiling tract-nnef v0.23.8
   Compiling tract-hir v0.23.8
   Compiling ttf-parser v0.25.1
   Compiling dpi v0.1.2
   Compiling imagesize v0.13.0
   Compiling xmlwriter v0.1.0
   Compiling twox-hash v2.1.4
   Compiling bit_field v0.10.3
   Compiling atspi-proxies v0.13.0
   Compiling tract-extra v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling data-url v0.3.2
   Compiling smithay-client-toolkit v0.20.0
   Compiling wasmi_ir v2.0.0
   Compiling pico-args v0.5.0
   Compiling futures-task v0.3.34
   Compiling tract-onnx-opl v0.23.8
   Compiling wasmparser v0.228.0
   Compiling smol_str v0.2.2
   Compiling bumpalo v3.20.3
   Compiling lebe v0.5.3
   Compiling exr v1.74.2
   Compiling tract-onnx v0.23.8
   Compiling usvg v0.45.1
   Compiling futures-util v0.3.34
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-seg)
   Compiling atspi v0.29.0
   Compiling owned_ttf_parser v0.25.1
   Compiling accesskit_atspi_common v0.18.1
   Compiling ruzstd v0.9.0
   Compiling hayro-jbig2 v0.3.0
   Compiling ghash v0.5.1
   Compiling calloop-wayland-source v0.4.1
   Compiling photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-pathops)
   Compiling lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-codecs)
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-pipeline)
   Compiling lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-raw)
   Compiling write-fonts v0.48.1
   Compiling rand_chacha v0.9.0
   Compiling poly1305 v0.8.0
   Compiling lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-scenes)
   Compiling wayland-protocols-misc v0.3.12
   Compiling wayland-protocols-experimental v20250721.0.1
   Compiling chacha20 v0.9.1
   Compiling aes v0.8.4
   Compiling ctr v0.9.2
   Compiling ed25519 v2.2.3
   Compiling pkcs1 v0.7.5
   Compiling regex v1.13.1
   Compiling brotli v8.0.4
   Compiling hayro-jpeg2000 v0.3.5
   Compiling wasmi_core v2.0.0
   Compiling base64 v0.23.1
   Compiling ryu v1.0.23
   Compiling ab_glyph_rasterizer v0.1.10
   Compiling constant_time_eq v0.4.2
   Compiling wasmi v2.0.0
   Compiling option-ext v0.2.0
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-engine)
   Compiling hayro-postscript v0.1.0
   Compiling jpeg-encoder v0.6.1
   Compiling dirs-sys v0.5.0
   Compiling photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-codecs)
   Compiling hayro-cmap v0.1.0
   Compiling ureq-proto v0.6.4
   Compiling ab_glyph v0.2.32
   Compiling aes-gcm v0.10.3
   Compiling hayro-syntax v0.7.2
   Compiling pdf-writer v0.15.0
   Compiling chacha20poly1305 v0.10.1
   Compiling lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-merge)
   Compiling subsetter v0.2.6
   Compiling ed25519-dalek v2.2.0
   Compiling rsa v0.9.10
   Compiling rand v0.9.5
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-gpu)
   Compiling accesskit_unix v0.21.1
   Compiling p256 v0.13.2
   Compiling p384 v0.13.1
   Compiling x25519-dalek v2.0.1
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-catalog)
   Compiling webpki-roots v1.0.9
   Compiling rustls-webpki v0.102.8
   Compiling lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-preview)
   Compiling photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-algo)
   Compiling sha1 v0.10.7
   Compiling tiny-skia-path v0.12.0
   Compiling xmp-writer v0.3.3
   Compiling imagesize v0.14.0
   Compiling ascii v1.1.0
   Compiling wasmi_collections v2.0.0
   Compiling utf-8 v0.7.6
   Compiling bitflags v1.3.2
   Compiling utf8-zero v0.8.1
   Compiling httpdate v1.0.3
   Compiling chunked_transfer v1.5.0
   Compiling data-encoding v2.11.1
   Compiling tiny_http v0.12.0
   Compiling tungstenite v0.27.0
   Compiling rustls-rustcrypto v0.0.2-alpha
   Compiling ureq v3.4.2
   Compiling png v0.17.16
   Compiling krilla v0.8.2
   Compiling accesskit_winit v0.32.2
   Compiling smithay-clipboard v0.7.3
   Compiling hayro-interpret v0.7.0
   Compiling photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-format)
   Compiling dirs v6.0.0
   Compiling arboard v3.6.1
   Compiling photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-paint)
   Compiling type-map v0.5.1
   Compiling photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-raw)
   Compiling serde_spanned v1.1.1
   Compiling is-docker v0.2.0
   Compiling leb128fmt v0.1.0
   Compiling toml_writer v1.1.2+spec-1.1.0
   Compiling toml v1.1.6+spec-1.1.0
   Compiling wasm-encoder v0.261.0
   Compiling is-wsl v0.4.0
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-ai)
   Compiling photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-pdf)
   Compiling tiny-skia v0.11.4
   Compiling egui-winit v0.36.2
   Compiling photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-plugins)
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-io)
   Compiling li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-attributions)
   Compiling photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-vecfx)
   Compiling photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-svg)
   Compiling egui-wgpu v0.36.2
   Compiling photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-affinity)
   Compiling pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-trace)
   Compiling photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ops)
   Compiling enum-map-derive v0.17.0
   Compiling accesskit_consumer v0.35.0
   Compiling getopts v0.2.24
   Compiling colored v2.2.0
   Compiling rfd v0.17.2
   Compiling dify v0.8.0
   Compiling kittest v0.4.0
   Compiling eframe v0.36.2
   Compiling enum-map v2.7.3
   Compiling resvg v0.45.1
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ui-egui)
   Compiling wast v261.0.0
   Compiling open v5.4.4
   Compiling photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-gpu)
   Compiling tempfile v3.27.0
   Compiling memory-stats v1.2.0
   Compiling pollster v0.4.0
   Compiling egui_kittest v0.36.2
   Compiling wat v1.261.0
   Compiling egui_extras v0.36.2
   Compiling photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-testkit)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-engine)
   Compiling sys-locale v0.3.2
    Finished `test` profile [optimized + debuginfo] target(s) in 8m 42s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-0962a62fc0794aad)

running 1 test
test layer_multi_cmds::tests::merge_selected_into_top_most ... FAILED

failures:

---- layer_multi_cmds::tests::merge_selected_into_top_most stdout ----

thread 'layer_multi_cmds::tests::merge_selected_into_top_most' (2592026) panicked at crates/pc-engine/src/layer_multi_cmds.rs:1204:13:
assertion `left == right` failed: undo leaves a valid selection
  left: [LayerId(3), LayerId(5)]
 right: [LayerId(5)]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    layer_multi_cmds::tests::merge_selected_into_top_most

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1318 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p photocraft-engine --lib`
```

</details>

### UI baseline

```bash
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate cargo test --offline -p photocraft-ui-egui --lib -- toolbar46_registered_icon_alpha_and_styles_at_16_20_40 a_new_active_layer_opens_its_groups_and_scrolls_into_view ds_finding_borrowed_shape_options_allow_corner_editing
```

Exit code: `101`.

<details>
<summary>Complete captured output</summary>

```text
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-raw)
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-io)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-engine)
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 1m 44s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-219d99becdc1f52f)

running 3 tests
test vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing ... FAILED
test fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 ... FAILED
test layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view ... FAILED

failures:

---- vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing stdout ----

thread 'vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing' (2597126) panicked at crates/pc-ui-egui/src/vector_ui.rs:1215:11:
No nodes found matching the query:
By {
    label: "Corners",
}

On node:
Node {
    id: NodeId(
        TreeIndex(
            0,
        ),
        #13826925289788388,
    ),
    role: Window,
    focused: true,
    hidden: false,
    disabled: false,
    children: [
        Node {
            id: NodeId(
                TreeIndex(
                    0,
                ),
                #18356943641707106210,
            ),
            role: GenericContainer,
            focused: false,
            hidden: false,
            disabled: false,
            children: [
                Node {
                    id: NodeId(
                        TreeIndex(
                            0,
                        ),
                        #17717534622495252412,
                    ),
                    role: GenericContainer,
                    focused: false,
                    hidden: false,
                    disabled: false,
                    children: [
                        Node {
                            id: NodeId(
                                TreeIndex(
                                    0,
                                ),
                                #15707054694645992835,
                            ),
                            role: GenericContainer,
                            focused: false,
                            hidden: false,
                            disabled: false,
                            children: [
                                Node {
                                    id: NodeId(
                                        TreeIndex(
                                            0,
                                        ),
                                        #17970341196664973444,
                                    ),
                                    role: GenericContainer,
                                    focused: false,
                                    hidden: false,
                                    disabled: false,
                                    children: [
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #11830556613808507354,
                                            ),
                                            role: Label,
                                            value: "Fill:",
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #11448958627200295308,
                                                    ),
                                                    role: TextRun,
                                                    value: "Fill:",
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #13043090184002317183,
                                            ),
                                            role: ColorWell,
                                            label: "Set shape fill type",
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #13295944985128458684,
                                            ),
                                            role: Label,
                                            value: "Stroke:",
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #994044982341609940,
                                                    ),
                                                    role: TextRun,
                                                    value: "Stroke:",
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #10020356100740804199,
                                            ),
                                            role: ColorWell,
                                            label: "Set shape stroke type",
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #1270175527873071711,
                                            ),
                                            role: GenericContainer,
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #13496857253973271620,
                                                    ),
                                                    role: GenericContainer,
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [
                                                        Node {
                                                            id: NodeId(
                                                                TreeIndex(
                                                                    0,
                                                                ),
                                                                #47298008658375766,
                                                            ),
                                                            role: GenericContainer,
                                                            focused: false,
                                                            hidden: false,
                                                            disabled: false,
                                                            children: [
                                                                Node {
                                                                    id: NodeId(
                                                                        TreeIndex(
                                                                            0,
                                                                        ),
                                                                        #10815539049172973997,
                                                                    ),
                                                                    role: SpinButton,
                                                                    label: "Set shape stroke width",
                                                                    value: "0",
                                                                    numeric_value: 0.0,
                                                                    focused: false,
                                                                    hidden: false,
                                                                    disabled: false,
                                                                    children: [],
                                                                },
                                                            ],
                                                        },
                                                    ],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #15933507211848062659,
                                            ),
                                            role: Label,
                                            value: "W",
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #505267946083522733,
                                                    ),
                                                    role: TextRun,
                                                    value: "W",
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #14167684941103313182,
                                            ),
                                            role: GenericContainer,
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #14592570028842927727,
                                                    ),
                                                    role: GenericContainer,
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [
                                                        Node {
                                                            id: NodeId(
                                                                TreeIndex(
                                                                    0,
                                                                ),
                                                                #12532489775578050897,
                                                            ),
                                                            role: GenericContainer,
                                                            focused: false,
                                                            hidden: false,
                                                            disabled: false,
                                                            children: [
                                                                Node {
                                                                    id: NodeId(
                                                                        TreeIndex(
                                                                            0,
                                                                        ),
                                                                        #5719450152997338399,
                                                                    ),
                                                                    role: SpinButton,
                                                                    label: "W",
                                                                    value: "100",
                                                                    numeric_value: 100.0,
                                                                    focused: false,
                                                                    hidden: false,
                                                                    disabled: false,
                                                                    children: [],
                                                                },
                                                            ],
                                                        },
                                                    ],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #12564753222565639137,
                                            ),
                                            role: Label,
                                            value: "H",
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #17748517989447083129,
                                                    ),
                                                    role: TextRun,
                                                    value: "H",
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #5258922240090030388,
                                            ),
                                            role: GenericContainer,
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #9441937188134776915,
                                                    ),
                                                    role: GenericContainer,
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [
                                                        Node {
                                                            id: NodeId(
                                                                TreeIndex(
                                                                    0,
                                                                ),
                                                                #1345684395512731545,
                                                            ),
                                                            role: GenericContainer,
                                                            focused: false,
                                                            hidden: false,
                                                            disabled: false,
                                                            children: [
                                                                Node {
                                                                    id: NodeId(
                                                                        TreeIndex(
                                                                            0,
                                                                        ),
                                                                        #14672408847321526839,
                                                                    ),
                                                                    role: SpinButton,
                                                                    label: "H",
                                                                    value: "80",
                                                                    numeric_value: 80.0,
                                                                    focused: false,
                                                                    hidden: false,
                                                                    disabled: false,
                                                                    children: [],
                                                                },
                                                            ],
                                                        },
                                                    ],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #2315806300398560578,
                                            ),
                                            role: Label,
                                            value: "X",
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #9411206024217747724,
                                                    ),
                                                    role: TextRun,
                                                    value: "X",
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #13649501684060272287,
                                            ),
                                            role: GenericContainer,
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #6531607527355839990,
                                                    ),
                                                    role: GenericContainer,
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [
                                                        Node {
                                                            id: NodeId(
                                                                TreeIndex(
                                                                    0,
                                                                ),
                                                                #12741307406381510470,
                                                            ),
                                                            role: GenericContainer,
                                                            focused: false,
                                                            hidden: false,
                                                            disabled: false,
                                                            children: [
                                                                Node {
                                                                    id: NodeId(
                                                                        TreeIndex(
                                                                            0,
                                                                        ),
                                                                        #91688474024001657,
                                                                    ),
                                                                    role: SpinButton,
                                                                    label: "X",
                                                                    value: "10",
                                                                    numeric_value: 10.0,
                                                                    focused: false,
                                                                    hidden: false,
                                                                    disabled: false,
                                                                    children: [],
                                                                },
                                                            ],
                                                        },
                                                    ],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #10177753109053859069,
                                            ),
                                            role: Label,
                                            value: "Y",
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #12606259333069948493,
                                                    ),
                                                    role: TextRun,
                                                    value: "Y",
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [],
                                                },
                                            ],
                                        },
                                        Node {
                                            id: NodeId(
                                                TreeIndex(
                                                    0,
                                                ),
                                                #5158267653125753920,
                                            ),
                                            role: GenericContainer,
                                            focused: false,
                                            hidden: false,
                                            disabled: false,
                                            children: [
                                                Node {
                                                    id: NodeId(
                                                        TreeIndex(
                                                            0,
                                                        ),
                                                        #11576945359965632102,
                                                    ),
                                                    role: GenericContainer,
                                                    focused: false,
                                                    hidden: false,
                                                    disabled: false,
                                                    children: [
                                                        Node {
                                                            id: NodeId(
                                                                TreeIndex(
                                                                    0,
                                                                ),
                                                                #9982050705815329584,
                                                            ),
                                                            role: GenericContainer,
                                                            focused: false,
                                                            hidden: false,
                                                            disabled: false,
                                                            children: [
                                                                Node {
                                                                    id: NodeId(
                                                                        TreeIndex(
                                                                            0,
                                                                        ),
                                                                        #4562536752093132876,
                                                                    ),
                                                                    role: SpinButton,
                                                                    label: "Y",
                                                                    value: "10",
                                                                    numeric_value: 10.0,
                                                                    focused: false,
                                                                    hidden: false,
                                                                    disabled: false,
                                                                    children: [],
                                                                },
                                                            ],
                                                        },
                                                    ],
                                                },
                                            ],
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                },
            ],
        },
    ],
}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 stdout ----

thread 'fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40' (2597124) panicked at crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy/tests.rs:252:13:
padding blend/16

---- layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view stdout ----

thread 'layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view' (2597125) panicked at crates/pc-ui-egui/src/layer_reveal_tests.rs:111:9:
assertion `left == right` failed: ProMedium: undo made the top layer active
  left: Some(LayerId(5))
 right: Some(LayerId(37))


failures:
    fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40
    layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view
    vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 1322 filtered out; finished in 0.03s

error: test failed, to rerun pass `-p photocraft-ui-egui --lib`
```

</details>

### Unavailable direct formatter

```bash
rustfmt --edition 2024 crates/pc-ui-egui/src/vector_ui.rs
```

Exit code: `127`.

<details>
<summary>Complete captured output</summary>

```text
/bin/bash: line 9: rustfmt: command not found
```

</details>

### Initial strict Clippy: pre-existing xtask dead fields

```bash
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate cargo clippy --offline -p photocraft-engine -p photocraft-ui-egui -p xtask --all-targets -- -D warnings
```

Exit code: `101`.

<details>
<summary>Complete captured output</summary>

```text
    Checking cfg-if v1.0.5
    Checking log v0.4.34
    Checking bitflags v2.13.2
    Checking once_cell v1.21.4
    Checking memchr v2.8.3
    Checking either v1.18.0
    Checking foldhash v0.2.0
    Checking serde_core v1.0.229
    Checking libc v0.2.189
    Checking libm v0.2.16
    Checking crossbeam-utils v0.8.23
   Compiling rustix v1.1.5
    Checking linux-raw-sys v0.12.1
   Compiling syn v2.0.119
    Checking bytemuck v1.25.2
    Checking simd-adler32 v0.3.10
    Checking thiserror v2.0.21
    Checking pin-project-lite v0.2.17
   Compiling generic-array v0.14.7
    Checking adler2 v2.0.1
    Checking equivalent v1.0.2
    Checking typenum v1.20.1
    Checking crc32fast v1.5.2
    Checking itoa v1.0.18
    Checking subtle v2.6.1
    Checking miniz_oxide v0.9.1
    Checking libloading v0.8.9
    Checking tracing-core v0.1.36
    Checking hashbrown v0.17.1
    Checking crossbeam-epoch v0.9.21
   Compiling serde_json v1.0.151
    Checking zmij v1.0.23
    Checking const-oid v0.9.6
    Checking allocator-api2 v0.2.21
    Checking dlib v0.5.3
    Checking rand_core v0.10.1
    Checking miniz_oxide v0.8.9
    Checking crossbeam-deque v0.8.8
    Checking scopeguard v1.2.0
    Checking wayland-sys v0.31.11
    Checking stable_deref_trait v1.2.1
    Checking unicode-ident v1.0.26
    Checking flate2 v1.1.10
    Checking zune-core v0.5.3
    Checking getrandom v0.4.3
    Checking lock_api v0.4.14
    Checking fdeflate v0.3.7
    Checking spin v0.9.9
    Checking hashbrown v0.16.1
    Checking rayon-core v1.13.0
    Checking anyhow v1.0.104
    Checking cpufeatures v0.2.17
    Checking quick-error v2.0.1
    Checking num-traits v0.2.19
    Checking indexmap v2.14.2
    Checking zune-jpeg v0.5.15
    Checking pxfm v0.1.30
    Checking lazy_static v1.5.0
    Checking weezl v0.1.12
    Checking zerofrom v0.1.8
    Checking memmap2 v0.9.11
    Checking getrandom v0.2.17
    Checking itertools v0.15.0
    Checking byteorder-lite v0.1.0
    Checking color_quant v1.1.0
    Checking yoke v0.8.3
    Checking rayon v1.12.0
    Checking png v0.18.1
    Checking rand_core v0.6.4
    Checking nom v8.0.0
    Checking static_assertions v1.1.0
    Checking gif v0.14.2
    Checking serde v1.0.229
    Checking image-webp v0.2.4
    Checking uuid v1.26.1
    Checking qoi v0.4.1
    Checking num-integer v0.1.47
    Checking num-complex v0.4.6
    Checking base64ct v1.8.3
    Checking siphasher v1.0.4
    Checking font-types v0.12.5
    Checking linebender_resource_handle v0.1.1
    Checking unicode-width v0.2.2
    Checking smallvec v1.16.2
    Checking arrayvec v0.7.8
    Checking photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-geom)
    Checking pem-rfc7468 v0.7.0
    Checking raw-window-handle v0.6.2
    Checking zerovec v0.11.8
    Checking getrandom v0.3.4
    Checking parking_lot_core v0.9.12
    Checking read-fonts v0.41.0
    Checking polycool v0.4.0
    Checking rawpointer v0.2.1
    Checking writeable v0.6.4
    Checking rustc-hash v1.1.0
    Checking fearless_simd v0.4.1
    Checking lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-geom)
    Checking string-interner v0.20.0
    Checking matrixmultiply v0.3.11
    Checking parking_lot v0.12.5
    Checking naga-types v30.0.1
    Checking kurbo v0.13.1
    Checking strength_reduce v0.2.4
    Checking inventory v0.3.25
    Checking bit-vec v0.9.1
    Checking maplit v1.0.2
    Checking downcast-rs v2.0.2
    Checking dyn-hash v1.0.0
    Checking dyn-clone v1.0.20
    Checking ndarray v0.17.2
    Checking transpose v0.2.3
    Checking typeid v1.0.3
    Checking codespan-reporting v0.13.1
    Checking num-bigint v0.4.8
    Checking primal-check v0.3.4
    Checking bit-set v0.10.0
    Checking filetime v0.2.29
    Checking spirv v0.4.0+sdk-1.4.341.0
    Checking photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-cms)
    Checking tinystr v0.8.4
    Checking litemap v0.8.3
    Checking byteorder v1.5.0
    Checking bytes v1.12.1
    Checking rustc-hash v2.1.3
    Checking bit-vec v0.10.1
    Checking hex v0.4.3
    Checking moxcms v0.8.1
    Checking icu_locale_core v2.3.0
    Checking rustfft v6.4.1
    Checking bit-set v0.11.1
    Checking potential_utf v0.1.6
    Checking nom-language v0.1.0
    Checking erased-serde v0.4.10
    Checking ash v0.38.0+1.3.281
    Checking lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-color)
    Checking wgpu-types v30.0.1
    Checking zerotrie v0.2.5
    Checking photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-color)
    Checking num-rational v0.4.2
    Checking anymap3 v1.1.0
    Checking percent-encoding v2.3.2
    Checking alloc-no-stdlib v2.0.4
    Checking cpufeatures v0.3.1
    Checking presser v0.3.1
    Checking khronos-egl v6.0.0
    Checking photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-raster)
    Checking xattr v1.6.1
    Checking ordered-float v5.5.0
    Checking same-file v1.0.6
    Checking icu_provider v2.3.1
    Checking alloc-stdlib v0.2.4
    Checking glow v0.17.0
    Checking slab v0.4.12
    Checking tar v0.4.46
    Checking photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-doc)
    Checking renderdoc-sys v1.1.0
    Checking utf8_iter v1.0.4
    Checking brotli-decompressor v5.0.3
    Checking walkdir v2.5.0
    Checking float-cmp v0.9.0
    Checking ff v0.13.1
    Checking icu_collections v2.3.0
    Checking base16ct v0.2.0
   Compiling semver v1.0.28
    Checking group v0.13.0
    Checking strict-num v0.1.1
    Checking polling v3.11.0
    Checking chacha20 v0.10.2
    Checking lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-raster)
    Checking phf_shared v0.13.1
   Compiling rustc_version v0.4.1
    Checking base64 v0.22.1
    Checking opaque-debug v0.3.1
    Checking arrayref v0.3.9
    Checking rand v0.10.3
    Checking icu_locale_fallback_data v2.3.0
    Checking icu_properties_data v2.3.0
    Checking as-slice v0.2.1
    Checking quick-xml v0.38.4
    Checking parking v2.2.1
    Checking downcast-rs v1.2.1
    Checking untrusted v0.9.0
    Checking scoped-tls v1.0.1
    Checking aligned v0.4.3
    Checking icu_locale_fallback v2.3.0
    Checking icu_normalizer_data v2.3.0
    Checking icu_segmenter_data v2.3.0
    Checking icu_properties v2.3.0
    Checking no_std_io2 v0.9.4
    Checking wayland-backend v0.3.17
    Checking font-types v0.11.3
    Checking rgb v0.8.53
    Checking parlance v0.1.0
   Compiling rustls v0.23.45
    Checking y4m v0.8.0
    Checking roxmltree v0.20.0
    Checking rand_distr v0.6.0
   Compiling curve25519-dalek v4.1.3
    Checking float-ord v0.3.2
    Checking web-time v1.1.0
   Compiling num-bigint-dig v0.8.6
    Checking futures-core v0.3.34
    Checking read-fonts v0.39.2
    Checking icu_segmenter v2.3.0
    Checking bitstream-io v4.10.0
    Checking icu_normalizer v2.3.0
    Checking httparse v1.10.1
    Checking http v1.5.0
    Checking maybe-rayon v0.1.1
    Checking lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-tiff)
    Checking num-iter v0.1.46
    Checking itertools v0.14.0
    Checking imgref v1.12.3
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-text)
    Checking winnow v1.0.4
    Checking new_debug_unreachable v1.0.6
    Checking wayland-client v0.31.15
    Checking tiny-skia-path v0.11.4
    Checking parley_data v0.11.1
    Checking lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-develop)
    Checking loop9 v0.1.5
    Checking kurbo v0.11.3
    Checking avif-serialize v0.8.9
    Checking photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-psd)
    Checking aho-corasick v1.1.5
    Checking raw-cpuid v11.6.0
    Checking lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-meta)
    Checking fastrand v2.5.0
    Checking pulp-wasm-simd-flag v0.1.1
    Checking jpeg-encoder v0.7.1
    Checking base64 v0.23.1
    Checking regex-syntax v0.8.11
    Checking pollster v1.0.1
    Checking reborrow v0.5.5
    Checking svgtypes v0.15.3
    Checking linesweeper v0.4.0
    Checking moxcms v0.9.1
    Checking ureq-proto v0.6.4
    Checking jpeg-decoder v0.3.2
    Checking rand_core v0.9.5
    Checking proc-macro2 v1.0.107
    Checking zune-inflate v0.2.54
    Checking simplecss v0.2.2
    Checking skrifa v0.44.0
    Checking harfrust v0.12.0
    Checking fontique v0.11.1
    Checking twox-hash v2.1.4
    Checking futures-io v0.3.34
    Checking pico-args v0.5.0
    Checking pulp v0.22.3
    Checking bumpalo v3.20.3
    Checking xmlwriter v0.1.0
    Checking data-url v0.3.2
    Checking ttf-parser v0.25.1
    Checking lebe v0.5.3
    Checking utf8-zero v0.8.1
    Checking imagesize v0.13.0
    Checking bit_field v0.10.3
    Checking hayro-ccitt v0.3.0
    Checking ruzstd v0.9.0
    Checking quote v1.0.47
    Checking lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-raw)
    Checking hayro-jbig2 v0.3.0
    Checking usvg v0.45.1
    Checking regex-automata v0.4.18
    Checking lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-pipeline)
    Checking event-listener v5.4.2
    Checking lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-scenes)
    Checking brotli v8.0.4
    Checking photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-vector)
    Checking hayro-jpeg2000 v0.3.5
    Checking wasmi_core v2.0.0
    Checking concurrent-queue v2.5.0
    Checking ryu v1.0.23
    Checking constant_time_eq v0.4.2
    Checking jpeg-encoder v0.6.1
    Checking hayro-postscript v0.1.0
    Checking ab_glyph_rasterizer v0.1.10
    Checking option-ext v0.2.0
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-engine)
    Checking owned_ttf_parser v0.25.1
    Checking blake3 v1.8.7
    Checking hayro-syntax v0.7.2
    Checking pdf-writer v0.15.0
    Checking dirs-sys v0.5.0
    Checking ab_glyph v0.2.32
    Checking wasmi_ir v2.0.0
    Checking photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-compose)
    Checking photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-pathops)
    Checking parley v0.11.1
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking event-listener-strategy v0.5.4
    Checking syn v3.0.6
    Checking regex v1.13.1
    Checking lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-catalog)
    Checking wasmparser v0.228.0
    Checking lensfun v0.7.0
    Checking futures-lite v2.6.1
    Checking skrifa v0.42.1
    Checking write-fonts v0.48.1
    Checking lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-preview)
    Checking tiny-skia-path v0.12.0
    Checking photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-algo)
    Checking chunked_transfer v1.5.0
    Checking imagesize v0.14.0
    Checking hayro-cmap v0.1.0
    Checking xmp-writer v0.3.3
    Checking utf-8 v0.7.6
    Checking wasmi_collections v2.0.0
    Checking ascii v1.1.0
    Checking httpdate v1.0.3
    Checking data-encoding v2.11.1
    Checking dirs v6.0.0
    Checking wayland-protocols v0.32.13
    Checking gpu-allocator v0.28.0
    Checking photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-paint)
    Checking photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-raw)
    Checking zcheapstr v1.1.0
    Checking endi v1.1.1
    Checking tiny_http v0.12.0
    Checking photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-vecfx)
    Checking photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ops)
    Checking pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-trace)
    Checking accesskit v0.24.1
    Checking errno v0.3.14
    Checking async-task v4.7.1
    Checking async-io v2.6.0
    Checking signal-hook-registry v1.4.8
    Checking async-channel v2.5.0
   Compiling ahash v0.8.12
    Checking atomic-waker v1.1.2
    Checking async-lock v3.4.2
    Checking euclid v0.22.14
    Checking piper v0.2.5
    Checking async-signal v0.2.14
    Checking color v0.3.3
    Checking async-executor v1.14.0
    Checking zvariant_utils v4.2.0
    Checking blocking v1.7.0
    Checking async-broadcast v0.7.2
    Checking ordered-stream v0.2.0
    Checking async-process v2.5.0
    Checking cursor-icon v1.2.0
    Checking xcursor v0.3.11
    Checking xkeysym v0.2.1
    Checking linux-raw-sys v0.4.15
    Checking guillotiere v0.7.0
    Checking wayland-csd-frame v0.3.0
    Checking peniko v0.6.1
    Checking emath v0.36.2
    Checking wayland-cursor v0.31.14
    Checking unicode-general-category v1.1.0
    Checking gethostname v1.1.0
    Checking as-raw-xcb-connection v1.0.1
    Checking unicode-segmentation v1.13.3
    Checking vello_common v0.1.0
    Checking rustix v0.38.44
    Checking nohash-hasher v0.2.0
    Checking wasmi v2.0.0
    Checking epaint_default_fonts v0.36.2
    Checking x11rb-protocol v0.13.2
    Checking ecolor v0.36.2
    Checking self_cell v1.3.0
    Checking x11-dl v2.21.0
    Checking xkbcommon-dl v0.4.2
    Checking accesskit_consumer v0.36.0
    Checking futures-task v0.3.34
    Checking smol_str v0.2.2
    Checking dpi v0.1.2
    Checking bitflags v1.3.2
    Checking toml_parser v1.1.3+spec-1.1.0
    Checking type-map v0.5.1
    Checking serde_spanned v1.1.1
    Checking futures-util v0.3.34
    Checking png v0.17.16
    Checking toml_datetime v1.1.1+spec-1.1.0
    Checking toml_writer v1.1.2+spec-1.1.0
   Compiling li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-attributions)
    Checking is-docker v0.2.0
    Checking vello_cpu v0.1.0
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ui-egui)
    Checking accesskit_consumer v0.35.0
    Checking toml v1.1.6+spec-1.1.0
    Checking is-wsl v0.4.0
    Checking getopts v0.2.24
    Checking tiny-skia v0.11.4
    Checking colored v2.2.0
    Checking pollster v0.4.0
    Checking kittest v0.4.0
    Checking open v5.4.4
    Checking tempfile v3.27.0
    Checking memory-stats v1.2.0
    Checking sys-locale v0.3.2
    Checking leb128fmt v0.1.0
    Checking typed-path v0.12.3
    Checking subsetter v0.2.6
    Checking wasm-encoder v0.261.0
    Checking krilla v0.8.2
    Checking resvg v0.45.1
    Checking wayland-protocols-wlr v0.3.12
    Checking wayland-protocols-plasma v0.3.12
    Checking wayland-protocols-experimental v20250721.0.1
    Checking wayland-protocols-misc v0.3.12
    Checking rfd v0.17.2
    Checking wast v261.0.0
    Checking zip v7.2.0
   Compiling zerocopy-derive v0.8.59
   Compiling zeroize_derive v1.5.0
   Compiling tracing-attributes v0.1.31
   Compiling fax_derive v0.2.0
   Compiling profiling-procmacros v1.0.18
   Compiling derive-new v0.7.0
   Compiling equator-macro v0.4.2
   Compiling phf_macros v0.13.1
   Compiling enumflags2_derive v0.7.12
   Compiling curve25519-dalek-derive v0.1.1
   Compiling prost-derive v0.14.4
   Compiling arg_enum_proc_macro v0.3.4
   Compiling num-derive v0.4.2
    Checking photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-plugins)
   Compiling async-recursion v1.1.1
    Checking profiling v1.0.18
   Compiling thiserror-impl v1.0.69
   Compiling enum-map-derive v0.17.0
    Checking fax v0.2.6
   Compiling enumflags2 v0.7.12
   Compiling zvariant v5.15.0
    Checking wat v1.261.0
    Checking equator v0.4.2
    Checking x11rb v0.13.2
    Checking aligned-vec v0.6.4
    Checking v_frame v0.3.9
    Checking av-scenechange v0.14.1
    Checking phf v0.13.1
    Checking av1-grain v0.2.5
    Checking hayro-interpret v0.7.0
    Checking zeroize v1.9.1
    Checking zbus_names v4.3.4
    Checking der v0.7.10
    Checking rustls-pki-types v1.15.1
    Checking rav1e v0.8.1
    Checking crypto-common v0.1.7
    Checking block-buffer v0.10.4
    Checking inout v0.1.4
    Checking universal-hash v0.5.1
    Checking digest v0.10.7
    Checking cipher v0.4.4
    Checking crypto-bigint v0.5.5
    Checking sha2 v0.10.9
    Checking hmac v0.12.1
    Checking spki v0.7.3
    Checking signature v2.2.0
    Checking hkdf v0.12.4
    Checking pkcs5 v0.7.1
    Checking rfc6979 v0.4.0
    Checking polyval v0.6.2
    Checking aead v0.5.2
    Checking pkcs8 v0.10.2
    Checking rustls-webpki v0.103.15
    Checking ghash v0.5.1
    Checking aes v0.8.4
    Checking ctr v0.9.2
    Checking sec1 v0.7.3
    Checking pkcs1 v0.7.5
    Checking tracing v0.1.44
    Checking ed25519 v2.2.3
    Checking elliptic-curve v0.13.8
    Checking chacha20 v0.9.1
    Checking poly1305 v0.8.0
    Checking jxl-bitstream v1.1.0
    Checking jxl-grid v0.6.2
    Checking jxl-threadpool v1.0.0
    Checking ed25519-dalek v2.2.0
    Checking x25519-dalek v2.0.1
    Checking aes-gcm v0.10.3
    Checking rustls-webpki v0.102.8
    Checking prost v0.14.4
    Checking primeorder v0.13.6
    Checking ecdsa v0.16.9
    Checking chacha20poly1305 v0.10.1
    Checking jxl-oxide-common v1.0.0
    Checking jxl-coding v1.0.1
    Checking webpki-roots v1.0.9
    Checking sha1 v0.10.7
   Compiling zbus_xml v5.2.1
    Checking jxl-image v0.13.0
    Checking calloop v0.14.4
    Checking enum-map v2.7.3
    Checking p256 v0.13.2
    Checking p384 v0.13.1
    Checking jxl-modular v0.11.3
    Checking jxl-color v0.11.0
   Compiling zbus_macros v5.19.0
    Checking zbus-lockstep v0.5.2
    Checking calloop-wayland-source v0.4.1
    Checking smithay-client-toolkit v0.20.0
    Checking jxl-vardct v0.11.1
    Checking thiserror v1.0.69
    Checking ravif v0.13.0
    Checking calloop v0.13.0
    Checking jxl-frame v0.13.3
    Checking smithay-clipboard v0.7.3
    Checking zerocopy v0.8.59
    Checking calloop-wayland-source v0.3.0
    Checking smithay-client-toolkit v0.19.2
    Checking jxl-jbr v0.2.1
    Checking jxl-render v0.12.4
    Checking ureq v3.4.2
   Compiling zbus-lockstep-macros v0.5.2
    Checking jxl-oxide v0.12.6
    Checking half v2.7.1
    Checking ppv-lite86 v0.2.21
    Checking epaint v0.36.2
    Checking winit v0.30.13
    Checking rand_chacha v0.3.1
    Checking rand_chacha v0.9.0
    Checking tiff v0.11.3
    Checking tract-data v0.23.8
    Checking naga v30.0.1
    Checking exr v1.74.2
    Checking zbus v5.19.0
    Checking rand v0.8.8
    Checking rand v0.9.5
    Checking tungstenite v0.27.0
   Compiling tract-linalg v0.23.8
    Checking egui v0.36.2
    Checking image v0.25.10
    Checking rsa v0.9.10
    Checking rustls-rustcrypto v0.0.2-alpha
    Checking xtask v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/xtask)
    Checking lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-codecs)
    Checking photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-codecs)
    Checking photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-affinity)
    Checking photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-svg)
    Checking photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-pdf)
    Checking arboard v3.6.1
    Checking dify v0.8.0
    Checking photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-testkit)
error: fields `tool`, `label`, and `recognition` are never read
  --> xtask/src/tool_art_vector.rs:6:9
   |
 5 | pub struct Art {
   |            --- fields in this struct
 6 |     pub tool: &'static str,
   |         ^^^^
 7 |     pub icon: &'static str,
 8 |     pub label: &'static str,
   |         ^^^^^
...
12 |     pub recognition: &'static str,
   |         ^^^^^^^^^^^
   |
   = note: `Art` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis
   = note: `-D dead-code` implied by `-D warnings`
   = help: to override `-D warnings` add `#[expect(dead_code)]` or `#[allow(dead_code)]`

error: fields `tool` and `label` are never read
 --> xtask/src/tool_art_vector.rs:6:9
  |
5 | pub struct Art {
  |            --- fields in this struct
6 |     pub tool: &'static str,
  |         ^^^^
7 |     pub icon: &'static str,
8 |     pub label: &'static str,
  |         ^^^^^
  |
  = note: `Art` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis
  = note: `-D dead-code` implied by `-D warnings`
  = help: to override `-D warnings` add `#[expect(dead_code)]` or `#[allow(dead_code)]`

error: could not compile `xtask` (bin "xtask") due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `xtask` (bin "xtask" test) due to 1 previous error
```

</details>

### Final strict Clippy

```bash
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate cargo clippy --offline -p photocraft-engine -p photocraft-ui-egui -p xtask --all-targets -- -D warnings
```

Exit code: `0`.

<details>
<summary>Complete captured output</summary>

```text
    Checking tract-linalg v0.23.8
    Checking wgpu-naga-bridge v30.0.1
    Checking lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-merge)
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-format)
    Checking atspi-common v0.13.0
    Checking li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-attributions)
    Checking egui_extras v0.36.2
    Checking xtask v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/xtask)
    Checking wgpu-hal v30.0.1
    Checking tract-core v0.23.8
    Checking wgpu-core-deps-windows-linux-android v30.0.1
    Checking atspi-proxies v0.13.0
    Checking accesskit_atspi_common v0.18.1
    Checking wgpu-core v30.0.1
    Checking atspi v0.29.0
    Checking accesskit_unix v0.21.1
    Checking accesskit_winit v0.32.2
    Checking egui-winit v0.36.2
    Checking wgpu v30.0.1
    Checking photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-gpu)
    Checking egui-wgpu v0.36.2
    Checking eframe v0.36.2
    Checking egui_kittest v0.36.2
    Checking tract-nnef v0.23.8
    Checking tract-hir v0.23.8
    Checking tract-extra v0.23.8
    Checking tract-transformers v0.23.8
    Checking tract-onnx-opl v0.23.8
    Checking tract-onnx v0.23.8
    Checking li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-seg)
    Checking lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-gpu)
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-ai)
    Checking lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-engine)
    Checking photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-io)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-engine)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ui-egui)
    Finished `dev` profile [optimized + debuginfo] target(s) in 41.94s
```

</details>

### Final engine/UI affected-module suites

```bash
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate cargo test --offline --no-fail-fast -p photocraft-engine -p photocraft-ui-egui --lib -- layer_multi_cmds:: fill_stroke_proxy:: layer_reveal:: vector_ui:: tests::undo_and_redo_restore_the_targeted_layers
```

Exit code: `0`.

<details>
<summary>Complete captured output</summary>

```text
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-engine)
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 40.77s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-0962a62fc0794aad)

running 24 tests
test layer_multi_cmds::tests::a_locked_group_keeps_a_grouped_background_locked ... ok
test layer_multi_cmds::tests::find_layers_by_name ... ok
test layer_multi_cmds::tests::a_locked_group_locks_its_contents ... ok
test layer_multi_cmds::tests::align_single_layer_to_canvas ... ok
test layer_multi_cmds::tests::move_copy_linked_layers_is_atomic_and_has_one_journal_and_undo_step ... ok
test layer_multi_cmds::tests::duplicate_hide_lock_and_reverse_selected ... ok
test layer_multi_cmds::tests::inspect_reports_selection ... ok
test layer_multi_cmds::tests::group_from_layers_preserves_order ... ok
test layer_multi_cmds::tests::moves_and_layer_props_report_only_their_area ... ok
test layer_multi_cmds::tests::group_preview_preserves_shape_cache_translation_identity ... ok
test layer_multi_cmds::tests::selection_pruned_after_delete_and_undo ... ok
test layer_multi_cmds::tests::the_move_preview_matches_the_move ... ok
test layer_multi_cmds::tests::select_linked_layers_requires_a_linked_selection ... ok
test layer_multi_cmds::tests::toggle_and_range_selection ... ok
test layer_multi_cmds::tests::grouping_is_capped_at_the_document_nesting_limit ... ok
test layer_multi_cmds::tests::translate_rejects_offsets_that_would_wrap ... ok
test tests::undo_and_redo_restore_the_targeted_layers ... ok
test layer_multi_cmds::tests::align_single_layer_to_selection ... ok
test layer_multi_cmds::tests::select_linked_layers_preserves_a_linked_active_layer ... ok
test layer_multi_cmds::tests::merge_selected_into_top_most ... ok
test layer_multi_cmds::tests::select_linked_layers_excludes_an_unlinked_active_layer ... ok
test layer_multi_cmds::tests::linked_layers_move_together_and_undo ... ok
test layer_multi_cmds::tests::distribute_uneven_layers ... ok
test layer_multi_cmds::tests::align_each_edge ... ok

test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 1295 filtered out; finished in 0.06s

     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc)

running 29 tests
test fill_stroke_proxy::tests::toolbar46_gpu_full_app_proxy_keys_icon_upload_and_pixel_parity ... ignored, 00 production toolbar/menu wiring and real RTX 5090 GPU milestone; owner speed rules
test fill_stroke_proxy::tests::toolbar46_gpu_upload_every_vector_icon_and_switch_styles ... ignored, RTX 5090 icon upload milestone; owner speed rules prohibit GPU runs
test fill_stroke_proxy::tests::review_additional_proxy_edits_only_selected_gradient_stop ... ok
test layer_reveal::tests::reveal_survives_odd_documents ... ok
test vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing ... ok
test vector_ui::tests::native_shape_context_allows_corner_editing_in_both_personas ... ok
test vector_ui::tests::paths_context_menu_uses_the_clicked_row_and_dispatches_commands ... ok
test vector_ui::tests::pen_builds_and_closes_a_work_path ... ok
test layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view ... ok
test vector_ui::tests::shape_drag_creates_layers_with_modifiers ... ok
test vector_ui::tests::shape_preview_is_the_committed_path ... ok
test vector_ui::tests::shape_colour_picker_stays_put_while_dragging ... ok
test vector_ui::tests::work_path_context_save_and_layer_path_deletion_rules ... ok
test vector_ui::zero_axis_options_regression::zero_height_curve_options_do_not_publish_idle_edits ... ok
test vector_ui::tests::shift_pressed_during_a_shape_drag_constrains_it ... ok
test vector_ui::path_selection_tests::cmd_enter_with_the_pen_loads_the_work_path ... ok
test vector_ui::path_selection_tests::cmd_enter_loads_the_path_selected_in_the_paths_panel ... ok
test vector_ui::path_selection_tests::without_a_path_it_says_why_and_the_key_can_be_rebound ... ok
test vector_ui::path_selection_tests::cmd_enter_while_drawing_finishes_the_path_and_loads_it ... ok
test fill_stroke_proxy::tests::toolbar46_contact_sheet_original_artwork ... ok
test fill_stroke_proxy::tests::toolbar46_pointer_hold_detached_preview_and_single_release ... ok
test fill_stroke_proxy::tests::toolbar46_frozen_pixel_menu_keys_and_all_chords_parse ... ok
test fill_stroke_proxy::tests::toolbar46_hold_flyout_curvature_and_preset_keyboard_switch ... ok
test fill_stroke_proxy::tests::toolbar46_narrow_toolbar_controls_popups_and_captured_release ... ok
test fill_stroke_proxy::tests::toolbar46_detached_proxy_rejects_stale_selection_locked_or_nonfinite_set ... ok
test fill_stroke_proxy::tests::toolbar46_proxy_clicks_keys_one_undo_and_defaults ... ok
test fill_stroke_proxy::tests::toolbar46_swap_preserves_hidden_paints_and_stroke_metadata ... ok
test fill_stroke_proxy::tests::toolbar46_multiple_targets_swap_individually_and_locked_tail_is_atomic ... ok
test fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 ... ok

test result: ok. 27 passed; 0 failed; 2 ignored; 0 measured; 1297 filtered out; finished in 0.33s

```

</details>

### Final generator-module suites

```bash
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate cargo test --offline -p xtask -- tool_art:: tool_art_vector::
```

Exit code: `0`.

<details>
<summary>Complete captured output</summary>

```text
   Compiling syn v2.0.119
   Compiling zerocopy v0.8.59
   Compiling num-traits v0.2.19
   Compiling rustix v1.1.5
   Compiling spin v0.9.9
   Compiling log v0.4.34
   Compiling httparse v1.10.1
   Compiling syn v3.0.6
   Compiling once_cell v1.21.4
   Compiling serde v1.0.229
   Compiling linux-raw-sys v0.12.1
   Compiling smallvec v1.16.2
   Compiling flate2 v1.1.10
   Compiling http v1.5.0
   Compiling winnow v1.0.4
   Compiling serde_json v1.0.151
   Compiling lazy_static v1.5.0
   Compiling toml_datetime v1.1.1+spec-1.1.0
   Compiling serde_spanned v1.1.1
   Compiling typed-path v0.12.3
   Compiling toml_parser v1.1.3+spec-1.1.0
   Compiling ureq-proto v0.6.4
   Compiling num-integer v0.1.47
   Compiling toml v1.1.6+spec-1.1.0
   Compiling num-iter v0.1.46
   Compiling zip v7.2.0
   Compiling xattr v1.6.1
   Compiling tar v0.4.46
   Compiling serde_derive v1.0.229
   Compiling ppv-lite86 v0.2.21
   Compiling rand_chacha v0.3.1
   Compiling rand v0.8.8
   Compiling zeroize_derive v1.5.0
   Compiling curve25519-dalek-derive v0.1.1
   Compiling zeroize v1.9.1
   Compiling generic-array v0.14.7
   Compiling der v0.7.10
   Compiling rustls-pki-types v1.15.1
   Compiling num-bigint-dig v0.8.6
   Compiling rustls-webpki v0.103.15
   Compiling rustls-webpki v0.102.8
   Compiling webpki-roots v1.0.9
   Compiling crypto-common v0.1.7
   Compiling block-buffer v0.10.4
   Compiling inout v0.1.4
   Compiling crypto-bigint v0.5.5
   Compiling cipher v0.4.4
   Compiling universal-hash v0.5.1
   Compiling aead v0.5.2
   Compiling digest v0.10.7
   Compiling polyval v0.6.2
   Compiling poly1305 v0.8.0
   Compiling ctr v0.9.2
   Compiling aes v0.8.4
   Compiling chacha20 v0.9.1
   Compiling hmac v0.12.1
   Compiling signature v2.2.0
   Compiling sha2 v0.10.9
   Compiling curve25519-dalek v4.1.3
   Compiling rustls v0.23.45
   Compiling spki v0.7.3
   Compiling hkdf v0.12.4
   Compiling rfc6979 v0.4.0
   Compiling ghash v0.5.1
   Compiling chacha20poly1305 v0.10.1
   Compiling pkcs5 v0.7.1
   Compiling aes-gcm v0.10.3
   Compiling pkcs8 v0.10.2
   Compiling sec1 v0.7.3
   Compiling ed25519 v2.2.3
   Compiling pkcs1 v0.7.5
   Compiling x25519-dalek v2.0.1
   Compiling elliptic-curve v0.13.8
   Compiling ed25519-dalek v2.2.0
   Compiling rsa v0.9.10
   Compiling ecdsa v0.16.9
   Compiling primeorder v0.13.6
   Compiling p384 v0.13.1
   Compiling p256 v0.13.2
   Compiling rustls-rustcrypto v0.0.2-alpha
   Compiling ureq v3.4.2
   Compiling xtask v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/xtask)
    Finished `test` profile [optimized + debuginfo] target(s) in 11.68s
     Running unittests src/main.rs (target/debug/deps/xtask-9e3632f8d6c6092f)

running 3 tests
test tool_art_vector::tests::toolbar46_artwork_freshness_and_recognition_inventory ... ok
test tool_art::tests::mixed_tool_artwork_is_pinned_fresh_and_preserves_the_approved_base ... ok
test tool_art::tests::ds2_unknown_icon_set_returns_error_and_generation_creates_destinations ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.04s

```

</details>

### Final workspace check

```bash
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate cargo check --offline --workspace
```

Exit code: `0`.

<details>
<summary>Complete captured output</summary>

```text
    Checking cfg-if v1.0.5
    Checking log v0.4.34
    Checking once_cell v1.21.4
    Checking memchr v2.8.3
    Checking either v1.18.0
    Checking foldhash v0.2.0
    Checking serde_core v1.0.229
    Checking libc v0.2.189
    Checking libm v0.2.16
    Checking crossbeam-utils v0.8.23
   Compiling rustix v1.1.5
    Checking bytemuck v1.25.2
    Checking zerocopy v0.8.59
    Checking zeroize v1.9.1
    Checking linux-raw-sys v0.12.1
    Checking thiserror v2.0.21
    Checking pin-project-lite v0.2.17
    Checking simd-adler32 v0.3.10
    Checking equivalent v1.0.2
    Checking stable_deref_trait v1.2.1
    Checking typenum v1.20.1
    Checking adler2 v2.0.1
    Checking itoa v1.0.18
    Checking hashbrown v0.17.1
   Compiling serde_json v1.0.151
    Checking subtle v2.6.1
    Checking crc32fast v1.5.2
    Checking zmij v1.0.23
    Checking libloading v0.8.9
    Checking crossbeam-epoch v0.9.21
    Checking miniz_oxide v0.9.1
    Checking tracing-core v0.1.36
    Checking const-oid v0.9.6
    Checking allocator-api2 v0.2.21
    Checking unicode-ident v1.0.26
    Checking dlib v0.5.3
    Checking scopeguard v1.2.0
    Checking miniz_oxide v0.8.9
    Checking rand_core v0.10.1
    Checking crossbeam-deque v0.8.8
    Checking wayland-sys v0.31.11
    Checking lock_api v0.4.14
    Checking nom v8.0.0
    Checking tracing v0.1.44
    Checking cpufeatures v0.2.17
    Checking zune-core v0.5.3
    Checking fdeflate v0.3.7
    Checking getrandom v0.3.4
    Checking generic-array v0.14.7
    Checking flate2 v1.1.10
    Checking getrandom v0.4.3
    Checking rayon-core v1.13.0
    Checking num-traits v0.2.19
    Checking memmap2 v0.9.11
    Checking spin v0.9.9
    Checking slab v0.4.12
    Checking zune-jpeg v0.5.15
    Checking rand_core v0.9.5
    Checking zerofrom v0.1.8
    Checking anyhow v1.0.104
    Checking quick-error v2.0.1
    Checking futures-core v0.3.34
    Checking lazy_static v1.5.0
    Checking typeid v1.0.3
    Checking getrandom v0.2.17
    Checking yoke v0.8.3
    Checking itertools v0.15.0
    Checking weezl v0.1.12
    Checking pxfm v0.1.30
    Checking rayon v1.12.0
    Checking profiling v1.0.18
    Checking crypto-common v0.1.7
    Checking block-buffer v0.10.4
    Checking color_quant v1.1.0
    Checking static_assertions v1.1.0
    Checking rand_core v0.6.4
    Checking byteorder-lite v0.1.0
    Checking fax v0.2.6
    Checking base64ct v1.8.3
    Checking digest v0.10.7
    Checking siphasher v1.0.4
    Checking num-integer v0.1.47
    Checking num-complex v0.4.6
    Checking gif v0.14.2
    Checking serde v1.0.229
    Checking bitflags v2.13.2
    Checking hashbrown v0.16.1
    Checking indexmap v2.14.2
    Checking image-webp v0.2.4
    Checking pem-rfc7468 v0.7.0
    Checking qoi v0.4.1
    Checking futures-io v0.3.34
    Checking png v0.18.1
    Checking der v0.7.10
    Checking uuid v1.26.1
    Checking linebender_resource_handle v0.1.1
    Checking scoped-tls v1.0.1
    Checking smallvec v1.16.2
    Checking arrayvec v0.7.8
    Checking photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-geom)
    Checking font-types v0.12.5
    Checking parking_lot_core v0.9.12
    Checking rawpointer v0.2.1
    Checking byteorder v1.5.0
    Checking polycool v0.4.0
    Checking raw-window-handle v0.6.2
    Checking dyn-clone v1.0.20
    Checking parking_lot v0.12.5
    Checking downcast-rs v1.2.1
    Checking matrixmultiply v0.3.11
    Checking kurbo v0.13.1
    Checking bytes v1.12.1
    Checking zerovec v0.11.8
    Checking string-interner v0.20.0
    Checking lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-geom)
    Checking jxl-bitstream v1.1.0
    Checking dyn-hash v1.0.0
    Checking rustc-hash v1.1.0
    Checking read-fonts v0.41.0
    Checking parking v2.2.1
    Checking ndarray v0.17.2
    Checking downcast-rs v2.0.2
    Checking spki v0.7.3
    Checking strength_reduce v0.2.4
    Checking photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-cms)
    Checking maplit v1.0.2
    Checking inventory v0.3.25
    Checking naga-types v30.0.1
    Checking raw-cpuid v11.6.0
    Checking nom-language v0.1.0
    Checking transpose v0.2.3
    Checking num-bigint v0.4.8
    Checking pkcs5 v0.7.1
    Checking primal-check v0.3.4
    Checking sha2 v0.10.9
    Checking unicode-width v0.2.2
    Checking writeable v0.6.4
    Checking bit-vec v0.9.1
    Checking pkcs8 v0.10.2
    Checking bit-vec v0.10.1
    Checking reborrow v0.5.5
    Checking ppv-lite86 v0.2.21
    Checking pulp-wasm-simd-flag v0.1.1
    Checking fearless_simd v0.4.1
    Checking codespan-reporting v0.13.1
    Checking bit-set v0.10.0
    Checking rustfft v6.4.1
    Checking moxcms v0.8.1
    Checking erased-serde v0.4.10
    Checking jxl-oxide-common v1.0.0
    Checking bit-set v0.11.1
    Checking photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-color)
    Checking lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-color)
    Checking spirv v0.4.0+sdk-1.4.341.0
    Checking rand_chacha v0.9.0
    Checking jxl-grid v0.6.2
    Checking filetime v0.2.29
    Checking anymap3 v1.1.0
    Checking alloc-no-stdlib v2.0.4
    Checking rand v0.9.5
    Checking photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-raster)
    Checking cpufeatures v0.3.1
    Checking ash v0.38.0+1.3.281
    Checking alloc-stdlib v0.2.4
    Checking tinystr v0.8.4
    Checking wgpu-types v30.0.1
    Checking jxl-coding v1.0.1
    Checking num-rational v0.4.2
    Checking jxl-threadpool v1.0.0
    Checking photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-doc)
    Checking same-file v1.0.6
    Checking winnow v1.0.4
    Checking litemap v0.8.3
    Checking presser v0.3.1
    Checking rand_distr v0.5.1
    Checking walkdir v2.5.0
    Checking pulp v0.22.3
    Checking equator v0.4.2
    Checking icu_locale_core v2.3.0
    Checking brotli-decompressor v5.0.3
    Checking khronos-egl v6.0.0
    Checking potential_utf v0.1.6
    Checking half v2.7.1
    Checking zerotrie v0.2.5
    Checking hmac v0.12.1
    Checking ordered-float v5.5.0
    Checking proc-macro2 v1.0.107
    Checking tiff v0.11.3
    Checking naga v30.0.1
    Checking polling v3.11.0
    Checking wayland-backend v0.3.17
    Checking xattr v1.6.1
    Checking tract-data v0.23.8
    Checking renderdoc-sys v1.1.0
    Checking rustc-hash v2.1.3
    Checking fastrand v2.5.0
    Checking tar v0.4.46
    Checking glow v0.17.0
    Checking quote v1.0.47
    Checking icu_provider v2.3.1
    Checking aligned-vec v0.6.4
    Checking jxl-modular v0.11.3
    Checking chacha20 v0.10.2
    Checking lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-raster)
    Checking wayland-client v0.31.15
    Checking jxl-image v0.13.0
    Checking event-listener v5.4.2
    Checking signature v2.2.0
    Checking image v0.25.10
    Checking ff v0.13.1
    Checking float-cmp v0.9.0
    Checking inout v0.1.4
    Checking concurrent-queue v2.5.0
    Checking utf8_iter v1.0.4
    Checking base16ct v0.2.0
    Checking hex v0.4.3
    Checking cipher v0.4.4
    Checking icu_collections v2.3.0
    Checking strict-num v0.1.1
    Checking sec1 v0.7.3
    Checking jxl-vardct v0.11.1
    Checking group v0.13.0
    Checking event-listener-strategy v0.5.4
    Checking rand v0.10.3
    Checking tract-linalg v0.23.8
    Checking v_frame v0.3.9
    Checking syn v3.0.6
    Checking futures-lite v2.6.1
    Checking hkdf v0.12.4
    Checking crypto-bigint v0.5.5
    Checking universal-hash v0.5.1
    Checking as-slice v0.2.1
    Checking rustls-pki-types v1.15.1
    Checking arrayref v0.3.9
    Checking percent-encoding v2.3.2
    Checking opaque-debug v0.3.1
    Checking aligned v0.4.3
    Checking enumflags2 v0.7.12
    Checking jxl-frame v0.13.3
    Checking zcheapstr v1.1.0
    Checking wayland-protocols v0.32.13
    Checking rand_distr v0.6.0
    Checking no_std_io2 v0.9.4
    Checking quick-xml v0.38.4
    Checking rgb v0.8.53
    Checking futures-sink v0.3.34
    Checking base64 v0.22.1
    Checking float-ord v0.3.2
    Checking y4m v0.8.0
    Checking endi v1.1.1
    Checking roxmltree v0.20.0
    Checking untrusted v0.9.0
    Checking bitstream-io v4.10.0
    Checking av-scenechange v0.14.1
    Checking prost v0.14.4
    Checking elliptic-curve v0.13.8
    Checking icu_locale_fallback_data v2.3.0
    Checking icu_properties_data v2.3.0
    Checking av1-grain v0.2.5
    Checking jxl-color v0.11.0
    Checking rfc6979 v0.4.0
    Checking rand_chacha v0.3.1
    Checking maybe-rayon v0.1.1
    Checking lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-tiff)
    Checking phf_shared v0.13.1
    Checking errno v0.3.14
    Checking itertools v0.14.0
   Compiling enumn v0.1.14
   Compiling ahash v0.8.12
    Checking new_debug_unreachable v1.0.6
    Checking async-task v4.7.1
    Checking web-time v1.1.0
    Checking imgref v1.12.3
    Checking phf v0.13.1
    Checking signal-hook-registry v1.4.8
    Checking rand v0.8.8
    Checking curve25519-dalek v4.1.3
    Checking harfrust v0.12.0
    Checking skrifa v0.44.0
    Checking photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-vector)
    Checking jxl-render v0.12.4
    Checking loop9 v0.1.5
    Checking ecdsa v0.16.9
    Checking lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-meta)
    Checking primeorder v0.13.6
    Checking tract-core v0.23.8
    Checking icu_properties v2.3.0
    Checking icu_locale_fallback v2.3.0
    Checking jxl-jbr v0.2.1
    Checking rustls-webpki v0.103.15
    Checking async-io v2.6.0
    Checking rav1e v0.8.1
    Checking icu_normalizer_data v2.3.0
    Checking icu_segmenter_data v2.3.0
    Checking zvariant_utils v4.2.0
    Checking futures-channel v0.3.34
    Checking polyval v0.6.2
    Checking async-channel v2.5.0
    Checking lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-develop)
    Checking avif-serialize v0.8.9
    Checking num-iter v0.1.46
    Checking euclid v0.22.14
    Checking aead v0.5.2
    Checking aho-corasick v1.1.5
    Checking font-types v0.11.3
    Checking color v0.3.3
    Checking parlance v0.1.0
    Checking futures-task v0.3.34
    Checking zvariant v5.15.0
    Checking atomic-waker v1.1.2
    Checking regex-syntax v0.8.11
    Checking jpeg-encoder v0.7.1
    Checking guillotiere v0.7.0
    Checking piper v0.2.5
    Checking peniko v0.6.1
    Checking fontique v0.11.1
    Checking read-fonts v0.39.2
    Checking futures-util v0.3.34
    Checking parley_data v0.11.1
    Checking num-bigint-dig v0.8.6
    Checking ghash v0.5.1
    Checking jxl-oxide v0.12.6
    Checking icu_segmenter v2.3.0
    Checking rustls v0.23.45
    Checking icu_normalizer v2.3.0
    Checking async-signal v0.2.14
    Checking poly1305 v0.8.0
    Checking tiny-skia-path v0.11.4
    Checking async-lock v3.4.2
    Checking chacha20 v0.9.1
    Checking aes v0.8.4
    Checking wayland-protocols-wlr v0.3.12
    Checking regex-automata v0.4.18
    Checking zbus_names v4.3.4
    Checking accesskit v0.24.1
    Checking ctr v0.9.2
    Checking ed25519 v2.2.3
    Checking moxcms v0.9.1
    Checking pkcs1 v0.7.5
    Checking jpeg-decoder v0.3.2
    Checking kurbo v0.11.3
    Checking pollster v1.0.1
    Checking ttf-parser v0.25.1
    Checking zbus_xml v5.2.1
    Checking rsa v0.9.10
    Checking gpu-allocator v0.28.0
    Checking ed25519-dalek v2.2.0
    Checking aes-gcm v0.10.3
    Checking wgpu-naga-bridge v30.0.1
    Checking lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-pipeline)
    Checking async-process v2.5.0
    Checking chacha20poly1305 v0.10.1
    Checking ravif v0.13.0
    Checking parley v0.11.1
    Checking vello_common v0.1.0
    Checking httparse v1.10.1
    Checking wgpu-hal v30.0.1
    Checking blocking v1.7.0
    Checking svgtypes v0.15.3
    Checking lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-raw)
    Checking x25519-dalek v2.0.1
    Checking p256 v0.13.2
    Checking p384 v0.13.1
    Checking async-executor v1.14.0
    Checking rustls-webpki v0.102.8
    Checking webpki-roots v1.0.9
    Checking async-broadcast v0.7.2
    Checking lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-scenes)
    Checking lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-codecs)
    Checking http v1.5.0
    Checking emath v0.36.2
    Checking ordered-stream v0.2.0
    Checking photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-psd)
    Checking simplecss v0.2.2
    Checking xkeysym v0.2.1
    Checking regex v1.13.1
    Checking imagesize v0.13.0
    Checking xcursor v0.3.11
    Checking pico-args v0.5.0
    Checking owned_ttf_parser v0.25.1
    Checking linux-raw-sys v0.4.15
    Checking data-url v0.3.2
   Compiling dyn-stack-macros v0.1.3
    Checking xmlwriter v0.1.0
    Checking ab_glyph_rasterizer v0.1.10
    Checking cursor-icon v1.2.0
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking wayland-cursor v0.31.14
    Checking rustls-rustcrypto v0.0.2-alpha
    Checking ecolor v0.36.2
    Checking ab_glyph v0.2.32
    Checking lensfun v0.7.0
    Checking wayland-csd-frame v0.3.0
    Checking usvg v0.45.1
    Checking zbus v5.19.0
    Checking thiserror v1.0.69
    Checking rustix v0.38.44
    Checking vello_cpu v0.1.0
    Checking zbus-lockstep v0.5.2
    Checking unicode-general-category v1.1.0
    Checking lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-catalog)
    Checking lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-preview)
    Checking photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-compose)
    Checking dyn-stack v0.13.2
    Checking linesweeper v0.4.0
    Checking gethostname v1.1.0
    Checking photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-text)
    Checking photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-algo)
    Checking zune-inflate v0.2.54
    Checking bit_field v0.10.3
    Checking twox-hash v2.1.4
   Compiling seq-macro v0.3.6
    Checking as-raw-xcb-connection v1.0.1
    Checking hayro-ccitt v0.3.0
    Checking x11rb-protocol v0.13.2
    Checking lebe v0.5.3
    Checking unicode-segmentation v1.13.3
    Checking epaint_default_fonts v0.36.2
    Checking base64 v0.23.1
    Checking nohash-hasher v0.2.0
    Checking self_cell v1.3.0
    Checking photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-pathops)
    Checking exr v1.74.2
    Checking hayro-jbig2 v0.3.0
    Checking epaint v0.36.2
    Checking ureq-proto v0.6.4
    Checking lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-merge)
    Checking wgpu-core-deps-windows-linux-android v30.0.1
    Checking ruzstd v0.9.0
    Checking wgpu-core v30.0.1
    Checking hayro-jpeg2000 v0.3.5
    Checking skrifa v0.42.1
    Checking write-fonts v0.48.1
    Checking gemm-common v0.19.0
    Checking brotli v8.0.4
    Checking ron v0.12.2
    Checking wasmi_core v2.0.0
   Compiling io-lifetimes v2.0.4
    Checking jpeg-encoder v0.6.1
    Checking constant_time_eq v0.4.2
    Checking foldhash v0.1.5
   Compiling ident_case v1.0.1
    Checking hayro-postscript v0.1.0
    Checking utf8-zero v0.8.1
   Compiling io-lifetimes v3.0.1
    Checking ryu v1.0.23
   Compiling strsim v0.11.1
    Checking option-ext v0.2.0
    Checking calloop v0.13.0
    Checking ureq v3.4.2
    Checking dirs-sys v0.5.0
    Checking pdf-writer v0.15.0
    Checking egui v0.36.2
   Compiling darling_core v0.24.1
    Checking hashbrown v0.15.5
    Checking calloop-wayland-source v0.3.0
    Checking wasmi_ir v2.0.0
    Checking photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-codecs)
    Checking blake3 v1.8.7
    Checking hayro-syntax v0.7.2
    Checking wasmparser v0.228.0
    Checking tiny-skia-path v0.12.0
    Checking sha1 v0.10.7
    Checking ascii v1.1.0
    Checking httpdate v1.0.3
    Checking imagesize v0.14.0
    Checking hayro-cmap v0.1.0
    Checking atspi-common v0.13.0
    Checking chunked_transfer v1.5.0
    Checking wasmi_collections v2.0.0
    Checking bumpalo v3.20.3
   Compiling ref-cast v1.0.27
    Checking data-encoding v2.11.1
    Checking utf-8 v0.7.6
    Checking xmp-writer v0.3.3
   Compiling io-extras v0.19.0
    Checking fixedbitset v0.5.7
    Checking photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-format)
    Checking hayro-interpret v0.7.0
    Checking tungstenite v0.27.0
    Checking tiny_http v0.12.0
    Checking petgraph v0.8.3
    Checking x11-dl v2.21.0
    Checking smithay-client-toolkit v0.19.2
    Checking dirs v6.0.0
    Checking xkbcommon-dl v0.4.2
    Checking accesskit_consumer v0.36.0
    Checking subsetter v0.2.6
    Checking wayland-protocols-plasma v0.3.12
    Checking calloop v0.14.4
    Checking photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-paint)
    Checking photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-raw)
    Checking socket2 v0.6.5
    Checking krilla v0.8.2
    Checking wgpu v30.0.1
    Checking mio v1.2.3
    Checking wasmi v2.0.0
   Compiling ref-cast-impl v1.0.27
   Compiling tokio-macros v2.7.2
   Compiling serde_derive_internals v0.30.0
    Checking dpi v0.1.2
   Compiling cap-primitives v4.0.3
    Checking bitflags v1.3.2
    Checking smol_str v0.2.2
    Checking png v0.17.16
    Checking calloop-wayland-source v0.4.1
    Checking tree_magic_mini v3.2.2
    Checking atspi-proxies v0.13.0
    Checking accesskit_atspi_common v0.18.1
    Checking fs-set-times v0.20.3
    Checking photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-affinity)
    Checking photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-svg)
    Checking photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-vecfx)
    Checking futures-executor v0.3.34
    Checking tract-nnef v0.23.8
    Checking tract-hir v0.23.8
    Checking wayland-protocols-experimental v20250721.0.1
    Checking wayland-protocols-misc v0.3.12
    Checking photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-pdf)
    Checking pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-trace)
    Checking photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ops)
    Checking rustix-linux-procfs v0.1.1
    Checking chrono v0.4.45
    Checking os_pipe v1.2.3
    Checking ipnet v2.12.2
   Compiling rmcp v3.5.0
    Checking maybe-owned v0.3.4
    Checking atspi v0.29.0
   Compiling cap-std v4.0.3
    Checking ambient-authority v0.0.2
    Checking accesskit_unix v0.21.1
    Checking wl-clipboard-rs v0.9.4
    Checking smithay-client-toolkit v0.20.0
    Checking futures v0.3.34
    Checking tiny-skia v0.11.4
    Checking gemm-f32 v0.19.0
    Checking typed-path v0.12.3
    Checking enum-map v2.7.3
    Checking gemm-c64 v0.19.0
    Checking gemm-f64 v0.19.0
    Checking gemm-c32 v0.19.0
    Checking type-map v0.5.1
    Checking gemm-f16 v0.19.0
    Checking tract-extra v0.23.8
    Checking tract-transformers v0.23.8
    Checking li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-attributions)
   Compiling lightcraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-ui-egui)
    Checking smithay-clipboard v0.7.3
   Compiling winresource v0.1.31
    Checking tract-onnx-opl v0.23.8
    Checking x11rb v0.13.2
    Checking float8 v0.6.1
    Checking safetensors v0.7.0
    Checking zip v7.2.0
    Checking num_cpus v1.17.0
    Checking gemm v0.19.0
    Checking resvg v0.45.1
    Checking is-docker v0.2.0
    Checking pollster v0.4.0
    Checking photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-gpu)
    Checking toml_parser v1.1.3+spec-1.1.0
    Checking is-wsl v0.4.0
    Checking rfd v0.17.2
    Checking serde_spanned v1.1.1
    Checking egui_extras v0.36.2
    Checking toml_datetime v1.1.1+spec-1.1.0
    Checking toml_writer v1.1.2+spec-1.1.0
    Checking sys-locale v0.3.2
    Checking candle-core v0.9.2
    Checking open v5.4.4
    Checking lightcraft-fetch v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-fetch)
    Checking toml v1.1.6+spec-1.1.0
    Checking heic-rs v0.1.1
    Checking memory-stats v1.2.0
    Checking lightcraft-sysmem v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-sysmem)
    Checking tract-onnx v0.23.8
    Checking photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-testkit)
    Checking winit v0.30.13
    Checking arboard v3.6.1
   Compiling local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/apps/local-image)
    Checking photocraft-tablet v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-tablet)
    Checking xtask v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/xtask)
    Checking photocraft-heif v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-heif)
    Checking photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-plugins)
    Checking tokio v1.53.1
   Compiling schemars_derive v1.2.2
    Checking candle-nn v0.9.2
    Checking accesskit_winit v0.32.2
    Checking egui-wgpu v0.36.2
    Checking egui-winit v0.36.2
    Checking li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-seg)
    Checking eframe v0.36.2
   Compiling darling_macro v0.24.1
    Checking lightcraft-segment v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-segment)
    Checking lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-gpu)
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/li-ai)
   Compiling darling v0.24.1
   Compiling rmcp-macros v3.5.0
    Checking lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-engine)
    Checking schemars v1.2.2
    Checking tokio-util v0.7.19
    Checking photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-io)
    Checking lightcraft-mcp v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/lc-mcp)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-engine)
    Checking photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-automation)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/crates/pc-ui-egui)
    Checking local-image-cli v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-known-failures/apps/local-image-cli)
    Finished `dev` profile [optimized + debuginfo] target(s) in 42.63s
```

</details>

### Final formatting check

```bash
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate cargo fmt --all -- --check
```

Exit code: `0`.

No output.

### History inspection

Each `$` line below is the exact Git command, followed by its complete output and exit status. This includes history for both reference sides and the earlier borrowed-shape regression.

<details>
<summary>Commands and complete history output</summary>

```text
$ git log -6 --oneline -- crates/pc-engine/src/lib.rs
a027712b codex-hotfix-critical: job result (landed by li-land)
1974facf Vector integration: wire all 48 packages, fix review findings, full validation (Codex gpt-6.1-sol); report docs/wip/CODEX-REPORT-vector-integration.md
8a22cedf Vector package vec-47-shared-tools-unification (Codex gpt-6.1-sol); report in docs/wip/
e1e2a6ae Vector package vec-46-vector-toolbar (Codex gpt-6.1-sol); report in docs/wip/
3d44e504 Merge wip/codex-vector-v1 into wip/vector: pc-pathops (attributions merged and regenerated)
89aea236 Vector V1: pc-pathops geometry engine completed (booleans, offset, flatten, simplify, fit) with the quality harness
Exit code: 0

$ git log -6 --oneline -- crates/pc-engine/src/layer_multi_cmds.rs
a027712b codex-hotfix-critical: job result (landed by li-land)
1974facf Vector integration: wire all 48 packages, fix review findings, full validation (Codex gpt-6.1-sol); report docs/wip/CODEX-REPORT-vector-integration.md
af018456 Apply package 15 symbols shared-file patch (integration/15.md); cover Symbol in scaled view and Library summary
6ace8474 Merge wip/codex-group-move-shape-rerasterise
8dc3090f Group move: stroked shapes reused with translation-aware caching instead of re-rasterising every frame
6eb4d618 codex-move-tool: Codex (gpt-6.1-sol) work; report in docs/wip/
Exit code: 0

$ git log -6 --oneline -- crates/pc-ui-egui/src/layer_reveal.rs
c18d18cf V2: standalone GPL Rust workspace, AI tools in the editor, Library mode, raster fixes
Exit code: 0

$ git log -6 --oneline -- crates/pc-ui-egui/src/layer_reveal_tests.rs
c18d18cf V2: standalone GPL Rust workspace, AI tools in the editor, Library mode, raster fixes
Exit code: 0

$ git log -6 --oneline -- crates/pc-ui-egui/src/vector_ui.rs
f73bc0ca Merge vector integration into the working branch (test build)
1974facf Vector integration: wire all 48 packages, fix review findings, full validation (Codex gpt-6.1-sol); report docs/wip/CODEX-REPORT-vector-integration.md
8a22cedf Vector package vec-47-shared-tools-unification (Codex gpt-6.1-sol); report in docs/wip/
28d67848 Fix 149 confirmed bugs from DeepSeek's code reviews (349 findings verified: 149 real, 181 not bugs, 10 obsolete, 9 deferred)
dc036a52 Compositing: live shape editing, context by selected item, working Pen/Selection bars
0ceb78e7 WIP: Compositing context bar, Pen bar actions, live shape options (stopped mid-work, unverified)
Exit code: 0

$ git log -6 --oneline -- crates/pc-ui-egui/src/shared_tools.rs
1974facf Vector integration: wire all 48 packages, fix review findings, full validation (Codex gpt-6.1-sol); report docs/wip/CODEX-REPORT-vector-integration.md
8a22cedf Vector package vec-47-shared-tools-unification (Codex gpt-6.1-sol); report in docs/wip/
Exit code: 0

$ git log -6 --oneline -- crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy/tests.rs
1974facf Vector integration: wire all 48 packages, fix review findings, full validation (Codex gpt-6.1-sol); report docs/wip/CODEX-REPORT-vector-integration.md
e1e2a6ae Vector package vec-46-vector-toolbar (Codex gpt-6.1-sol); report in docs/wip/
Exit code: 0

$ git log -6 --oneline -- assets/icons/color/blend.svg
83a68b34 Tool icons: approved glossy-duotone set plus six sourced open-source glyphs (Krita, Lucide), re-filled in duotone (owner: mix best of both)
8098e1eb codex-icons-standard: Codex (gpt-6.1-sol) work; report in docs/wip/
1a8dd802 Checkpoint after the machine crash (work in progress, unverified)
Exit code: 0

$ git log -6 --oneline -- xtask/src/tool_art.rs
c0652fae Fix 60 verified DeepSeek findings (post-ds review); report in docs/wip/CODEX-REPORT-ds2-fix.md
b8b64444 Merge wip/codex-icons-sourced: icon mix (approved duotone + six sourced glyphs)
83a68b34 Tool icons: approved glossy-duotone set plus six sourced open-source glyphs (Krita, Lucide), re-filled in duotone (owner: mix best of both)
28d67848 Fix 149 confirmed bugs from DeepSeek's code reviews (349 findings verified: 149 real, 181 not bugs, 10 obsolete, 9 deferred)
8098e1eb codex-icons-standard: Codex (gpt-6.1-sol) work; report in docs/wip/
Exit code: 0

$ git diff c0652fae 1974facf -- crates/pc-engine/src/lib.rs crates/pc-ui-egui/src/layer_reveal.rs crates/pc-ui-egui/src/layer_reveal_tests.rs crates/pc-ui-egui/src/vector_ui.rs
diff --git a/crates/pc-engine/src/lib.rs b/crates/pc-engine/src/lib.rs
index b8fe35aa..b0526d64 100644
--- a/crates/pc-engine/src/lib.rs
+++ b/crates/pc-engine/src/lib.rs
@@ -86,6 +86,7 @@ pub mod render_cmds;
 pub mod retouch_cmds;
 pub mod toolsets;
 // local-image: Edit › Paste Special › Paste Seamless.
+pub mod pathops_cmds;
 pub mod seamless_cmds;
 pub mod select_extra_cmds;
 pub mod selection_cmds;
@@ -104,7 +105,9 @@ pub mod type_extra_cmds;
 pub mod type_spell_cmds;
 pub mod type_styles_cmds;
 mod variables_cmds;
+pub mod vector_api;
 pub mod vector_cmds;
+pub mod vector_packages;
 pub mod vectorize_cmds;
 mod video_cmds;
 pub mod vp_cmds;
@@ -160,6 +163,10 @@ pub(crate) fn active_layer_of(s: &Session) -> std::result::Result<&photocraft_do
 /// Per-document editing state.
 #[derive(Clone, Debug)]
 pub struct DocState {
+    pub vector_selection: vector_packages::selection::Memory,
+    pub vector_selection_scope: vector_packages::selection::Scope,
+    pub vector_isolation: vector_packages::isolation::State,
+    pub vector_view: vector_packages::isolation::ViewState,
     pub doc: Arc<Document>,
     pub history: History,
     /// The primary ("key") layer: what single-layer commands act on. Always a member of
@@ -204,6 +211,10 @@ impl DocState {
         let mut history = History::default();
         history.set_current_layers(LayerTarget { active: active_layer, selected: active_layer.into_iter().collect() });
         Self {
+            vector_selection: Default::default(),
+            vector_selection_scope: Default::default(),
+            vector_isolation: Default::default(),
+            vector_view: Default::default(),
             doc: Arc::new(doc),
             history,
             active_layer,
@@ -229,7 +240,20 @@ impl DocState {
     pub fn selected_layers(&self) -> Vec<LayerId> {
         let Some(active) = self.active_layer else { return Vec::new() };
         let set: &[LayerId] = if self.selected_layers.contains(&active) { &self.selected_layers } else { std::slice::from_ref(&active) };
-        self.doc.walk().into_iter().map(|(_, _, l)| l.id).filter(|id| set.contains(id)).collect()
+        let members = (set.len() > 8).then(|| set.iter().copied().collect::<std::collections::HashSet<_>>());
+        fn collect(layers: &[photocraft_doc::Layer], set: &[LayerId], members: Option<&std::collections::HashSet<LayerId>>, out: &mut Vec<LayerId>) {
+            for layer in layers {
+                if members.map_or_else(|| set.contains(&layer.id), |ids| ids.contains(&layer.id)) {
+                    out.push(layer.id);
+                }
+                if let Some(children) = layer.children() {
+                    collect(children, set, members, out);
+                }
+            }
+        }
+        let mut out = Vec::with_capacity(set.len());
+        collect(&self.doc.layers, set, members.as_ref(), &mut out);
+        out
     }
     pub fn is_layer_selected(&self, id: LayerId) -> bool {
         self.active_layer == Some(id) || (self.active_layer.is_some_and(|a| self.selected_layers.contains(&a)) && self.selected_layers.contains(&id))
@@ -281,6 +305,8 @@ impl Default for ToolState {
 
 #[derive(Default)]
 pub struct Session {
+    /// Captured insertion policy for the active vector drawing gesture; never persisted.
+    pub vector_insertion: Option<vector_integration::Insertion>,
     docs: Vec<DocState>,
     active: Option<usize>,
     pub tools: ToolState,
@@ -350,6 +376,7 @@ pub fn move_item<T>(v: &mut Vec<T>, i: usize, to: usize) -> Option<usize> {
 
 impl Session {
     pub fn new() -> Self {
+        vector_integration::initialize();
         Self::default()
     }
 
@@ -408,6 +435,7 @@ impl Session {
     /// Preserve its identity unless another open document already owns it.
     /// Identity-dependent callers must read the admitted document at the returned index.
     pub fn add_document(&mut self, mut doc: Document, path: Option<String>) -> usize {
+        vector_integration::initialize();
         // Persisted IDs can overlap the allocator, so check every replacement too.
         while self.docs.iter().any(|st| st.doc.id == doc.id) {
             doc.id = photocraft_doc::DocId::fresh();
@@ -443,6 +471,7 @@ impl Session {
     /// Runs synchronously, including job-capable commands (see [`jobs`]; [`Session::start`] runs
     /// those in the background).
     pub fn execute(&mut self, id: &str, params: Value) -> Result<Value> {
+        vector_integration::initialize();
         match self.dispatch(id, params, false)? {
             jobs::Started::Done(v) => Ok(v),
             // Not reached: inline dispatch never starts a job. Waiting is still correct.
@@ -492,6 +521,9 @@ impl Session {
         if let (Some(k), Some(id)) = (restrict, active) {
             channel_cmds::restrict_to_color(&before, &mut doc, id, k);
         }
+        vector_integration::maintain_metadata(&mut doc)?;
+        // Preserve the current non-editing selection as the undo target.
+        st.history.set_current_layers(st.layer_target());
         st.doc = Arc::new(doc);
         st.active_layer = active;
         fix_selection(st);
@@ -607,3 +639,13 @@ pub(crate) fn fix_selection(st: &mut DocState) {
 
 #[cfg(test)]
 mod tests;
+
+// Package 46 layout; shared shell/router hookup is supplied in integration/46.md.
+pub mod vector_toolbar_layout;
+
+pub mod shared_tools;
+
+pub mod vector_integration;
+
+#[cfg(test)]
+mod gpu_test_support;
diff --git a/crates/pc-ui-egui/src/vector_ui.rs b/crates/pc-ui-egui/src/vector_ui.rs
index 5dde2789..6d4bb80d 100644
--- a/crates/pc-ui-egui/src/vector_ui.rs
+++ b/crates/pc-ui-egui/src/vector_ui.rs
@@ -15,13 +15,32 @@ use crate::state::{Tool, ToolOptions};
 use crate::theme::Tokens;
 
 /// Pen tool path under construction: knots as [anchor, in, out] (document px).
-#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
+#[derive(Clone, Debug, Default, Serialize, Deserialize)]
 pub struct PenPath {
     pub knots: Vec<[[f64; 2]; 3]>,
     #[serde(skip)]
     pub dragging: bool,
+    #[serde(skip)]
+    pub shared: crate::vector_packages::pen::State,
 }
 
+impl PartialEq for PenPath {
+    fn eq(&self, other: &Self) -> bool {
+        self.knots == other.knots && self.dragging == other.dragging
+    }
+}
+impl PenPath {
+    fn inspect(&mut self) {
+        if let Some(g) = &self.shared.gesture {
+            let knots = g.path().subpaths.last().map(|s| s.knots.as_slice()).unwrap_or_default();
+            self.knots.resize(knots.len(), [[0.; 2]; 3]);
+            for (dst, k) in self.knots.iter_mut().zip(knots) {
+                *dst = [[k.anchor.x, k.anchor.y], [k.in_ctrl.x, k.in_ctrl.y], [k.out_ctrl.x, k.out_ctrl.y]];
+            }
+            self.dragging = g.captured();
+        }
+    }
+}
 pub fn is_shape_tool(t: Tool) -> bool {
     matches!(t, Tool::Rectangle | Tool::EllipseShape | Tool::Triangle | Tool::Polygon | Tool::Line | Tool::CustomShape)
 }
@@ -41,58 +60,53 @@ fn stroke_param(app: &PhotocraftApp) -> Value {
     if o.stroke_width > 0.0 { json!({"width": o.stroke_width, "color": hex(app.session.tools.background)}) } else { Value::Null }
 }
 
-/// Geometry of a Shape-tool drag as `shape.create` params, without fill and stroke (None when too
-/// small): ⇧ constrains proportions, ⌥ draws from the centre. Custom shapes give their box as a `rect`.
-fn shape_geometry(o: &ToolOptions, tool: Tool, start: [f64; 2], end: [f64; 2], mods: egui::Modifiers) -> Option<Value> {
-    Some(if tool == Tool::Line {
-        let (mut dx, mut dy) = (end[0] - start[0], end[1] - start[1]);
-        if mods.shift {
-            // Snap to 45°.
-            let a = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
-            let len = dx.hypot(dy);
-            (dx, dy) = (len * a.cos(), len * a.sin());
-        }
-        if dx.hypot(dy) < 1.0 {
-            return None;
-        }
-        json!({"kind": "line", "from": start, "to": [start[0] + dx, start[1] + dy], "weight": o.line_weight.max(1.0)})
-    } else {
-        let (mut w, mut h) = (end[0] - start[0], end[1] - start[1]);
-        if mods.shift {
-            let m = w.abs().max(h.abs());
-            (w, h) = (m * w.signum(), m * h.signum());
-        }
-        let (x0, y0) = if mods.alt { (start[0] - w, start[1] - h) } else { (start[0], start[1]) };
-        let (w, h) = if mods.alt { (w * 2.0, h * 2.0) } else { (w, h) };
-        let rect = [x0.min(x0 + w).round(), y0.min(y0 + h).round(), w.abs().round(), h.abs().round()];
-        if rect[2] < 1.0 || rect[3] < 1.0 {
+/// Live design geometry of a Shape-tool drag (None when too small): ⇧ constrains proportions, ⌥ draws from the centre. Custom shapes give their box as a `rect`.
+fn shape_geometry(
+    o: &ToolOptions,
+    tool: Tool,
+    start: [f64; 2],
+    end: [f64; 2],
+    mods: egui::Modifiers,
+) -> Option<photocraft_doc::vector_design::drawing::DesignShape> {
+    use photocraft_doc::vector_design::drawing::{CornerKind, DesignShape, ShapeParameters};
+    use photocraft_geom::Point;
+    let parameters = match tool {
+        Tool::Rectangle | Tool::CustomShape => ShapeParameters::Rect { radii: [f64::from(o.corner_radius); 4], kinds: [CornerKind::Round; 4] },
+        Tool::Polygon => ShapeParameters::Polygon { sides: o.polygon_sides.max(3), curved: 0., corner: 0. },
+        Tool::EllipseShape => ShapeParameters::Ellipse,
+        Tool::Triangle => ShapeParameters::Triangle { top: 0.5 },
+        Tool::Line => ShapeParameters::Line { from: start, to: end },
+        _ => return None,
+    };
+    let original = DesignShape { rect: [start[0], start[1], 1., 1.], angle: 0., parameters };
+    let d = crate::vector_packages::drawing::drag_design(&original, Point::new(start[0], start[1]), Point::new(end[0], end[1]), mods, |p, _, _| p);
+    if tool == Tool::Line {
+        if (end[0] - start[0]).hypot(end[1] - start[1]) < 1. {
             return None;
         }
-        match tool {
-            Tool::Rectangle if o.corner_radius > 0.0 => json!({"kind": "roundedRect", "rect": rect, "radii": vec![o.corner_radius; 4]}),
-            Tool::Rectangle | Tool::CustomShape => json!({"kind": "rect", "rect": rect}),
-            Tool::EllipseShape => json!({"kind": "ellipse", "rect": rect}),
-            Tool::Triangle => json!({"kind": "polygon", "rect": rect, "sides": 3}),
-            _ => json!({"kind": "polygon", "rect": rect, "sides": o.polygon_sides.max(3)}),
-        }
-    })
+    } else if d.rect[2] < 1. || d.rect[3] < 1. {
+        return None;
+    }
+    Some(d)
 }
 
 /// Finish a Shape-tool drag: ⇧ constrains proportions, ⌥ draws from the centre.
 pub fn finish_shape(app: &mut PhotocraftApp, tool: Tool, start: [f64; 2], end: [f64; 2], mods: egui::Modifiers) {
-    let Some(mut p) = shape_geometry(&app.ui.tool_options, tool, start, end, mods) else { return };
-    let fill = if app.ui.tool_options.shape_fill { json!(hex(app.session.tools.foreground)) } else { Value::Null };
-    let stroke = stroke_param(app);
+    let Some(design) = shape_geometry(&app.ui.tool_options, tool, start, end, mods) else { return };
+    let (fill, stroke) = crate::shared_tools::shape_paints(app, &design.parameters);
     if tool == Tool::CustomShape {
-        // ⇧ keeps the shape's proportions (the rect is already squared).
-        if let Ok(rect) = serde_json::from_value(p["rect"].take()) {
-            crate::preset_panels::finish_custom_shape(app, rect, mods.shift, fill, stroke);
-        }
+        crate::preset_panels::finish_custom_shape(
+            app,
+            design.rect,
+            mods.shift,
+            if fill.is_some() { json!(hex(app.session.tools.foreground)) } else { Value::Null },
+            stroke_param(app),
+        );
         return;
     }
-    p["fill"] = fill;
-    p["stroke"] = stroke;
-    if let Err(e) = app.run("shape.create", p) {
+    let Some(st) = app.session.active() else { return };
+    let p = json!({"document":st.doc.id.0,"revision":st.revision,"above":st.active_layer.map(|id|id.0),"design":design,"fill":fill,"stroke":stroke});
+    if let Err(e) = app.run("shape.design.create", p) {
         app.ui.status = e;
         app.ui.status_error = true;
     }
@@ -102,7 +116,9 @@ pub fn finish_shape(app: &mut PhotocraftApp, tool: Tool, start: [f64; 2], end: [
 /// path outline. Custom shapes show their box outline only.
 pub fn draw_shape_preview(app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform, tool: Tool, start: [f64; 2], end: [f64; 2], mods: egui::Modifiers) {
     let o = &app.ui.tool_options;
-    let Some(path) = shape_geometry(o, tool, start, end, mods).and_then(|p| photocraft_engine::vector_cmds::shape_path(&p).ok()) else { return };
+    let Some(path) = shape_geometry(o, tool, start, end, mods).and_then(|p| photocraft_engine::vector_packages::drawing::geometry::generate(&p).ok()) else {
+        return;
+    };
     let custom = tool == Tool::CustomShape;
     // ponytail: preview colours skip the canvas's colour management; the commit renders them exactly.
     let fill = if o.shape_fill && !custom { rgb32(app.session.tools.foreground) } else { Color32::TRANSPARENT };
@@ -119,66 +135,88 @@ pub fn draw_shape_preview(app: &PhotocraftApp, painter: &egui::Painter, xf: &Vie
 // ---------------------------------------------------------------------------------------------
 // Pen
 
-fn pen_to_json(pen: &PenPath, closed: bool) -> Value {
-    json!({"subpaths": [{"closed": closed, "knots": pen.knots.iter().map(|k| json!({"anchor": k[0], "in": k[1], "out": k[2], "smooth": k[1] != k[0] || k[2] != k[0]})).collect::<Vec<_>>()}]})
+/// Compatibility entrypoints delegate to the full shared Pen handler. The new router
+/// passes live modifiers through pen_pointer; historic callers use their current drag.
+pub fn pen_pointer(app: &mut PhotocraftApp, ev: crate::canvas::ToolEvent, mods: egui::Modifiers) {
+    let mut facade = app.ui.pen.take().unwrap_or_default();
+    facade.shared.options = app.ui.tool_options.pen_options.clone();
+    let paints = crate::shared_tools::pen_paints(app, facade.shared.options.mode);
+    let zoom = f64::from(app.current_zoom());
+    let result = crate::shared_tools::pen_pointer(
+        &mut facade.shared,
+        &mut app.session,
+        ev,
+        mods,
+        app.ui.tool_options.vector_mode == "shape",
+        app.ui.vector_mask_target,
+        zoom,
+        paints,
+    );
+    match result {
+        Ok(()) if facade.shared.gesture.is_some() => {
+            facade.inspect();
+            app.ui.pen = Some(facade);
+        }
+        Ok(()) => {}
+        Err(e) => {
+            app.ui.status = e.to_string();
+            app.ui.status_error = true;
+        }
+    }
 }
-
-/// Pen press: close on the first anchor, else add an anchor (dragging pulls smooth handles).
 pub fn pen_down(app: &mut PhotocraftApp, x: f64, y: f64) {
-    let tol = 6.0 / app.current_zoom().max(0.01) as f64;
-    let pen = app.ui.pen.get_or_insert_with(PenPath::default);
-    if let Some(first) = pen.knots.first().map(|k| k[0])
-        && pen.knots.len() >= 2
-        && (first[0] - x).hypot(first[1] - y) < tol
-    {
-        pen_commit(app, true);
-        return;
-    }
-    pen.knots.push([[x, y]; 3]);
-    pen.dragging = true;
+    pen_pointer(app, crate::canvas::ToolEvent::Down { x, y, pressure: 1. }, egui::Modifiers::NONE);
 }
-
 pub fn pen_move(app: &mut PhotocraftApp, x: f64, y: f64) {
-    if let Some(pen) = app.ui.pen.as_mut()
-        && pen.dragging
-        && let Some(k) = pen.knots.last_mut()
-    {
-        let a = k[0];
-        k[2] = [x, y];
-        k[1] = [2.0 * a[0] - x, 2.0 * a[1] - y];
-    }
+    pen_pointer(app, crate::canvas::ToolEvent::Move { x, y, pressure: 1. }, egui::Modifiers::NONE);
 }
-
 pub fn pen_up(app: &mut PhotocraftApp) {
-    if let Some(pen) = app.ui.pen.as_mut() {
-        pen.dragging = false;
+    // Release the placed knot without moving a handle back to its anchor.
+    let result = app.ui.pen.as_mut().map(|p| p.shared.up(&mut app.session));
+    if let Some(Err(e)) = result {
+        app.ui.status = e.to_string();
+        app.ui.status_error = true;
+        app.ui.pen = None;
+    }
+    if app.ui.pen.as_ref().is_some_and(|p| p.shared.gesture.is_none()) {
+        app.ui.pen = None;
+    }
+    if let Some(p) = &mut app.ui.pen {
+        p.inspect();
     }
 }
 
-/// Finish the pen path: a work path (Path mode) or a new shape layer (Shape mode).
+/// Finish through the same atomic command as package 04, including old serialized drafts.
 pub fn pen_commit(app: &mut PhotocraftApp, closed: bool) {
-    let Some(pen) = app.ui.pen.take() else { return };
-    if pen.knots.len() < 2 {
-        return;
-    }
-    let path = pen_to_json(&pen, closed);
-    let r = if app.ui.tool_options.vector_mode == "shape" {
-        let fill = if closed && app.ui.tool_options.shape_fill { json!(hex(app.session.tools.foreground)) } else { Value::Null };
-        let stroke =
-            if closed { stroke_param(app) } else { json!({"width": app.ui.tool_options.stroke_width.max(1.0), "color": hex(app.session.tools.foreground)}) };
-        app.run("shape.create", json!({"kind": "path", "path": path, "fill": fill, "stroke": stroke}))
-    } else if let Some((id, existing)) = targeted_vector_mask(app) {
-        // A targeted vector mask takes the new subpath (#196), as in Photoshop.
-        let mut p = photocraft_engine::vector_cmds::path_json(&existing);
-        if let (Some(subs), Some(new)) = (p.get_mut("subpaths").and_then(Value::as_array_mut), path.get("subpaths").and_then(Value::as_array)) {
-            subs.extend(new.iter().cloned());
-        }
-        app.run("layer.vectorMask.edit", json!({"layer": id, "path": p}))
+    let Some(mut pen) = app.ui.pen.take() else { return };
+    let result = if pen.shared.gesture.is_some() {
+        pen.shared.enter(&mut app.session)
+    } else if pen.knots.len() >= 2 {
+        use photocraft_doc::{Knot, Path, Subpath};
+        use photocraft_geom::Point;
+        let path = Path {
+            subpaths: vec![Subpath {
+                closed,
+                knots: pen
+                    .knots
+                    .iter()
+                    .map(|k| Knot {
+                        anchor: Point::new(k[0][0], k[0][1]),
+                        in_ctrl: Point::new(k[1][0], k[1][1]),
+                        out_ctrl: Point::new(k[2][0], k[2][1]),
+                        smooth: k[0] != k[1] || k[0] != k[2],
+                    })
+                    .collect(),
+                ..Default::default()
+            }],
+            ..Default::default()
+        };
+        crate::shared_tools::commit_legacy_pen(app, path)
     } else {
-        app.run("path.set", json!({"name": "work", "path": path}))
+        Ok(())
     };
-    if let Err(e) = r {
-        app.ui.status = e;
+    if let Err(e) = result {
+        app.ui.status = e.to_string();
         app.ui.status_error = true;
     }
 }
@@ -201,6 +239,11 @@ pub fn active_path_name(app: &PhotocraftApp) -> Option<String> {
 /// Selection tool or a path selected in the Paths panel. A path being drawn with the Pen is
 /// finished first and loads; otherwise [`active_path_name`]. `params` may set feather, mode…
 pub fn path_to_selection(app: &mut PhotocraftApp, params: Value) -> Result<Value, String> {
+    if let Some(gesture) = &app.shared_tools.pen.gesture {
+        let target = gesture.request().target.clone();
+        app.shared_tools.pen.enter(&mut app.session).map_err(|e| e.to_string())?;
+        app.ui.selected_path = Some(if matches!(target, photocraft_engine::vector_packages::pen::Target::Work) { "work" } else { "layer" }.into());
+    }
     if app.ui.pen.as_ref().is_some_and(|p| p.knots.len() >= 2) {
         // The finished path becomes the work path, or a shape layer's path or a vector mask.
         let on_layer = app.ui.tool_options.vector_mode == "shape" || targeted_vector_mask(app).is_some();
@@ -226,11 +269,14 @@ enum PathTarget {
 }
 
 /// The targeted vector mask of the active layer, if the Layers panel targets it.
-pub(crate) fn targeted_vector_mask(app: &PhotocraftApp) -> Option<(u64, Path)> {
+pub(crate) fn targeted_vector_mask_ref(app: &PhotocraftApp) -> Option<(u64, &Path)> {
     let st = app.session.active()?;
     let l = st.active_layer.and_then(|id| st.doc.layer(id))?;
     (app.ui.vector_mask_target && !matches!(l.content, LayerContent::Shape(_))).then_some(())?;
-    l.vector_mask.as_ref().map(|m| (l.id.0, m.path.clone()))
+    l.vector_mask.as_ref().map(|m| (l.id.0, &m.path))
+}
+pub(crate) fn targeted_vector_mask(app: &PhotocraftApp) -> Option<(u64, Path)> {
+    targeted_vector_mask_ref(app).map(|(id, path)| (id, path.clone()))
 }
 
 /// The active shape layer's path, if the active layer is a shape layer.
@@ -363,34 +409,12 @@ pub fn draw_overlay(app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform
             }
         }
         // A targeted vector mask shows its path with any tool (#196).
-        if let Some((_, p)) = targeted_vector_mask(app) {
-            draw_path(&p, tool == Tool::PathSelection);
+        if let Some((_, p)) = targeted_vector_mask_ref(app) {
+            draw_path(p, tool == Tool::PathSelection);
         }
     }
-    // Pen path in progress, with handles of the last knot and a rubber band to the pointer.
     if let Some(pen) = &app.ui.pen {
-        let mut pts = Vec::new();
-        for w in pen.knots.windows(2) {
-            for k in 0..=16 {
-                pts.push(to_scr(bezier(w[0][0], w[0][2], w[1][1], w[1][0], k as f64 / 16.0)));
-            }
-        }
-        painter.add(egui::Shape::line(pts, Stroke::new(1.5, accent)));
-        if let (Some(last), Some(h)) = (pen.knots.last(), app.hover_doc)
-            && !pen.dragging
-        {
-            painter.add(egui::Shape::dashed_line(&[to_scr(last[0]), to_scr(h)], Stroke::new(1.0, accent), 4.0, 3.0));
-        }
-        for (i, k) in pen.knots.iter().enumerate() {
-            let c = to_scr(k[0]);
-            let last = i + 1 == pen.knots.len();
-            if last && k[2] != k[0] {
-                for hnd in [k[1], k[2]] {
-                    draw_handle(painter, c, to_scr(hnd), accent);
-                }
-            }
-            draw_anchor(painter, c, last, accent);
-        }
+        crate::vector_packages::pen::paint_overlay(&pen.shared, painter, |p| xf.to_screen(p.x as f32, p.y as f32));
     }
 }
 
@@ -433,6 +457,9 @@ pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool) -> bo
         let opts = [("path".to_string(), tl!("Path")), ("shape".to_string(), tl!("Shape"))];
         crate::widgets::dropdown(ui, "pen-mode", &mut app.ui.tool_options.vector_mode, &opts, 80.0);
         crate::widgets::vline(ui, 22.0);
+        if crate::vector_packages::pen::context_options(ui, &mut app.ui.tool_options.pen_options) {
+            app.ui.pen = None;
+        }
         if app.ui.tool_options.vector_mode == "shape" && app.ui.pen.is_none() && shape_layer_bar(app, ui) {
             return true;
         }
@@ -639,14 +666,14 @@ pub fn box_controls(ui: &mut egui::Ui, sh: &photocraft_doc::vector::ShapeLayer,
                 json!({"rect": [x, y, nw.round().max(1.0), nh.round().max(1.0)], "coalesce": key(id, "wh")})
             } else {
                 // A free-form path scales about its top-left corner.
-                let (sx, sy) = (nw.max(1.0) / w.max(1e-6), nh.max(1.0) / h.max(1e-6));
+                let (sx, sy) = (if w > 1e-9 { nw / w } else { 1. }, if h > 1e-9 { nh / h } else { 1. });
                 json!({"transform": [sx, 0.0, 0.0, sy, x - sx * x, y - sy * y], "coalesce": key(id, "wh")})
             }
         };
-        if let Some(v) = field(ui, "W", w.round(), 1.0..=BIG, "px") {
+        if let Some(v) = field(ui, "W", w.round(), if live_box { 1.0 } else { 0.0 }..=BIG, "px") {
             edit = Some(resize(v, h));
         }
-        if let Some(v) = field(ui, "H", h.round(), 1.0..=BIG, "px") {
+        if let Some(v) = field(ui, "H", h.round(), if live_box { 1.0 } else { 0.0 }..=BIG, "px") {
             edit = Some(resize(w, v));
         }
     }
@@ -722,29 +749,25 @@ pub fn kind_controls(ui: &mut egui::Ui, sh: &photocraft_doc::vector::ShapeLayer,
 /// Options bar for the selected shape layer: Fill, Stroke (colour, width, type), W/H/X/Y and
 /// the shape's own parameters, all editing the layer. False when no shape layer is selected.
 pub fn shape_layer_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> bool {
-    let Some(st) = app.session.active() else { return false };
-    let Some(id) = st.active_layer else { return false };
-    let doc = st.doc.clone();
-    let Some(layer) = doc.layer(id) else { return false };
-    let LayerContent::Shape(sh) = &layer.content else { return false };
+    let Some((id, sh)) = active_shape(app) else { return false };
     let mut edit = None;
     dim(ui, tl!("Fill:"));
-    edit = fill_control(ui, sh, id).or(edit);
+    edit = fill_control(ui, &sh, id).or(edit);
     dim(ui, tl!("Stroke:"));
-    edit = stroke_color_control(ui, sh, id).or(edit);
-    edit = stroke_width_control(ui, sh, id, 58.0).or(edit);
-    edit = stroke_type_control(ui, sh, id).or(edit);
+    edit = stroke_color_control(ui, &sh, id).or(edit);
+    edit = stroke_width_control(ui, &sh, id, 58.0).or(edit);
+    edit = stroke_type_control(ui, &sh, id).or(edit);
     crate::widgets::vline(ui, 22.0);
-    edit = box_controls(ui, sh, id, 62.0).or(edit);
+    edit = box_controls(ui, &sh, id, 62.0).or(edit);
     if sh.live.as_ref().is_some_and(|l| !matches!(l, photocraft_doc::vector::LiveShape::Ellipse { .. })) {
         crate::widgets::vline(ui, 22.0);
-        edit = kind_controls(ui, sh, id, false).or(edit);
+        edit = kind_controls(ui, &sh, id, false).or(edit);
         if matches!(sh.live, Some(photocraft_doc::vector::LiveShape::Rect { .. })) {
             let corners = ui.button(tl!("Corners"));
             egui::Popup::menu(&corners).show(|ui| {
                 ui.set_min_width(190.0);
                 ui.horizontal_wrapped(|ui| {
-                    edit = kind_controls(ui, sh, id, true).or(edit.take());
+                    edit = kind_controls(ui, &sh, id, true).or(edit.take());
                 });
             });
         }
@@ -1169,52 +1192,6 @@ mod tests {
         assert!(!entries.iter().any(|(_, id, _)| *id == "path.delete"));
     }
 
-    #[test]
-    fn ds_finding_borrowed_shape_options_allow_corner_editing() {
-        use egui_kittest::kittest::Queryable;
-        let mut app = app();
-        finish_shape(&mut app, Tool::Rectangle, [10., 10.], [110., 90.], egui::Modifiers::NONE);
-        let doc = app.session.active().unwrap().doc.clone();
-        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400., 160.)).build_ui_state(
-            |ui, app: &mut PhotocraftApp| {
-                ui.horizontal(|ui| {
-                    assert!(shape_layer_bar(app, ui));
-                });
-            },
-            app,
-        );
-        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
-        h.run_steps(3);
-        h.get_by_label("Corners").click();
-        h.run_steps(2);
-        let at = h.get_by_label("Top-left corner radius").rect().center();
-        h.hover_at(at);
-        h.run_steps(1);
-        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
-        h.run_steps(1);
-        h.event(egui::Event::PointerMoved(at + egui::vec2(25., 0.)));
-        h.run_steps(1);
-        h.event(egui::Event::PointerButton {
-            pos: at + egui::vec2(25., 0.),
-            button: egui::PointerButton::Primary,
-            pressed: false,
-            modifiers: egui::Modifiers::NONE,
-        });
-        h.run_steps(2);
-        let radius = |doc: &photocraft_doc::Document| {
-            let sh = match &doc.layers.last().unwrap().content {
-                LayerContent::Shape(sh) => sh,
-                _ => panic!("shape"),
-            };
-            match sh.live.as_ref().unwrap() {
-                photocraft_doc::vector::LiveShape::Rect { radii, .. } => radii[0],
-                _ => panic!("rect"),
-            }
-        };
-        assert_eq!(radius(&doc), 0.);
-        assert!(radius(&h.state().session.active().unwrap().doc) > 0.);
-    }
-
     #[test]
     fn shape_drag_creates_layers_with_modifiers() {
         let mut app = app();
@@ -1260,7 +1237,7 @@ mod tests {
         app.ui.tool_options.corner_radius = 8.0;
         let preview = |app: &PhotocraftApp, tool, end| {
             shape_geometry(&app.ui.tool_options, tool, [10.0, 10.0], end, egui::Modifiers::ALT)
-                .and_then(|p| photocraft_engine::vector_cmds::shape_path(&p).ok())
+                .and_then(|p| photocraft_engine::vector_packages::drawing::geometry::generate(&p).ok())
         };
         for tool in [Tool::Rectangle, Tool::EllipseShape, Tool::Triangle, Tool::Polygon, Tool::Line] {
             let shown = preview(&app, tool, [60.0, 30.0]).unwrap();
@@ -1286,6 +1263,7 @@ mod tests {
         pen_move(&mut app, 140.0, 140.0); // smooth knot
         pen_up(&mut app);
         pen_down(&mut app, 20.5, 20.5); // click the first anchor: close
+        pen_up(&mut app);
         assert!(app.ui.pen.is_none());
         let wp = app.session.active().unwrap().doc.work_path.clone().unwrap();
         assert!(wp.subpaths[0].closed && wp.subpaths[0].knots.len() == 3);
@@ -1361,3 +1339,21 @@ mod tests {
 #[cfg(test)]
 #[path = "path_selection_tests.rs"]
 mod path_selection_tests;
+
+#[cfg(test)]
+mod zero_axis_options_regression {
+    #[test]
+    fn zero_height_curve_options_do_not_publish_idle_edits() {
+        let shape = photocraft_doc::ShapeLayer {
+            path: photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polyline(&[(20., 90.), (220., 90.)])]),
+            ..Default::default()
+        };
+        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800., 80.)).build_ui_state(
+            |ui, shape| {
+                ui.horizontal(|ui| assert!(super::box_controls(ui, shape, photocraft_doc::LayerId(1), 62.).is_none()));
+            },
+            shape,
+        );
+        h.run_steps(5);
+    }
+}
Exit code: 0

$ git show 28d67848 -- crates/pc-ui-egui/src/vector_ui.rs
commit 28d67848ff6ad6369baec6ee934f7f218effb6ec
Author: Claude <noreply@anthropic.com>
Date:   Sat Oct 10 12:50:11 2026 -0400

    Fix 149 confirmed bugs from DeepSeek's code reviews (349 findings verified: 149 real, 181 not bugs, 10 obsolete, 9 deferred)
    
    Each fix has a regression test; also the safetensors metadata family classification. Codex (gpt-6.1-sol);
    report in docs/wip/CODEX-REPORT-ds-findings.md
    
    Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>

diff --git a/crates/pc-ui-egui/src/vector_ui.rs b/crates/pc-ui-egui/src/vector_ui.rs
index 0e9df1e8..5dde2789 100644
--- a/crates/pc-ui-egui/src/vector_ui.rs
+++ b/crates/pc-ui-egui/src/vector_ui.rs
@@ -722,25 +722,29 @@ pub fn kind_controls(ui: &mut egui::Ui, sh: &photocraft_doc::vector::ShapeLayer,
 /// Options bar for the selected shape layer: Fill, Stroke (colour, width, type), W/H/X/Y and
 /// the shape's own parameters, all editing the layer. False when no shape layer is selected.
 pub fn shape_layer_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> bool {
-    let Some((id, sh)) = active_shape(app) else { return false };
+    let Some(st) = app.session.active() else { return false };
+    let Some(id) = st.active_layer else { return false };
+    let doc = st.doc.clone();
+    let Some(layer) = doc.layer(id) else { return false };
+    let LayerContent::Shape(sh) = &layer.content else { return false };
     let mut edit = None;
     dim(ui, tl!("Fill:"));
-    edit = fill_control(ui, &sh, id).or(edit);
+    edit = fill_control(ui, sh, id).or(edit);
     dim(ui, tl!("Stroke:"));
-    edit = stroke_color_control(ui, &sh, id).or(edit);
-    edit = stroke_width_control(ui, &sh, id, 58.0).or(edit);
-    edit = stroke_type_control(ui, &sh, id).or(edit);
+    edit = stroke_color_control(ui, sh, id).or(edit);
+    edit = stroke_width_control(ui, sh, id, 58.0).or(edit);
+    edit = stroke_type_control(ui, sh, id).or(edit);
     crate::widgets::vline(ui, 22.0);
-    edit = box_controls(ui, &sh, id, 62.0).or(edit);
+    edit = box_controls(ui, sh, id, 62.0).or(edit);
     if sh.live.as_ref().is_some_and(|l| !matches!(l, photocraft_doc::vector::LiveShape::Ellipse { .. })) {
         crate::widgets::vline(ui, 22.0);
-        edit = kind_controls(ui, &sh, id, false).or(edit);
+        edit = kind_controls(ui, sh, id, false).or(edit);
         if matches!(sh.live, Some(photocraft_doc::vector::LiveShape::Rect { .. })) {
             let corners = ui.button(tl!("Corners"));
             egui::Popup::menu(&corners).show(|ui| {
                 ui.set_min_width(190.0);
                 ui.horizontal_wrapped(|ui| {
-                    edit = kind_controls(ui, &sh, id, true).or(edit.take());
+                    edit = kind_controls(ui, sh, id, true).or(edit.take());
                 });
             });
         }
@@ -1165,6 +1169,52 @@ mod tests {
         assert!(!entries.iter().any(|(_, id, _)| *id == "path.delete"));
     }
 
+    #[test]
+    fn ds_finding_borrowed_shape_options_allow_corner_editing() {
+        use egui_kittest::kittest::Queryable;
+        let mut app = app();
+        finish_shape(&mut app, Tool::Rectangle, [10., 10.], [110., 90.], egui::Modifiers::NONE);
+        let doc = app.session.active().unwrap().doc.clone();
+        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400., 160.)).build_ui_state(
+            |ui, app: &mut PhotocraftApp| {
+                ui.horizontal(|ui| {
+                    assert!(shape_layer_bar(app, ui));
+                });
+            },
+            app,
+        );
+        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
+        h.run_steps(3);
+        h.get_by_label("Corners").click();
+        h.run_steps(2);
+        let at = h.get_by_label("Top-left corner radius").rect().center();
+        h.hover_at(at);
+        h.run_steps(1);
+        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
+        h.run_steps(1);
+        h.event(egui::Event::PointerMoved(at + egui::vec2(25., 0.)));
+        h.run_steps(1);
+        h.event(egui::Event::PointerButton {
+            pos: at + egui::vec2(25., 0.),
+            button: egui::PointerButton::Primary,
+            pressed: false,
+            modifiers: egui::Modifiers::NONE,
+        });
+        h.run_steps(2);
+        let radius = |doc: &photocraft_doc::Document| {
+            let sh = match &doc.layers.last().unwrap().content {
+                LayerContent::Shape(sh) => sh,
+                _ => panic!("shape"),
+            };
+            match sh.live.as_ref().unwrap() {
+                photocraft_doc::vector::LiveShape::Rect { radii, .. } => radii[0],
+                _ => panic!("rect"),
+            }
+        };
+        assert_eq!(radius(&doc), 0.);
+        assert!(radius(&h.state().session.active().unwrap().doc) > 0.);
+    }
+
     #[test]
     fn shape_drag_creates_layers_with_modifiers() {
         let mut app = app();
Exit code: 0

$ git show c0652fae -- xtask/src/tool_art.rs
commit c0652faec0d82eaf8e7ec4a5e4b78a32493d7f98
Author: Claude <noreply@anthropic.com>
Date:   Sat Oct 10 16:03:07 2026 -0400

    Fix 60 verified DeepSeek findings (post-ds review); report in docs/wip/CODEX-REPORT-ds2-fix.md
    
    Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>

diff --git a/xtask/src/tool_art.rs b/xtask/src/tool_art.rs
index 100fc209..9bf17e8f 100644
--- a/xtask/src/tool_art.rs
+++ b/xtask/src/tool_art.rs
@@ -154,7 +154,8 @@ fn repaint(source: &str, tool: &Tool, mono: bool, layer: Layer) -> Result<String
                 writer.write_event(if empty { Event::Empty(element) } else { Event::Start(element) })?;
                 if root {
                     root = false;
-                    let (top, bottom) = if mono { ("#ffffff", "#ffffff") } else { palette(&tool.family).with_context(|| format!("unknown icon family: {}", tool.family))? };
+                    let (top, bottom) =
+                        if mono { ("#ffffff", "#ffffff") } else { palette(&tool.family).with_context(|| format!("unknown icon family: {}", tool.family))? };
                     let (metal_top, metal_bottom) = if mono { ("#ffffff", "#ffffff") } else { palette("grey").expect("built-in grey palette") };
                     let shine = if mono { "#ffffff" } else { "#F7F9FC" };
                     writer.get_mut().extend_from_slice(format!("<defs><linearGradient id=\"mix-face\" x2=\".25\" y2=\"1\"><stop stop-color=\"{top}\"/><stop offset=\"1\" stop-color=\"{bottom}\"/></linearGradient><linearGradient id=\"mix-metal\" x2=\".25\" y2=\"1\"><stop stop-color=\"{metal_top}\"/><stop offset=\"1\" stop-color=\"{metal_bottom}\"/></linearGradient><linearGradient id=\"mix-shine\" x2=\"0\" y2=\"1\"><stop stop-color=\"{shine}\" stop-opacity=\".28\"/><stop offset=\"1\" stop-color=\"{shine}\" stop-opacity=\"0\"/></linearGradient></defs>").as_bytes());
@@ -197,7 +198,7 @@ fn artwork(root: &Path, manifest: &Manifest, tool: &Tool, mono: bool) -> Result<
             STANDARD.encode(repaint(text, tool, mono, layer)?)
         ));
     }
-    let commit = &manifest.sets[&source.set].commit;
+    let commit = &manifest.sets.get(&source.set).context("source references an unknown icon set")?.commit;
     Ok(format!(
         "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\">\n<!-- {} @ {commit} ({}); filled Glossy Duotone adaptation GPL-3.0-or-later, {} family. -->\n<g transform=\"translate(1.5 1.5) scale(.875)\">{images}</g>\n</svg>\n",
         component.source, source.licence, tool.family
@@ -223,6 +224,7 @@ pub fn run(root: &Path) -> Result<()> {
     let manifest = manifest(root)?;
     for tool in &manifest.tools {
         for (dir, mono) in [("color", false), ("tools", true)] {
+            std::fs::create_dir_all(root.join("assets/icons").join(dir))?;
             std::fs::write(root.join(format!("assets/icons/{dir}/{}.svg", tool.id)), artwork(root, &manifest, tool, mono)?)?;
         }
     }
@@ -248,6 +250,43 @@ pub fn run(root: &Path) -> Result<()> {
 
 #[cfg(test)]
 mod tests {
+
+    #[test]
+    fn ds2_unknown_icon_set_returns_error_and_generation_creates_destinations() {
+        let source = crate::root();
+        let mut manifest = manifest(&source).unwrap();
+        let tool = manifest.tools.iter().find(|t| t.mode == "sourced").unwrap();
+        let name = &tool.components[0].source;
+        manifest.sources.get_mut(name).unwrap().set = "missing".into();
+        assert!(artwork(&source, &manifest, tool, false).unwrap_err().to_string().contains("unknown icon set"));
+        fn copy(source: &Path, dest: &Path) {
+            std::fs::create_dir_all(dest).unwrap();
+            for entry in std::fs::read_dir(source).unwrap() {
+                let entry = entry.unwrap();
+                if entry.file_type().unwrap().is_dir() {
+                    copy(&entry.path(), &dest.join(entry.file_name()));
+                } else {
+                    std::fs::copy(entry.path(), dest.join(entry.file_name())).unwrap();
+                }
+            }
+        }
+        let root = source.join("target/ds2/icon-bootstrap");
+        let _ = std::fs::remove_dir_all(&root);
+        copy(&source.join("assets/icons"), &root.join("assets/icons"));
+        for file in [&manifest.approved.colour, &manifest.approved.monochrome] {
+            let dest = root.join(file);
+            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
+            std::fs::copy(source.join(file), dest).unwrap();
+        }
+        std::fs::remove_dir_all(root.join("assets/icons/color")).unwrap();
+        std::fs::remove_dir_all(root.join("assets/icons/tools")).unwrap();
+        std::fs::create_dir_all(root.join("crates/pc-ui-egui/src")).unwrap();
+        run(&root).unwrap();
+        assert!(root.join("assets/icons/color/crop.svg").is_file());
+        assert!(root.join("assets/icons/tools/crop.svg").is_file());
+        std::fs::remove_dir_all(root).unwrap();
+    }
+
     use super::*;
     #[test]
     fn mixed_tool_artwork_is_pinned_fresh_and_preserves_the_approved_base() {
Exit code: 0

```

</details>

### Final diff check

```bash
git diff --check
```

Exit code: `0`. No output.
