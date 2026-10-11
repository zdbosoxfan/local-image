# Critical hotfix report

Continued the existing `f73bc0ca` integration on `wip/codex-hotfix-critical`; reused its shared vector algorithms and package 23 fallback. Changes are uncommitted. The pre-existing edit to `CODEX-TASK.md` is preserved.

| Finding | Change | Regression evidence |
| --- | --- | --- |
| O-001 | Allow the vector options scroll area to shrink horizontally, leaving room for Pixel controls. | Headless tests check actual control rectangles in the visible row for Pixel and Vector, with Pro Medium and Studio themes. Before: Pixel Opacity at x=1638.8–1686.3 in a 1400-point window. |
| O-002 | Transform design-shape metadata with paths in committed moves, held Move previews, Free Transform, shape X/Y/path transforms, and canvas/image geometry. Move crop paths and live-group anchors too. Propagate validation errors through callers. Avoid scaling layout corner radii twice. | Rectangle, Ellipse, Polygon and Line projected paths/cache pixels agree with stored geometry after movement, uniform/nonuniform transforms, canvas/image resize and undo; held previews stay detached. Repeat anchor/clip follows layer and canvas transforms. Before: stored moved bounds `(40,30,70,50)` versus displayed `(10,10,40,30)`; Repeat anchor remains `(30,30)` instead of `(40,50)`. |
| O-003 | Inline canvas text bypasses vector shortcuts after the path-text hook. Fill/stroke proxy uses the shared focus guard. | Pixel/Vector and Illustrator/Affinity keyboard tests type `OKHXD/D` without changing tools or paint; Esc ends editing. Before: `O` switches ArtText to Reflect. |
| O-004 | Implement package 23's Projection using the common canvas projection. Connect both native save services and headless encoding to the projection-aware IO bridge for PSD/PSB/layered TIFF. Bake advanced content on a detached copy, preserve the layer compositing envelope, and collect the original editability losses before evaluated metadata is cleared. | Repeat, Blend, Envelope and crop fixtures save/reopen as PSD, PSB and layered TIFF with pixels matching the projected canvas. Actual SVG placement saves `photo-Edit.psd`/`.tif` with one original loss notice. Native service regression exercises the Library-style filename. Native documents remain unchanged. Before: PSD encoding rejects the missing common projection. |
| O-005 (shared cause) | Clear live design-shape and corner metadata on path-operation results so projection cannot regenerate the source shape. | Boolean union produces curves whose stored and displayed paths/caches agree. Before: successful `path.boolean` retains `vector.live_shape`. |

O-006 was examined. Its early options-bar return and conversion-modal routing have a different cause from O-002; no O-006 behavior was changed. No other review findings were addressed.

## Validation

Rust/cargo 1.98.1; `RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`; every cargo build/test/check/clippy invocation uses `--offline`; `CARGO_BUILD_JOBS` is unset. This shell exposes the Fedora cargo/rustc directly, so plain `cargo` selects 1.98.1. `cargo +1.98.1` initially returned `error: no such command: +1.98.1` because this PATH does not expose the rustup proxy.

The initial harness setup was corrected to use the actual ArtText enum, the Pro theme's `Opacity:` label, and the engine's `path.boolean` ID. Focused layout/boolean counterfactuals then verified the actual pre-fix failures. Full command output is recorded below, including these failures.

The changed-module run passed all seven hotfix regression tests. Changed-module suites (including those new tests): engine 192 passed / 1 failed; UI 44 passed / 1 failed / 2 already ignored. Both failures also reproduce in the untouched implementation's pre-fix test binaries:

- `layer_multi_cmds::tests::merge_selected_into_top_most`: undo restores two selected layers where the test expects only one.
- `fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40`: `padding blend/16`.

They are outside the authorized findings and were left unchanged. The two ignored UI GPU milestones retain their existing ignore annotations. Existing engine transform GPU tests in the selected modules passed. The contact-sheet test regenerated `docs/design/vector-tool-icons.png`; that generated artifact was restored to its original tracked bytes.

Final gates:

- All **8 new regressions passed**: 5 engine, 2 UI, 1 native-service test.
- Selected IO suites: **99 passed**; native-service suite: **8 passed**, including the new production-save regression.
- `cargo check --offline --workspace`: **exit 0**. Existing untargeted xtask dead-field warning and missing CJK-font build notice are recorded in the output.
- Strict Clippy, all targets of all four touched crates, with `-D warnings`: **exit 0**.
- Format check and final `git diff --check`: **exit 0**, no output.
- No commits, GUI windows, dependency changes, build-job cap, or changes outside the requested hotfix scope.

## Exact validation commands and output

All commands were run from this worktree. Redirections retain stdout and stderr together; the following blocks reproduce the captured files verbatim. The first two runs document harness setup corrections, not successful gates. Focused pre-fix reproductions temporarily retain only the old implementation of the finding being tested; the final changed-module run includes all fixes.

### Initial rustup-style invocation

```bash
cargo +1.98.1 test --offline -p photocraft-engine -p photocraft-ui-egui --lib hotfix_ -- --nocapture > /tmp/codex-hotfix-critical-logs/before.log 2>&1
```

Exit code: `101`. Initial output (the log was subsequently replaced by the direct-cargo run below):

```text
error: no such command: `+1.98.1`

help: invoke `cargo` through `rustup` to handle `+toolchain` directives
```

### Initial compiler/harness setup

```bash
cargo test --offline -p photocraft-engine -p photocraft-ui-egui --lib hotfix_ -- --nocapture > /tmp/codex-hotfix-critical-logs/before.log 2>&1
```

Exit code: `101`.

<details>
<summary>Captured output</summary>

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
   Compiling crossbeam-epoch v0.9.21
   Compiling simd-adler32 v0.3.10
   Compiling pin-project-lite v0.2.17
   Compiling crossbeam-deque v0.8.8
   Compiling rayon-core v1.13.0
   Compiling adler2 v2.0.1
   Compiling shlex v2.0.1
   Compiling find-msvc-tools v0.1.14
   Compiling generic-array v0.14.7
   Compiling itoa v1.0.18
   Compiling typenum v1.20.1
   Compiling zmij v1.0.23
   Compiling cc v1.5.1
   Compiling num-traits v0.2.19
   Compiling libloading v0.8.9
   Compiling crc32fast v1.5.2
   Compiling syn v3.0.6
   Compiling syn v2.0.119
   Compiling subtle v2.6.1
   Compiling pkg-config v0.3.34
   Compiling miniz_oxide v0.9.1
   Compiling tracing-core v0.1.36
   Compiling rayon v1.12.0
   Compiling allocator-api2 v0.2.21
   Compiling serde_json v1.0.151
   Compiling flate2 v1.1.10
   Compiling hashbrown v0.16.1
   Compiling dlib v0.5.3
   Compiling hashbrown v0.17.1
   Compiling getrandom v0.4.3
   Compiling rand_core v0.10.1
   Compiling parking_lot_core v0.9.12
   Compiling num-integer v0.1.47
   Compiling miniz_oxide v0.8.9
   Compiling const-oid v0.9.6
   Compiling scopeguard v1.2.0
   Compiling lock_api v0.4.14
   Compiling stable_deref_trait v1.2.1
   Compiling winnow v1.0.4
   Compiling zune-core v0.5.3
   Compiling cfg_aliases v0.2.2
   Compiling anyhow v1.0.104
   Compiling fdeflate v0.3.7
   Compiling toml_datetime v1.1.1+spec-1.1.0
   Compiling quick-error v2.0.1
   Compiling spin v0.9.9
   Compiling siphasher v1.0.4
   Compiling slab v0.4.12
   Compiling wayland-sys v0.31.11
   Compiling zune-jpeg v0.5.15
   Compiling cpufeatures v0.2.17
   Compiling weezl v0.1.12
   Compiling pxfm v0.1.30
   Compiling memmap2 v0.9.11
   Compiling itertools v0.15.0
   Compiling photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-cms)
   Compiling toml_parser v1.1.3+spec-1.1.0
   Compiling indexmap v2.14.2
   Compiling byteorder-lite v0.1.0
   Compiling color_quant v1.1.0
   Compiling png v0.18.1
   Compiling gif v0.14.2
   Compiling image-webp v0.2.4
   Compiling lazy_static v1.5.0
   Compiling nom v8.0.0
   Compiling static_assertions v1.1.0
   Compiling uuid v1.26.1
   Compiling synstructure v0.14.0
   Compiling toml_edit v0.25.15+spec-1.1.0
   Compiling polling v3.11.0
   Compiling getrandom v0.2.17
   Compiling moxcms v0.8.1
   Compiling proc-macro-crate v3.5.0
   Compiling parking v2.2.1
   Compiling scoped-tls v1.0.1
   Compiling downcast-rs v1.2.1
   Compiling rand_core v0.6.4
   Compiling quick-xml v0.41.0
   Compiling futures-core v0.3.34
   Compiling getrandom v0.3.4
   Compiling unicode-width v0.2.2
   Compiling endi v1.1.1
   Compiling wayland-client v0.31.15
   Compiling linebender_resource_handle v0.1.1
   Compiling wayland-backend v0.3.17
   Compiling matrixmultiply v0.3.11
   Compiling raw-window-handle v0.6.2
   Compiling base64ct v1.8.3
   Compiling same-file v1.0.6
   Compiling fastrand v2.5.0
   Compiling pem-rfc7468 v0.7.0
   Compiling memo-map v0.3.4
   Compiling rawpointer v0.2.1
   Compiling walkdir v2.5.0
   Compiling fearless_simd v0.4.1
   Compiling typeid v1.0.3
   Compiling futures-io v0.3.34
   Compiling rustc-hash v1.1.0
   Compiling writeable v0.6.4
   Compiling nom-language v0.1.0
   Compiling event-listener v5.4.2
   Compiling naga v30.0.1
   Compiling concurrent-queue v2.5.0
   Compiling strength_reduce v0.2.4
   Compiling paste v1.0.15
   Compiling dyn-hash v1.0.0
   Compiling downcast-rs v2.0.2
   Compiling maplit v1.0.2
   Compiling ash v0.38.0+1.3.281
   Compiling erased-serde v0.4.10
   Compiling dyn-clone v1.0.20
   Compiling inventory v0.3.25
   Compiling bit-vec v0.9.1
   Compiling futures-lite v2.6.1
   Compiling transpose v0.2.3
   Compiling event-listener-strategy v0.5.4
   Compiling bit-set v0.10.0
   Compiling codespan-reporting v0.13.1
   Compiling num-bigint v0.4.8
   Compiling primal-check v0.3.4
   Compiling wayland-scanner v0.31.11
   Compiling spirv v0.4.0+sdk-1.4.341.0
   Compiling rustc-hash v2.1.3
   Compiling pastey v0.2.3
   Compiling litemap v0.8.3
   Compiling byteorder v1.5.0
   Compiling khronos-egl v6.0.0
   Compiling bit-vec v0.10.1
   Compiling bit-set v0.11.1
   Compiling wgpu-hal v30.0.1
   Compiling num-rational v0.4.2
   Compiling async-io v2.6.0
   Compiling itertools v0.14.0
   Compiling anymap3 v1.1.0
   Compiling presser v0.3.1
   Compiling cpufeatures v0.3.1
   Compiling alloc-no-stdlib v2.0.4
   Compiling alloc-stdlib v0.2.4
   Compiling accesskit v0.24.1
   Compiling xattr v1.6.1
   Compiling phf_shared v0.13.1
   Compiling ordered-float v5.5.0
   Compiling errno v0.3.14
   Compiling filetime v0.2.29
   Compiling utf8_iter v1.0.4
   Compiling glow v0.17.0
   Compiling hex v0.4.3
   Compiling bytes v1.12.1
   Compiling async-task v4.7.1
   Compiling renderdoc-sys v1.1.0
   Compiling phf_generator v0.13.1
   Compiling tar v0.4.46
   Compiling signal-hook-registry v1.4.8
   Compiling brotli-decompressor v5.0.3
   Compiling async-channel v2.5.0
   Compiling wgpu-core v30.0.1
   Compiling float-cmp v0.9.0
   Compiling ahash v0.8.12
   Compiling litrs v1.0.0
   Compiling atomic-waker v1.1.2
   Compiling icu_properties_data v2.3.0
   Compiling icu_locale_fallback_data v2.3.0
   Compiling percent-encoding v2.3.2
   Compiling piper v0.2.5
   Compiling async-signal v0.2.14
   Compiling strict-num v0.1.1
   Compiling chacha20 v0.10.2
   Compiling async-lock v3.4.2
   Compiling wgpu v30.0.1
   Compiling euclid v0.22.14
   Compiling thiserror v1.0.69
   Compiling icu_normalizer_data v2.3.0
   Compiling icu_segmenter_data v2.3.0
   Compiling rustix v0.38.44
   Compiling document-features v0.2.12
   Compiling arrayref v0.3.9
   Compiling guillotiere v0.7.0
   Compiling async-process v2.5.0
   Compiling rand v0.10.3
   Compiling blocking v1.7.0
   Compiling async-executor v1.14.0
   Compiling async-broadcast v0.7.2
   Compiling ordered-stream v0.2.0
   Compiling ff v0.13.1
   Compiling as-slice v0.2.1
   Compiling unicode-general-category v1.1.0
   Compiling av-scenechange v0.14.1
   Compiling cursor-icon v1.2.0
   Compiling linux-raw-sys v0.4.15
   Compiling xcursor v0.3.11
   Compiling semver v1.0.28
   Compiling xkeysym v0.2.1
   Compiling built v0.8.1
   Compiling base16ct v0.2.0
   Compiling aligned v0.4.3
   Compiling group v0.13.0
   Compiling rustc_version v0.4.1
   Compiling rand_distr v0.6.0
   Compiling no_std_io2 v0.9.4
   Compiling roxmltree v0.20.0
   Compiling y4m v0.8.0
   Compiling parlance v0.1.0
   Compiling opaque-debug v0.3.1
   Compiling float-ord v0.3.2
   Compiling web-time v1.1.0
   Compiling pastey v0.1.1
   Compiling base64 v0.22.1
   Compiling bitstream-io v4.10.0
   Compiling curve25519-dalek v4.1.3
   Compiling gethostname v1.1.0
   Compiling maybe-rayon v0.1.1
   Compiling x11-dl v2.21.0
   Compiling simd_helpers v0.1.0
   Compiling pulp v0.22.3
   Compiling quick-xml v0.38.4
   Compiling as-raw-xcb-connection v1.0.1
   Compiling httparse v1.10.1
   Compiling new_debug_unreachable v1.0.6
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-text)
   Compiling x11rb-protocol v0.13.2
   Compiling rav1e v0.8.1
   Compiling unicode-segmentation v1.13.3
   Compiling untrusted v0.9.0
   Compiling imgref v1.12.3
   Compiling nohash-hasher v0.2.0
   Compiling self_cell v1.3.0
   Compiling smithay-client-toolkit v0.19.2
   Compiling epaint_default_fonts v0.36.2
   Compiling noop_proc_macro v0.3.0
   Compiling loop9 v0.1.5
   Compiling winit v0.30.13
   Compiling aho-corasick v1.1.5
   Compiling raw-cpuid v11.6.0
   Compiling rustls v0.23.45
   Compiling num-bigint-dig v0.8.6
   Compiling reborrow v0.5.5
   Compiling jpeg-encoder v0.7.1
   Compiling regex-syntax v0.8.11
   Compiling pollster v1.0.1
   Compiling pulp-wasm-simd-flag v0.1.1
   Compiling lensfun v0.7.0
   Compiling xkbcommon-dl v0.4.2
   Compiling http v1.5.0
   Compiling accesskit_consumer v0.36.0
   Compiling rand_core v0.9.5
   Compiling blake3 v1.8.7
   Compiling moxcms v0.9.1
   Compiling jpeg-decoder v0.3.2
   Compiling regex-automata v0.4.18
   Compiling num-iter v0.1.46
   Compiling zune-inflate v0.2.54
   Compiling simplecss v0.2.2
   Compiling imagesize v0.13.0
   Compiling smithay-client-toolkit v0.20.0
   Compiling futures-task v0.3.34
   Compiling xmlwriter v0.1.0
   Compiling bit_field v0.10.3
   Compiling lebe v0.5.3
   Compiling hayro-ccitt v0.3.0
   Compiling data-url v0.3.2
   Compiling wasmi_ir v2.0.0
   Compiling dpi v0.1.2
   Compiling pico-args v0.5.0
   Compiling wasmparser v0.228.0
   Compiling ttf-parser v0.25.1
   Compiling smol_str v0.2.2
   Compiling bumpalo v3.20.3
   Compiling twox-hash v2.1.4
   Compiling regex v1.13.1
   Compiling hayro-jbig2 v0.3.0
   Compiling brotli v8.0.4
   Compiling hayro-jpeg2000 v0.3.5
   Compiling wasmi_core v2.0.0
   Compiling owned_ttf_parser v0.25.1
   Compiling jpeg-encoder v0.6.1
   Compiling constant_time_eq v0.4.2
   Compiling wasmi v2.0.0
   Compiling ryu v1.0.23
   Compiling ab_glyph_rasterizer v0.1.10
   Compiling zerocopy-derive v0.8.59
   Compiling zeroize_derive v1.5.0
   Compiling serde_derive v1.0.229
   Compiling bytemuck_derive v1.12.1
   Compiling thiserror-impl v2.0.21
   Compiling tracing-attributes v0.1.31
   Compiling zerofrom-derive v0.1.8
   Compiling yoke-derive v0.8.3
   Compiling profiling-procmacros v1.0.18
   Compiling fax_derive v0.2.0
   Compiling enumflags2_derive v0.7.12
   Compiling zerovec-derive v0.11.6
   Compiling displaydoc v0.2.7
   Compiling derive-new v0.7.0
   Compiling equator-macro v0.4.2
   Compiling phf_macros v0.13.1
   Compiling zerofrom v0.1.8
   Compiling zvariant_utils v4.2.0
   Compiling smallvec v1.16.2
   Compiling arrayvec v0.7.8
   Compiling tracing v0.1.44
   Compiling half v2.7.1
   Compiling photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-geom)
   Compiling photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-color)
   Compiling polycool v0.4.0
   Compiling zeroize v1.9.1
   Compiling photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-raster)
   Compiling profiling v1.0.18
   Compiling kurbo v0.13.1
   Compiling crypto-common v0.1.7
   Compiling block-buffer v0.10.4
   Compiling der v0.7.10
   Compiling minijinja v2.24.0
   Compiling jxl-bitstream v1.1.0
   Compiling naga-types v30.0.1
   Compiling lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-geom)
   Compiling digest v0.10.7
   Compiling string-interner v0.20.0
   Compiling zcheapstr v1.1.0
   Compiling parking_lot v0.12.5
   Compiling photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-doc)
   Compiling enumflags2 v0.7.12
   Compiling wayland-protocols v0.32.13
   Compiling jxl-oxide-common v1.0.0
   Compiling jxl-grid v0.6.2
   Compiling lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-color)
   Compiling jxl-coding v1.0.1
   Compiling spki v0.7.3
   Compiling zvariant_derive v5.15.0
   Compiling jxl-threadpool v1.0.0
   Compiling pkcs5 v0.7.1
   Compiling gpu-allocator v0.28.0
   Compiling equator v0.4.2
   Compiling sha2 v0.10.9
   Compiling pkcs8 v0.10.2
   Compiling jxl-modular v0.11.3
   Compiling jxl-image v0.13.0
   Compiling lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-raster)
   Compiling hmac v0.12.1
   Compiling ppv-lite86 v0.2.21
   Compiling signature v2.2.0
   Compiling inout v0.1.4
   Compiling phf v0.13.1
   Compiling fax v0.2.6
   Compiling aligned-vec v0.6.4
   Compiling async-trait v0.1.92
   Compiling jxl-vardct v0.11.1
   Compiling tiff v0.11.3
   Compiling photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vector)
   Compiling zvariant v5.15.0
   Compiling v_frame v0.3.9
   Compiling serde_repr v0.1.21
   Compiling yoke v0.8.3
   Compiling async-recursion v1.1.1
   Compiling thiserror-impl v1.0.69
   Compiling zerovec v0.11.8
   Compiling bytemuck v1.25.2
   Compiling zbus_names v4.3.4
   Compiling tinystr v0.8.4
   Compiling num-complex v0.4.6
   Compiling qoi v0.4.1
   Compiling image v0.25.10
   Compiling font-types v0.12.5
   Compiling ndarray v0.17.2
   Compiling icu_locale_core v2.3.0
   Compiling rustfft v6.4.1
   Compiling read-fonts v0.41.0
   Compiling wgpu-types v30.0.1
   Compiling potential_utf v0.1.6
   Compiling zerotrie v0.2.5
   Compiling icu_collections v2.3.0
   Compiling zbus_xml v5.2.1
   Compiling icu_provider v2.3.1
   Compiling tract-data v0.23.8
   Compiling color v0.3.3
   Compiling wgpu-naga-bridge v30.0.1
   Compiling skrifa v0.44.0
   Compiling harfrust v0.12.0
   Compiling peniko v0.6.1
   Compiling zbus-lockstep v0.5.2
   Compiling zbus_macros v5.19.0
   Compiling zbus-lockstep-macros v0.5.2
   Compiling vello_common v0.1.0
   Compiling icu_locale_fallback v2.3.0
   Compiling icu_properties v2.3.0
   Compiling rgb v0.8.53
   Compiling emath v0.36.2
   Compiling font-types v0.11.3
   Compiling cipher v0.4.4
   Compiling jxl-frame v0.13.3
   Compiling wayland-protocols-wlr v0.3.12
   Compiling hkdf v0.12.4
   Compiling sec1 v0.7.3
   Compiling wayland-cursor v0.31.14
   Compiling wgpu-core-deps-windows-linux-android v30.0.1
   Compiling wayland-csd-frame v0.3.0
   Compiling universal-hash v0.5.1
   Compiling crypto-bigint v0.5.5
   Compiling rustls-pki-types v1.15.1
   Compiling arg_enum_proc_macro v0.3.4
   Compiling prost-derive v0.14.4
   Compiling elliptic-curve v0.13.8
   Compiling tract-linalg v0.23.8
   Compiling ecolor v0.36.2
   Compiling read-fonts v0.39.2
   Compiling calloop v0.13.0
   Compiling parley_data v0.11.1
   Compiling vello_cpu v0.1.0
   Compiling icu_segmenter v2.3.0
   Compiling icu_normalizer v2.3.0
   Compiling fontique v0.11.1
   Compiling av1-grain v0.2.5
   Compiling jxl-color v0.11.0
   Compiling lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-tiff)
   Compiling num-derive v0.4.2
   Compiling parley v0.11.1
   Compiling prost v0.14.4
   Compiling lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-meta)
   Compiling jxl-render v0.12.4
   Compiling epaint v0.36.2
   Compiling zbus v5.19.0
   Compiling calloop-wayland-source v0.3.0
   Compiling jxl-jbr v0.2.1
   Compiling tiny-skia-path v0.11.4
   Compiling rand_chacha v0.3.1
   Compiling rfc6979 v0.4.0
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-develop)
   Compiling kurbo v0.11.3
   Compiling photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-psd)
   Compiling avif-serialize v0.8.9
   Compiling tract-core v0.23.8
   Compiling atspi-common v0.13.0
   Compiling x11rb v0.13.2
   Compiling curve25519-dalek-derive v0.1.1
   Compiling ravif v0.13.0
   Compiling svgtypes v0.15.3
   Compiling atspi-proxies v0.13.0
   Compiling jxl-oxide v0.12.6
   Compiling ecdsa v0.16.9
   Compiling rand v0.8.8
   Compiling egui v0.36.2
   Compiling skrifa v0.42.1
   Compiling primeorder v0.13.6
   Compiling rustls-webpki v0.103.15
   Compiling polyval v0.6.2
   Compiling photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-compose)
   Compiling wayland-protocols-plasma v0.3.12
   Compiling linesweeper v0.4.0
   Compiling aead v0.5.2
   Compiling calloop v0.14.4
   Compiling futures-macro v0.3.34
   Compiling calloop-wayland-source v0.4.1
   Compiling photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pathops)
   Compiling exr v1.74.2
   Compiling lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-codecs)
   Compiling ghash v0.5.1
   Compiling tract-nnef v0.23.8
   Compiling tract-hir v0.23.8
   Compiling futures-util v0.3.34
   Compiling atspi v0.29.0
   Compiling usvg v0.45.1
   Compiling accesskit_atspi_common v0.18.1
   Compiling tract-extra v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-pipeline)
   Compiling lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-raw)
   Compiling write-fonts v0.48.1
   Compiling tract-onnx-opl v0.23.8
   Compiling poly1305 v0.8.0
   Compiling aes v0.8.4
   Compiling ctr v0.9.2
   Compiling chacha20 v0.9.1
   Compiling wayland-protocols-experimental v20250721.0.1
   Compiling wayland-protocols-misc v0.3.12
   Compiling lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-scenes)
   Compiling ed25519 v2.2.3
   Compiling tract-onnx v0.23.8
   Compiling rand_chacha v0.9.0
   Compiling pkcs1 v0.7.5
   Compiling ruzstd v0.9.0
   Compiling option-ext v0.2.0
   Compiling base64 v0.23.1
   Compiling hayro-postscript v0.1.0
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-engine)
   Compiling aes-gcm v0.10.3
   Compiling hayro-cmap v0.1.0
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling ureq-proto v0.6.4
   Compiling dirs-sys v0.5.0
   Compiling rsa v0.9.10
   Compiling rand v0.9.5
   Compiling lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-merge)
   Compiling ed25519-dalek v2.2.0
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-seg)
   Compiling chacha20poly1305 v0.10.1
   Compiling subsetter v0.2.6
   Compiling accesskit_unix v0.21.1
   Compiling photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-codecs)
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-gpu)
   Compiling p384 v0.13.1
   Compiling p256 v0.13.2
   Compiling x25519-dalek v2.0.1
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-catalog)
   Compiling webpki-roots v1.0.9
   Compiling rustls-webpki v0.102.8
   Compiling tiny-skia-path v0.12.0
   Compiling lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-preview)
   Compiling photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-algo)
   Compiling sha1 v0.10.7
   Compiling hayro-syntax v0.7.2
   Compiling ab_glyph v0.2.32
   Compiling pdf-writer v0.15.0
   Compiling imagesize v0.14.0
   Compiling chunked_transfer v1.5.0
   Compiling data-encoding v2.11.1
   Compiling utf8-zero v0.8.1
   Compiling bitflags v1.3.2
   Compiling utf-8 v0.7.6
   Compiling httpdate v1.0.3
   Compiling wasmi_collections v2.0.0
   Compiling ascii v1.1.0
   Compiling xmp-writer v0.3.3
   Compiling tiny_http v0.12.0
   Compiling tungstenite v0.27.0
   Compiling png v0.17.16
   Compiling krilla v0.8.2
   Compiling ureq v3.4.2
   Compiling photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-format)
   Compiling hayro-interpret v0.7.0
   Compiling rustls-rustcrypto v0.0.2-alpha
   Compiling accesskit_winit v0.32.2
   Compiling smithay-clipboard v0.7.3
   Compiling dirs v6.0.0
   Compiling arboard v3.6.1
   Compiling photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-paint)
   Compiling type-map v0.5.1
   Compiling photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-raw)
   Compiling serde_spanned v1.1.1
   Compiling is-docker v0.2.0
   Compiling toml_writer v1.1.2+spec-1.1.0
   Compiling leb128fmt v0.1.0
   Compiling toml v1.1.6+spec-1.1.0
   Compiling wasm-encoder v0.261.0
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-io)
   Compiling is-wsl v0.4.0
   Compiling egui-wgpu v0.36.2
   Compiling egui-winit v0.36.2
   Compiling photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-plugins)
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-ai)
   Compiling photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pdf)
   Compiling tiny-skia v0.11.4
   Compiling photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-affinity)
   Compiling photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-svg)
   Compiling photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vecfx)
   Compiling photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ops)
   Compiling pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-trace)
   Compiling li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-attributions)
   Compiling enum-map-derive v0.17.0
   Compiling accesskit_consumer v0.35.0
   Compiling getopts v0.2.24
   Compiling colored v2.2.0
   Compiling rfd v0.17.2
   Compiling dify v0.8.0
   Compiling kittest v0.4.0
   Compiling resvg v0.45.1
   Compiling eframe v0.36.2
   Compiling enum-map v2.7.3
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ui-egui)
   Compiling open v5.4.4
   Compiling wast v261.0.0
   Compiling photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-gpu)
   Compiling tempfile v3.27.0
   Compiling memory-stats v1.2.0
   Compiling pollster v0.4.0
   Compiling egui_extras v0.36.2
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-engine)
   Compiling egui_kittest v0.36.2
   Compiling wat v1.261.0
   Compiling sys-locale v0.3.2
   Compiling photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-testkit)
warning: unused import: `photocraft_geom::Affine`
 --> crates/pc-engine/src/hotfix_critical_tests.rs:6:5
  |
6 | use photocraft_geom::Affine;
  |     ^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

error[E0599]: no variant, associated function, or constant named `ArtisticText` found for enum `state::Tool` in the current scope
  --> crates/pc-ui-egui/src/hotfix_critical_tests.rs:43:62
   |
43 |             let tool = if persona == Persona::Vector { Tool::ArtisticText } else { Tool::Type };
   |                                                              ^^^^^^^^^^^^ variant, associated function, or constant not found in `state::Tool`
   |
  ::: crates/pc-ui-egui/src/state.rs:64:1
   |
64 | pub enum Tool {
   | ------------- variant, associated function, or constant `ArtisticText` not found for this enum

For more information about this error, try `rustc --explain E0599`.
error: could not compile `photocraft-ui-egui` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
warning: `photocraft-engine` (lib test) generated 1 warning (run `cargo fix --lib -p photocraft-engine --tests` to apply 1 suggestion)

```

</details>

### Initial baseline reproduction (followed by corrected label/command queries)

```bash
cargo test --offline -p photocraft-engine -p photocraft-ui-egui --lib hotfix_ --no-fail-fast -- --nocapture > /tmp/codex-hotfix-critical-logs/before-repro.log 2>&1
```

Exit code: `101`.

<details>
<summary>Captured output</summary>

```text
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-engine)
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 56.55s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-0962a62fc0794aad)

running 4 tests

thread 'hotfix_critical_tests::hotfix_repeat_anchor_and_clip_follow_layer_and_canvas_transforms' (1986410) panicked at crates/pc-engine/src/hotfix_critical_tests.rs:118:9:
assertion `left == right` failed: layer.translate
  left: [30.0, 30.0]
 right: [40.0, 50.0]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test hotfix_critical_tests::hotfix_repeat_anchor_and_clip_follow_layer_and_canvas_transforms ... FAILED

thread 'hotfix_critical_tests::hotfix_live_shapes_follow_move_transform_and_canvas_geometry' (1986409) panicked at crates/pc-engine/src/hotfix_critical_tests.rs:26:5:
assertion `left == right` failed
  left: Some((40.0, 30.0, 70.0, 50.0))
 right: Some((10.0, 10.0, 40.0, 30.0))
test hotfix_critical_tests::hotfix_live_shapes_follow_move_transform_and_canvas_geometry ... FAILED

thread 'hotfix_critical_tests::hotfix_boolean_results_do_not_regenerate_the_source_live_shape' (1986407) panicked at crates/pc-engine/src/hotfix_critical_tests.rs:60:48:
called `Result::unwrap()` on an `Err` value: UnknownCommand("layer.geometry.add")
test hotfix_critical_tests::hotfix_boolean_results_do_not_regenerate_the_source_live_shape ... FAILED

thread 'hotfix_critical_tests::hotfix_live_and_clipped_vectors_save_as_psd_psb_and_layered_tiff' (1986408) panicked at crates/pc-engine/src/hotfix_critical_tests.rs:92:134:
called `Result::unwrap()` on an `Err` value: Other("psd: unsupported: PSD live/crop export requires the common feature projection (18–20 / vector crop)")
test hotfix_critical_tests::hotfix_live_and_clipped_vectors_save_as_psd_psb_and_layered_tiff ... FAILED

failures:

failures:
    hotfix_critical_tests::hotfix_boolean_results_do_not_regenerate_the_source_live_shape
    hotfix_critical_tests::hotfix_live_and_clipped_vectors_save_as_psd_psb_and_layered_tiff
    hotfix_critical_tests::hotfix_live_shapes_follow_move_transform_and_canvas_geometry
    hotfix_critical_tests::hotfix_repeat_anchor_and_clip_follow_layer_and_canvas_transforms

test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 1314 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p photocraft-engine --lib`
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc)

running 2 tests

thread 'hotfix_critical_tests::hotfix_options_bar_controls_are_inside_the_visible_row' (1986436) panicked at crates/pc-ui-egui/src/hotfix_critical_tests.rs:30:29:
No nodes found matching the query:
By {
    label: "Opacity",
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
                                        #9751865662918377093,
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
                                                #841487379939941194,
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
                                                        #18397422437217158121,
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
                                                                #517301083259484822,
                                                            ),
                                                            role: Unknown,
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
                                                                #4062584291299164286,
                                                            ),
                                                            role: Button,
                                                            label: "Tool presets",
                                                            focused: false,
                                                            hidden: false,
                                                            disabled: false,
                                                            toggled: False,
                                                            children: [],
                                                        },
                                                        Node {
                                                            id: NodeId(
                                                                TreeIndex(
                                                                    0,
                                                                ),
                                                                #17328386275585824631,
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
                                                                        #747027722713447098,
                                                                    ),
                                                                    role: GenericContainer,
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
                                                                #5898169287711684299,
                                                            ),
                                                            role: Button,
                                                            label: "Brush Preset picker",
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
                                                                #5154475764976904100,
                                                            ),
                                                            role: Button,
                                                            label: "Toggle the Brush Settings panel",
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
                                                                #4309854821472234898,
                                                            ),
                                                            role: Label,
                                                            value: "Mode:",
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
                                                                #14406169462850846312,
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
                                                                        #1311990202573493058,
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
                                                                                #9400519685189450937,
                                                                            ),
                                                                            role: ComboBox,
                                                                            value: "Normal",
                                                                            focused: false,
                                                                            hidden: false,
                                                                            disabled: false,
                                                                            children: [],
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
                                                                #5143309952087608463,
                                                            ),
                                                            role: Label,
                                                            value: "Opacity:",
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
                                                                #13519517626445154864,
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
                                                                        #6476449482583073706,
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
                                                                                #2369007452966573629,
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
                                                                                        #16445406534483633713,
                                                                                    ),
                                                                                    role: SpinButton,
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
                                                                #9461388693161830321,
                                                            ),
                                                            role: Unknown,
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
                                                                #9485591590953990401,
                                                            ),
                                                            role: Label,
                                                            value: "Flow:",
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
                                                                #5735398422834200756,
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
                                                                        #7055281644616784208,
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
                                                                                #5869294659993841258,
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
                                                                                        #2772926586352248597,
                                                                                    ),
                                                                                    role: SpinButton,
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
                                                                #10133586825152412551,
                                                            ),
                                                            role: Unknown,
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
                                                                #15635237664835389230,
                                                            ),
                                                            role: Label,
                                                            value: "Smoothing:",
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
                                                                #380663901346050376,
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
                                                                        #3019058989671751627,
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
                                                                                #10619535757587773295,
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
                                                                                        #10295600741099804184,
                                                                                    ),
                                                                                    role: SpinButton,
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
                                                                #9806071796399887084,
                                                            ),
                                                            role: Unknown,
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
                                                                #16158184484639542281,
                                                            ),
                                                            role: Unknown,
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
                                                                #6722662782222289693,
                                                            ),
                                                            role: Unknown,
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
}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test hotfix_critical_tests::hotfix_options_bar_controls_are_inside_the_visible_row ... FAILED

thread 'hotfix_critical_tests::hotfix_inline_vector_text_owns_tool_and_paint_shortcut_letters' (1986435) panicked at crates/pc-ui-egui/src/hotfix_critical_tests.rs:53:17:
assertion `left == right` failed: Vector/Illustrator: O
  left: Reflect
 right: ArtText
test hotfix_critical_tests::hotfix_inline_vector_text_owns_tool_and_paint_shortcut_letters ... FAILED

failures:

failures:
    hotfix_critical_tests::hotfix_inline_vector_text_owns_tool_and_paint_shortcut_letters
    hotfix_critical_tests::hotfix_options_bar_controls_are_inside_the_visible_row

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 1323 filtered out; finished in 0.04s

error: test failed, to rerun pass `-p photocraft-ui-egui --lib`
error: 2 targets failed:
    `-p photocraft-engine --lib`
    `-p photocraft-ui-egui --lib`

```

</details>

### O-005 pre-fix reproduction with the corrected engine command

```bash
cargo test --offline -p photocraft-engine --lib hotfix_boolean -- --nocapture > /tmp/codex-hotfix-critical-logs/before-boolean.log 2>&1
```

Exit code: `101`.

<details>
<summary>Captured output</summary>

```text
   Compiling syn v3.0.6
   Compiling syn v2.0.119
   Compiling serde_core v1.0.229
   Compiling serde v1.0.229
   Compiling tracing v0.1.44
   Compiling rustix v1.1.5
   Compiling linux-raw-sys v0.12.1
   Compiling naga v30.0.1
   Compiling wgpu-hal v30.0.1
   Compiling wgpu-core v30.0.1
   Compiling linebender_resource_handle v0.1.1
   Compiling wgpu v30.0.1
   Compiling rgb v0.8.53
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling uuid v1.26.1
   Compiling jxl-bitstream v1.1.0
   Compiling jxl-grid v0.6.2
   Compiling jxl-threadpool v1.0.0
   Compiling jxl-oxide-common v1.0.0
   Compiling jxl-coding v1.0.1
   Compiling jxl-image v0.13.0
   Compiling jxl-modular v0.11.3
   Compiling jxl-color v0.11.0
   Compiling jxl-vardct v0.11.1
   Compiling synstructure v0.14.0
   Compiling xattr v1.6.1
   Compiling jxl-frame v0.13.3
   Compiling tar v0.4.46
   Compiling minijinja v2.24.0
   Compiling jxl-jbr v0.2.1
   Compiling serde_derive v1.0.229
   Compiling bytemuck_derive v1.12.1
   Compiling thiserror-impl v2.0.21
   Compiling zerovec-derive v0.11.6
   Compiling zerofrom-derive v0.1.8
   Compiling yoke-derive v0.8.3
   Compiling displaydoc v0.2.7
   Compiling bytemuck v1.25.2
   Compiling zerofrom v0.1.8
   Compiling num-complex v0.4.6
   Compiling qoi v0.4.1
   Compiling yoke v0.8.3
   Compiling zerovec v0.11.8
   Compiling zerotrie v0.2.5
   Compiling font-types v0.12.5
   Compiling ndarray v0.17.2
   Compiling rustfft v6.4.1
   Compiling thiserror v2.0.21
   Compiling naga-types v30.0.1
   Compiling read-fonts v0.41.0
   Compiling zeroize_derive v1.5.0
   Compiling zerocopy-derive v0.8.59
   Compiling fax_derive v0.2.0
   Compiling derive-new v0.7.0
   Compiling profiling-procmacros v1.0.18
   Compiling equator-macro v0.4.2
   Compiling wgpu-types v30.0.1
   Compiling gpu-allocator v0.28.0
   Compiling tinystr v0.8.4
   Compiling icu_locale_core v2.3.0
   Compiling potential_utf v0.1.6
   Compiling smallvec v1.16.2
   Compiling arrayvec v0.7.8
   Compiling photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-color)
   Compiling parking_lot_core v0.9.12
   Compiling photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-geom)
   Compiling parking_lot v0.12.5
   Compiling photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-raster)
   Compiling polycool v0.4.0
   Compiling lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-geom)
   Compiling string-interner v0.20.0
   Compiling kurbo v0.13.1
   Compiling photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-doc)
   Compiling lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-color)
   Compiling profiling v1.0.18
   Compiling icu_provider v2.3.1
   Compiling icu_collections v2.3.0
   Compiling arg_enum_proc_macro v0.3.4
   Compiling fax v0.2.6
   Compiling lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-raster)
   Compiling icu_locale_fallback v2.3.0
   Compiling icu_properties v2.3.0
   Compiling prost-derive v0.14.4
   Compiling font-types v0.11.3
   Compiling equator v0.4.2
   Compiling read-fonts v0.39.2
   Compiling icu_segmenter v2.3.0
   Compiling aligned-vec v0.6.4
   Compiling parley_data v0.11.1
   Compiling v_frame v0.3.9
   Compiling icu_normalizer v2.3.0
   Compiling zeroize v1.9.1
   Compiling av1-grain v0.2.5
   Compiling lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-tiff)
   Compiling generic-array v0.14.7
   Compiling der v0.7.10
   Compiling rustls-pki-types v1.15.1
   Compiling harfrust v0.12.0
   Compiling fontique v0.11.1
   Compiling crypto-common v0.1.7
   Compiling block-buffer v0.10.4
   Compiling inout v0.1.4
   Compiling digest v0.10.7
   Compiling cipher v0.4.4
   Compiling sha2 v0.10.9
   Compiling hmac v0.12.1
   Compiling signature v2.2.0
   Compiling universal-hash v0.5.1
   Compiling hkdf v0.12.4
   Compiling spki v0.7.3
   Compiling crypto-bigint v0.5.5
   Compiling skrifa v0.44.0
   Compiling pkcs5 v0.7.1
   Compiling pkcs8 v0.10.2
   Compiling av-scenechange v0.14.1
   Compiling sec1 v0.7.3
   Compiling num-derive v0.4.2
   Compiling photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vector)
   Compiling elliptic-curve v0.13.8
   Compiling rfc6979 v0.4.0
   Compiling lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-meta)
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-develop)
   Compiling avif-serialize v0.8.9
   Compiling curve25519-dalek-derive v0.1.1
   Compiling photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-psd)
   Compiling parley v0.11.1
   Compiling jxl-render v0.12.4
   Compiling rav1e v0.8.1
   Compiling jxl-oxide v0.12.6
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-text)
   Compiling prost v0.14.4
   Compiling curve25519-dalek v4.1.3
   Compiling zerocopy v0.8.59
   Compiling ravif v0.13.0
   Compiling skrifa v0.42.1
   Compiling photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-compose)
   Compiling ecdsa v0.16.9
   Compiling primeorder v0.13.6
   Compiling polyval v0.6.2
   Compiling aead v0.5.2
   Compiling half v2.7.1
   Compiling ppv-lite86 v0.2.21
   Compiling tiff v0.11.3
   Compiling tract-data v0.23.8
   Compiling tract-linalg v0.23.8
   Compiling rand_chacha v0.3.1
   Compiling rustls-webpki v0.103.15
   Compiling rand v0.8.8
   Compiling image v0.25.10
   Compiling linesweeper v0.4.0
   Compiling pulp v0.22.3
   Compiling rustls v0.23.45
   Compiling num-bigint-dig v0.8.6
   Compiling lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-codecs)
   Compiling exr v1.74.2
   Compiling photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pathops)
   Compiling tract-core v0.23.8
   Compiling wgpu-naga-bridge v30.0.1
   Compiling rand_chacha v0.9.0
   Compiling ghash v0.5.1
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-pipeline)
   Compiling write-fonts v0.48.1
   Compiling wgpu-core-deps-windows-linux-android v30.0.1
   Compiling lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-raw)
   Compiling pkcs1 v0.7.5
   Compiling ed25519 v2.2.3
   Compiling poly1305 v0.8.0
   Compiling aes v0.8.4
   Compiling ctr v0.9.2
   Compiling chacha20 v0.9.1
   Compiling lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-scenes)
   Compiling kurbo v0.11.3
   Compiling phf_macros v0.13.1
   Compiling svgtypes v0.15.3
   Compiling tract-nnef v0.23.8
   Compiling tract-hir v0.23.8
   Compiling lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-merge)
   Compiling chacha20poly1305 v0.10.1
   Compiling aes-gcm v0.10.3
   Compiling ed25519-dalek v2.2.0
   Compiling rsa v0.9.10
   Compiling tract-extra v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling tract-onnx-opl v0.23.8
   Compiling phf v0.13.1
   Compiling subsetter v0.2.6
   Compiling rand v0.9.5
   Compiling photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-codecs)
   Compiling tract-onnx v0.23.8
   Compiling p256 v0.13.2
   Compiling p384 v0.13.1
   Compiling x25519-dalek v2.0.1
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-catalog)
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-seg)
   Compiling sha1 v0.10.7
   Compiling rustls-webpki v0.102.8
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-gpu)
   Compiling webpki-roots v1.0.9
   Compiling lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-preview)
   Compiling photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-algo)
   Compiling blake3 v1.8.7
   Compiling hayro-syntax v0.7.2
   Compiling lensfun v0.7.0
   Compiling tiny-skia-path v0.12.0
   Compiling tiny-skia-path v0.11.4
   Compiling hayro-interpret v0.7.0
   Compiling usvg v0.45.1
   Compiling krilla v0.8.2
   Compiling photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-format)
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-engine)
   Compiling ureq v3.4.2
   Compiling rustls-rustcrypto v0.0.2-alpha
   Compiling tungstenite v0.27.0
   Compiling photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-paint)
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-ai)
   Compiling photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pdf)
   Compiling photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-svg)
   Compiling photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-gpu)
   Compiling photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vecfx)
   Compiling photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-affinity)
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-io)
   Compiling pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-trace)
   Compiling photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ops)
   Compiling photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-plugins)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-engine)
    Finished `test` profile [optimized + debuginfo] target(s) in 3m 12s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-32901a71184913f6)

running 1 test

thread 'hotfix_critical_tests::hotfix_boolean_results_do_not_regenerate_the_source_live_shape' (2022832) panicked at crates/pc-engine/src/hotfix_critical_tests.rs:70:5:
assertion failed: s.active().unwrap().doc.layer(a).unwrap().vector.live_shape.is_none()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test hotfix_critical_tests::hotfix_boolean_results_do_not_regenerate_the_source_live_shape ... FAILED

failures:

failures:
    hotfix_critical_tests::hotfix_boolean_results_do_not_regenerate_the_source_live_shape

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1316 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p photocraft-engine --lib`

```

</details>

### O-001 pre-fix reproduction with the corrected Pro label

```bash
cargo test --offline -p photocraft-ui-egui --lib hotfix_options_bar -- --nocapture > /tmp/codex-hotfix-critical-logs/before-options.log 2>&1
```

Exit code: `101`.

<details>
<summary>Captured output</summary>

```text
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-raw)
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-io)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-engine)
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 1m 44s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-219d99becdc1f52f)

running 1 test

thread 'hotfix_critical_tests::hotfix_options_bar_controls_are_inside_the_visible_row' (2028498) panicked at crates/pc-ui-egui/src/hotfix_critical_tests.rs:38:13:
Pixel/ProMedium: [[1638.8 18.0] - [1686.3 33.0]]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test hotfix_critical_tests::hotfix_options_bar_controls_are_inside_the_visible_row ... FAILED

failures:

failures:
    hotfix_critical_tests::hotfix_options_bar_controls_are_inside_the_visible_row

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1324 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p photocraft-ui-egui --lib`

```

</details>

### Formatting

```bash
cargo fmt --all > /tmp/codex-hotfix-critical-logs/fmt-final.log 2>&1
```

Exit code: `0`.

No output.

### New regressions plus the existing changed-module tests

```bash
cargo test --offline -p photocraft-engine -p photocraft-ui-egui --lib --no-fail-fast -- hotfix_ artboard_cmds:: canvas_geom:: commands:: comps_cmds:: file_cmds:: image_cmds:: layer_copy_cmds:: layer_multi_cmds:: mode_cmds:: pathops_cmds:: transform_cmds:: vector_cmds:: vector_integration:: vector_packages::transform_tools:: vector_packages::layout:: panels:: fill_stroke_proxy:: brush_panel::tests:: > /tmp/codex-hotfix-critical-logs/changed-modules.log 2>&1
```

Exit code: `101`.

<details>
<summary>Captured output</summary>

```text
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-io)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-engine)
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 55.40s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-0962a62fc0794aad)

running 193 tests
test artboard_cmds::tests::raster_pdf_xref_offsets_point_at_objects ... ok
test canvas_geom::tests::gradient_fill_angle_follows_the_canvas ... ok
test commands::registration_tests::integrated_command_ids_are_unique_and_findable ... ok
test artboard_cmds::tests::new_artboards_place_and_grow_canvas ... ok
test artboard_cmds::tests::clear_artboard_guides_only_inside ... ok
test artboard_cmds::tests::disabled_without_artboards ... ok
test comps_cmds::tests::disabled_states_and_bad_params ... ok
test comps_cmds::tests::manage_comps_and_cycle ... ok
test artboard_cmds::tests::export_to_files_and_pdf ... ok
test comps_cmds::tests::out_of_range_comp_ids_are_rejected_not_wrapped ... ok
test comps_cmds::tests::undo_redo_and_last_document_state ... ok
test comps_cmds::tests::warnings_for_deleted_layers ... ok
test comps_cmds::tests::export_comps_to_files_naming ... ok
test comps_cmds::tests::capture_apply_visibility_position_appearance_independently ... ok
test file_cmds::tests::conditional_mode_change_cannot_run_a_mode_change_the_gate_denies ... ok
test artboard_cmds::tests::from_layers_and_group_then_compose_clips ... ok
test file_cmds::tests::camera_raw_opens_as_16_bit_with_notes ... ok
test file_cmds::tests::file_info_round_trips_through_xmp_and_psd ... ok
test file_cmds::tests::only_layered_files_save_in_place ... ok
test file_cmds::tests::color_lookup_table_bakes_the_adjustment_stack ... ok
test file_cmds::tests::guide_layouts ... ok
test file_cmds::tests::close_all_and_close_others ... ok
test file_cmds::tests::layers_to_files_writes_each_visible_layer ... ok
test file_cmds::tests::open_as_forces_the_decoder ... ok
test canvas_geom::tests::turns_on_empty_and_tiny_documents_never_panic ... ok
test file_cmds::tests::templates_open_untitled_without_their_path ... ok
test file_cmds::tests::save_a_copy_keeps_path_and_dirty_state ... ok
test hotfix_critical_tests::hotfix_boolean_results_do_not_regenerate_the_source_live_shape ... ok
test file_cmds::tests::rotated_jpegs_open_and_place_upright ... ok
test file_cmds::tests::revert_reloads_as_one_undoable_step ... ok
test hotfix_critical_tests::hotfix_repeat_anchor_and_clip_follow_layer_and_canvas_transforms ... ok
test canvas_geom::tests::image_size_scales_vector_geometry ... ok
test file_cmds::tests::load_files_into_stack_and_image_processor_and_batch ... ok
test file_cmds::tests::place_embedded_centres_fits_and_embeds ... ok
test canvas_geom::tests::canvas_size_and_crop_move_vector_geometry ... ok
test image_cmds::tests::canvas_size_anchors_and_extends_background ... ok
test image_cmds::tests::crop_rejects_rectangles_that_would_wrap ... ok
test canvas_geom::tests::arbitrary_rotation_moves_marks_and_unlinked_vector_masks ... ok
test canvas_geom::tests::turns_map_guides_paths_slices_and_live_shapes ... ok
test image_cmds::tests::duplicate_document ... ok
test image_cmds::tests::resolution_only_change ... ok
test image_cmds::tests::crop_to_explicit_rect_can_extend_canvas ... ok
test image_cmds::tests::crop_to_selection_and_trim ... ok
test file_cmds::tests::fit_image_and_conditional_mode_change ... ok
test hotfix_critical_tests::hotfix_placed_svg_saves_with_the_original_editability_notice ... ok
test image_cmds::tests::trim_based_on_top_left_colour ... ok
test layer_copy_cmds::tests::a_copied_background_is_an_ordinary_layer_and_several_layers_keep_their_order ... ok
test layer_copy_cmds::tests::bad_params_and_states_fail_without_changing_anything ... ok
test layer_copy_cmds::tests::copies_the_selected_layers_above_the_destinations_active_layer_in_one_step ... ok
test layer_copy_cmds::tests::places_the_copies_centred_on_the_canvas_on_a_point_or_offset ... ok
test layer_copy_cmds::tests::the_sources_patterns_come_along_once ... ok
test layer_multi_cmds::tests::a_locked_group_keeps_a_grouped_background_locked ... ok
test image_cmds::tests::straightened_crop_comes_out_upright_and_undoes ... ok
test layer_multi_cmds::tests::a_locked_group_locks_its_contents ... ok
test layer_multi_cmds::tests::align_single_layer_to_canvas ... ok
test file_cmds::tests::batch_reports_inputs_that_share_an_output_name ... ok
test layer_multi_cmds::tests::group_preview_preserves_shape_cache_translation_identity ... ok
test layer_multi_cmds::tests::find_layers_by_name ... ok
test layer_multi_cmds::tests::duplicate_hide_lock_and_reverse_selected ... ok
test layer_multi_cmds::tests::group_from_layers_preserves_order ... ok
test image_cmds::tests::image_size_scales_everything ... ok
test layer_multi_cmds::tests::inspect_reports_selection ... ok
test canvas_geom::tests::right_angle_turns_move_vectors_type_and_smart_objects ... ok
test layer_multi_cmds::tests::align_single_layer_to_selection ... ok
test layer_multi_cmds::tests::merge_selected_into_top_most ... FAILED
test layer_multi_cmds::tests::move_copy_linked_layers_is_atomic_and_has_one_journal_and_undo_step ... ok
test layer_multi_cmds::tests::moves_and_layer_props_report_only_their_area ... ok
test layer_multi_cmds::tests::selection_pruned_after_delete_and_undo ... ok
test layer_multi_cmds::tests::the_move_preview_matches_the_move ... ok
test layer_multi_cmds::tests::select_linked_layers_requires_a_linked_selection ... ok
test layer_multi_cmds::tests::toggle_and_range_selection ... ok
test layer_multi_cmds::tests::select_linked_layers_excludes_an_unlinked_active_layer ... ok
test layer_multi_cmds::tests::translate_rejects_offsets_that_would_wrap ... ok
test layer_multi_cmds::tests::select_linked_layers_preserves_a_linked_active_layer ... ok
test layer_multi_cmds::tests::distribute_uneven_layers ... ok
test layer_multi_cmds::tests::linked_layers_move_together_and_undo ... ok
test layer_multi_cmds::tests::grouping_is_capped_at_the_document_nesting_limit ... ok
test mode_cmds::tests::bitmap_methods_need_grayscale ... ok
test pathops_cmds::validation_tests::review_regression_boolean_operation_rejects_non_string_and_unknown_values ... ok
test mode_cmds::tests::color_table_presets_and_entries ... ok
test mode_cmds::tests::modes_round_trip_through_pcraft ... ok
test transform_cmds::tests::affine_matrix_and_perspective ... ok
test transform_cmds::tests::concave_distort_draws_two_triangles_and_undoes ... ok
test mode_cmds::tests::duotone_inks_and_display ... ok
test transform_cmds::tests::ds2_single_layer_array_accepts_folded_quad_and_targets_named_layer ... ok
test transform_cmds::tests::background_becomes_layer_0 ... ok
test transform_cmds::tests::ds2_transform_layers_uses_union_and_rejects_empty_requests ... ok
test image_cmds::tests::image_size_keeps_canvas_edges_opaque ... ok
test transform_cmds::tests::linked_raster_mask_moves_with_the_pixels ... ok
test transform_cmds::tests::split_gray_selected_rejects_non_gray ... ok
test mode_cmds::tests::rotate_arbitrary_other_modes ... ok
test transform_cmds::tests::scale_via_quad_and_undo ... ok
test transform_cmds::tests::targeted_alpha_channel_transforms_alone ... ok
test transform_cmds::tests::shapes_move_and_transform_as_vectors ... ok
test transform_cmds::tests::linked_mask_with_a_selection_moves_only_the_selected_region ... ok
test transform_cmds::tests::folded_distort_on_a_smart_object_becomes_a_warp ... ok
test file_cmds::tests::flatten_all_layer_effects_and_masks ... ok
test mode_cmds::tests::indexed_exact_transparency_and_sources ... ok
test mode_cmds::tests::rotate_arbitrary_grows_canvas_and_keeps_background ... ok
test transform_cmds::tests::targeted_linked_mask_moves_with_its_layer ... ok
test vector_cmds::tests::all_vector_commands_are_registered ... ok
test transform_cmds::tests::targeted_unlinked_mask_transforms_alone ... ok
test transform_cmds::tests::transform_multiple_layers_is_atomic_and_one_undo_step ... ok
test transform_cmds::tests::type_moves_with_the_move_command ... ok
test transform_cmds::tests::type_layers_transform_as_vectors ... ok
test layer_copy_cmds::tests::converts_to_the_destinations_depth_and_colour_mode ... ok
test vector_cmds::tests::clipping_path_requires_saved_path_and_tracks_rename_delete ... ok
test vector_cmds::tests::copy_and_paste_shape_fill_and_stroke ... ok
test transform_cmds::tests::transform_targets_fail_gracefully ... ok
test vector_cmds::tests::path_stroke_rejects_oversized_brush_before_rendering ... ok
test vector_cmds::tests::fill_and_stroke_path_on_pixel_layer ... ok
test vector_cmds::tests::paths_set_rename_delete_list ... ok
test transform_cmds::tests::with_selection_only_selected_pixels_move ... ok
test vector_cmds::tests::path_fill_handles_extreme_coordinates_and_feather ... ok
test transform_cmds::tests::unlinked_mask_stays_put ... ok
test vector_cmds::tests::shape_create_fill_stroke_and_composite ... ok
test vector_cmds::tests::shape_add_to_with_path_ops ... ok
test vector_cmds::tests::shape_rasterize_keeps_pixels ... ok
test vector_cmds::tests::path_to_selection_matches_analytic_shape ... ok
test vector_integration::integration_tests::integration_projected_live_groups_reserve_global_ids_and_are_stable ... ok
test vector_cmds::tests::transform_work_saved_and_shape_paths ... ok
test vector_cmds::tests::translate_vectors_moves_shapes_and_masks ... ok
test vector_cmds::tests::shape_edit_live_params_and_undo ... ok
test vector_integration::integration_tests::integration_shared_insertion_isolation_uses_parent_and_rejects_stale_selection ... ok
test hotfix_critical_tests::hotfix_live_and_clipped_vectors_save_as_psd_psb_and_layered_tiff ... ok
test vector_cmds::tests::selection_to_work_path_roundtrip_iou ... ok
test vector_packages::layout::tests::layout16_aggregate_output_budget_refuses_many_safe_individual_extents_atomically ... ok
test vector_cmds::tests::vector_mask_commands_and_compositing ... ok
test vector_integration::integration_tests::integration_shared_insertion_draw_modes_preview_publication_and_undo ... ok
test vector_packages::layout::tests::layout16_all_descendant_locks_and_ancestor_overlap_are_preflighted ... ok
test vector_packages::layout::tests::layout16_anchor_transform_keeps_selected_anchor_and_has_one_undo ... ok
test vector_packages::layout::tests::layout16_aggregate_preflight_counts_cached_appearance_extent_not_only_small_paths ... ok
test layer_multi_cmds::tests::align_each_edge ... ok
test vector_packages::layout::tests::layout16_constraint_set_requires_boolean_object_or_null_and_is_atomic ... ok
test vector_packages::layout::tests::layout16_bounded_caches_and_invalid_transforms ... ok
test vector_packages::layout::tests::layout16_automatic_spacing_preserves_outer_edges ... ok
test image_cmds::tests::mode_and_depth_conversions ... ok
test vector_packages::layout::tests::layout16_existing_artboard_set_applies_constraints_without_changing_legacy_unconstrained_children ... ok
test vector_cmds::tests::shape_create_every_kind_and_undo ... ok
test vector_packages::layout::tests::layout16_invalid_late_target_duplicate_key_and_stale_capture_are_atomic ... ok
test vector_packages::layout::tests::layout16_fractional_artboard_requests_pin_to_the_published_integer_frame_without_drift ... ok
test vector_packages::layout::tests::layout16_group_padding_without_a_persisted_frame_is_rejected_atomically ... ok
test vector_packages::layout::tests::layout16_large_parent_frame_never_allocates_a_parent_sized_cache ... ok
test vector_packages::layout::tests::layout16_crop_content_move_keeps_clip_in_place ... ok
test hotfix_critical_tests::hotfix_live_shapes_follow_move_transform_and_canvas_geometry ... ok
test vector_packages::layout::tests::layout16_point_similarity_has_independent_pivot_and_endpoint_oracles ... ok
test vector_packages::layout::tests::layout16_crop_preserves_geometry_masks_and_undo_clear_restore ... ok
test vector_packages::layout::tests::layout16_units_exact_definitions_and_legacy_default ... ok
test vector_packages::layout::tests::layout16_negative_pasteboard_artboard_create_resize_does_not_wrap_canvas_dimensions ... ok
test vector_packages::path_commands::tests::object36_grid_exact_six_cells_ten_gutters_and_guides ... ok
test vector_packages::path_commands::tests::object36_invalid_last_target_locked_stale_numeric_and_anchor_set_are_atomic ... ok
test vector_packages::layout::tests::review_regression_layout16_automatic_spacing_uses_farthest_edge_not_last_origin ... ok
test vector_packages::layout::tests::layout16_key_alignment_and_interior_key_spacing_preserve_key ... ok
test vector_packages::layout::tests::layout16_native_old_fixture_defaults_pixels_and_crop_roundtrip ... ok
test vector_packages::path_commands::tests::object36_cleanup_exact_seeded_strays_unpainted_empty_text_and_one_undo ... ok
test vector_packages::layout::tests::layout16_resize_rotated_child_preserves_right_margin_and_exact_vertices ... ok
test vector_packages::path_commands::tests::object36_divide_below_conserves_area_and_consumes_only_cutter ... ok
test vector_packages::path_commands::tests::object36_selected_anchors_use_base_indices_before_live_corner_projection ... ok
test canvas_geom::tests::turns_at_every_depth_and_mode ... ok
test vector_packages::transform_tools::tests::transform34_auxiliary_patterns_off_nonlinear_geometry_retains_canvas_phase ... ok
test vector_packages::transform_tools::tests::review_followup_reshape_again_preserves_identity_when_selection_is_reordered ... ok
test vector_packages::path_commands::tests::object36_add_midpoints_remove_selected_restores_curve_and_compound_ops ... ok
test vector_packages::path_commands::tests::object36_average_global_centroid_axes_and_handles ... ok
test vector_packages::transform_tools::tests::transform34_auxiliary_editable_text_effect_scaling_option ... ok
test vector_packages::path_commands::tests::object36_vector_clipboard_preserves_group_masks_effects_and_native_metadata ... ok
test vector_packages::path_commands::tests::object36_join_coincident_noncoincident_close_straight_and_one_undo ... ok
test vector_packages::transform_tools::tests::transform34_engine_again_copy_three_repeats_exact_offsets ... ok
test vector_packages::transform_tools::tests::transform34_engine_distort_trapezoid_preview_commit_matches ... ok
test vector_packages::path_commands::tests::object36_vector_paste_without_selection_uses_root_ends_and_empty_document ... ok
test vector_packages::transform_tools::tests::transform34_engine_each_seed_deterministic_local_reference_and_copy ... ok
test vector_packages::transform_tools::tests::transform34_conversion_styled_and_path_text_preserve_paints_decorations_atomic_copy ... ok
test vector_packages::transform_tools::tests::transform34_engine_old_options_numeric_validation_move_oracle ... ok
test vector_packages::transform_tools::tests::transform34_engine_pattern_options_phase_similarity_and_provider_failure ... ok
test vector_packages::transform_tools::tests::transform34_engine_group_all_descendants_atomic_and_copy_ids_unique ... ok
test vector_packages::transform_tools::tests::transform34_engine_group_clip_linked_vector_mask_and_effects ... ok
test vector_packages::path_commands::tests::object36_simplify_circle_independent_bidirectional_hausdorff_preview_and_counts ... ok
test vector_packages::transform_tools::tests::transform34_corner_pattern_frame_rounded_geometry_keeps_canvas_phase ... ok
test vector_packages::path_commands::tests::object36_vector_clipboard_symbol_resources_use_owner_transfer_without_collisions ... ok
test vector_packages::transform_tools::tests::transform34_engine_entire_target_set_stale_locked_invalid_no_publication ... ok
test vector_packages::transform_tools::tests::transform34_engine_reshape_selected_and_falloff_independent ... ok
test vector_packages::transform_tools::tests::transform34_engine_rotate_clicked_node_exact_preview_atomic_undo ... ok
test vector_packages::transform_tools::tests::transform34_engine_reflect_twice_30_degrees_identity_and_shear_area ... ok
test vector_packages::transform_tools::tests::transform34_engine_symbol_conversion_choice_atomic_and_retains_original_with_copy ... ok
test vector_packages::transform_tools::tests::transform34_engine_reshape_copy_again_remaps_node_targets ... ok
test vector_packages::transform_tools::tests::transform34_engine_live_similarity_preserved_nonsimilarity_baked ... ok
test vector_packages::transform_tools::tests::transform34_auxiliary_entire_text_set_budget_before_provider_rasterizes ... ok
test vector_packages::path_commands::tests::object36_vector_clipboard_front_back_order_coordinates_native_and_undo ... ok
test vector_packages::transform_tools::tests::transform34_engine_stroke_profile_effect_and_corner_options ... ok
test mode_cmds::tests::indexed_color_palettes_and_dithers ... ok
test vector_packages::transform_tools::tests::transform34_engine_live_unrepresentable_reflection_bakes_without_geometry_loss ... ok
test vector_cmds::tests::rasterizing_a_vector_mask_keeps_what_the_masks_show ... ok
test vector_packages::layout::tests::gpu::layout16_rtx_crop_and_resize_mixed_layers_preview_release_undo_parity ... ok
test vector_packages::transform_tools::gpu_tests::transform_tools_real_gpu_detached_preview_release_and_cancel_parity ... ok

failures:

---- layer_multi_cmds::tests::merge_selected_into_top_most stdout ----

thread 'layer_multi_cmds::tests::merge_selected_into_top_most' (2032539) panicked at crates/pc-engine/src/layer_multi_cmds.rs:1204:13:
assertion `left == right` failed: undo leaves a valid selection
  left: [LayerId(634), LayerId(636)]
 right: [LayerId(636)]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    layer_multi_cmds::tests::merge_selected_into_top_most

test result: FAILED. 192 passed; 1 failed; 0 ignored; 0 measured; 1126 filtered out; finished in 1.29s

/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner: line 13: total +  : arithmetic syntax error: operand expected (error token is "+  ")
/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner: line 18: [: : integer expected
error: test failed, to rerun pass `-p photocraft-engine --lib`
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc)

running 47 tests
test brush_panel::tests::sections_with_boxes_have_flags ... ok
test fill_stroke_proxy::tests::review_additional_proxy_edits_only_selected_gradient_stop ... ok
test brush_panel::tests::section_locks_round_trip_and_survive_preset_picks ... ok
test brush_panel::tests::drop_targets_and_actions_reorder_presets ... ok
test fill_stroke_proxy::tests::toolbar46_gpu_full_app_proxy_keys_icon_upload_and_pixel_parity ... ignored, 00 production toolbar/menu wiring and real RTX 5090 GPU milestone; owner speed rules
test fill_stroke_proxy::tests::toolbar46_gpu_upload_every_vector_icon_and_switch_styles ... ignored, RTX 5090 icon upload milestone; owner speed rules prohibit GPU runs
test brush_panel::tests::bad_patches_fail_without_touching_the_brush ... ok
test brush_panel::tests::patches_are_minimal_and_skip_unchanged_bitmaps ... ok
test brush_panel::tests::mixer_brush_options_update_the_persisted_mixer_settings ... ok
test brush_panel::tests::options_bar_edits_are_one_set_brush_per_gesture ... ok
test brush_panel::tests::preview_strip_re_renders_only_when_the_brush_changes ... ok
test brush_panel::tests::f5_edits_the_default_brush_with_every_section ... ok
test brush_panel::tests::options_bar_reaches_brush_settings_and_the_preset_library ... ok
test brush_panel::tests::brushes_panel_groups_collapse_and_filter ... ok
test brush_panel::tests::brush_sections_paint_simplified_chinese_labels_and_heading ... ok
test panels::color_tests::background_chip_retargets_the_color_field ... ok
test panels::color_tests::each_chip_keeps_its_own_hue_on_black ... ok
test panels::color_tests::double_clicking_a_chip_opens_its_color_picker ... ok
test panels::color_tests::srgb_hsva_roundtrip ... ok
test panels::color_tests::hue_and_saturation_fields_take_values_on_black ... ok
test panels::lock_tests::simple_lock_button_never_locks_the_eraser_out_of_the_background ... ok
test panels::history_transform_tests::history_rows_wait_for_an_open_transform ... ok
test hotfix_critical_tests::hotfix_options_bar_controls_are_inside_the_visible_row ... ok
test panels::swatch_type_tests::clicking_a_swatch_recolours_selected_type ... ok
test panels::title_tests::title_never_overlaps_menus_or_controls ... ok
test panels::properties_card_tests::uncommitted_arithmetic_stays_with_its_layer ... ok
test panels::swatch_type_tests::idle_hsb_sliders_leave_the_foreground_and_selected_type_alone ... ok
test preset_panels::tests::clone_marker_follows_the_engine_slot ... ok
test preset_panels::tests::clone_marker_shows_only_while_painting ... ok
test hotfix_critical_tests::hotfix_inline_vector_text_owns_tool_and_paint_shortcut_letters ... ok
test preset_panels::tests::custom_shape_tool_places_the_picked_shape ... ok
test preset_panels::tests::tool_preset_switches_tool_and_options ... ok
test preset_panels::tests::window_menu_toggles_panels_and_color_tabs ... ok
test vector_integration::review_tests::review_additional_vector_preferences_reset_and_unrelated_edits ... ok
test brush_panel::tests::every_section_round_trips_through_set_brush ... ok
test panels::type_flyout_tests::long_press_type_button_selects_vertical_without_selecting_on_release ... ok
test fill_stroke_proxy::tests::toolbar46_contact_sheet_original_artwork ... ok
test fill_stroke_proxy::tests::toolbar46_detached_proxy_rejects_stale_selection_locked_or_nonfinite_set ... ok
test fill_stroke_proxy::tests::toolbar46_frozen_pixel_menu_keys_and_all_chords_parse ... ok
test fill_stroke_proxy::tests::toolbar46_hold_flyout_curvature_and_preset_keyboard_switch ... ok
test fill_stroke_proxy::tests::toolbar46_multiple_targets_swap_individually_and_locked_tail_is_atomic ... ok
test fill_stroke_proxy::tests::toolbar46_narrow_toolbar_controls_popups_and_captured_release ... ok
test fill_stroke_proxy::tests::toolbar46_pointer_hold_detached_preview_and_single_release ... ok
test fill_stroke_proxy::tests::toolbar46_proxy_clicks_keys_one_undo_and_defaults ... ok
test fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 ... FAILED
test fill_stroke_proxy::tests::toolbar46_swap_preserves_hidden_paints_and_stroke_metadata ... ok
test brush_panel::tests::every_section_draws_without_issuing_commands ... ok

failures:

---- fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 stdout ----

thread 'fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40' (2032939) panicked at crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy/tests.rs:252:13:
padding blend/16
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40

test result: FAILED. 44 passed; 1 failed; 2 ignored; 0 measured; 1278 filtered out; finished in 0.90s

error: test failed, to rerun pass `-p photocraft-ui-egui --lib`
error: 2 targets failed:
    `-p photocraft-engine --lib`
    `-p photocraft-ui-egui --lib`

```

</details>

### Confirm the Merge Selected failure on the original implementation binary

```bash
target/debug/deps/photocraft_engine-0962a62fc0794aad --exact layer_multi_cmds::tests::merge_selected_into_top_most --nocapture > /tmp/codex-hotfix-critical-logs/baseline-merge.log 2>&1
```

Exit code: `101`.

<details>
<summary>Captured output</summary>

```text

running 1 test

thread 'layer_multi_cmds::tests::merge_selected_into_top_most' (2033237) panicked at crates/pc-engine/src/layer_multi_cmds.rs:1204:13:
assertion `left == right` failed: undo leaves a valid selection
  left: [LayerId(3), LayerId(5)]
 right: [LayerId(5)]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test layer_multi_cmds::tests::merge_selected_into_top_most ... FAILED

failures:

failures:
    layer_multi_cmds::tests::merge_selected_into_top_most

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1318 filtered out; finished in 0.01s


```

</details>

### Confirm the Blend icon failure on the original implementation binary

```bash
target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc --exact fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 --nocapture > /tmp/codex-hotfix-critical-logs/baseline-icon.log 2>&1
```

Exit code: `101`.

<details>
<summary>Captured output</summary>

```text

running 1 test

thread 'fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40' (2033240) panicked at crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy/tests.rs:252:13:
padding blend/16
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 ... FAILED

failures:

failures:
    fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1324 filtered out; finished in 0.02s


```

</details>

### IO and native-service tests

```bash
cargo test --offline -p photocraft-io -p local-image --no-fail-fast -- vector_fallback:: tiff_layers:: vector:: psd32_export:: psd_structure:: flat:: services::tests:: > /tmp/codex-hotfix-critical-logs/io-native-tests.log 2>&1
```

Exit code: `0`.

<details>
<summary>Captured output</summary>

```text
   Compiling rustix v1.1.5
   Compiling bitflags v2.13.2
   Compiling serde_json v1.0.151
   Compiling indexmap v2.14.2
   Compiling font-types v0.12.5
   Compiling photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-psd)
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling enumn v0.1.14
   Compiling ahash v0.8.12
   Compiling futures-sink v0.3.34
   Compiling emath v0.36.2
   Compiling foldhash v0.1.5
   Compiling wait-timeout v0.2.1
   Compiling fnv v1.0.7
   Compiling quick-error v1.2.3
   Compiling fixedbitset v0.5.7
   Compiling bit-vec v0.8.0
   Compiling futures-channel v0.3.34
   Compiling png v0.18.1
   Compiling spirv v0.4.0+sdk-1.4.341.0
   Compiling raw-cpuid v11.6.0
   Compiling futures-util v0.3.34
   Compiling rustix v0.38.44
   Compiling ron v0.12.2
   Compiling read-fonts v0.41.0
   Compiling hashbrown v0.15.5
   Compiling ecolor v0.36.2
   Compiling bit-set v0.8.0
   Compiling xkbcommon-dl v0.4.2
   Compiling naga-types v30.0.1
   Compiling write-fonts v0.48.1
   Compiling wgpu-types v30.0.1
   Compiling rand_xorshift v0.4.0
   Compiling image v0.25.10
   Compiling petgraph v0.8.3
   Compiling photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-doc)
   Compiling naga v30.0.1
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-develop)
   Compiling pulp v0.22.3
   Compiling ident_case v1.0.1
   Compiling accesskit v0.24.1
   Compiling io-lifetimes v3.0.1
   Compiling strsim v0.11.1
   Compiling io-lifetimes v2.0.4
   Compiling unarray v0.1.4
   Compiling darling_core v0.24.1
   Compiling pdf-writer v0.15.0
   Compiling lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-codecs)
   Compiling xattr v1.6.1
   Compiling wayland-backend v0.3.17
   Compiling polling v3.11.0
   Compiling tar v0.4.46
   Compiling async-io v2.6.0
   Compiling tract-nnef v0.23.8
   Compiling wayland-client v0.31.15
   Compiling harfrust v0.12.0
   Compiling skrifa v0.44.0
   Compiling photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vector)
   Compiling fontique v0.11.1
   Compiling wayland-protocols v0.32.13
   Compiling tract-extra v0.23.8
   Compiling tract-onnx-opl v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling wgpu-naga-bridge v30.0.1
   Compiling wgpu-hal v30.0.1
   Compiling tract-onnx v0.23.8
   Compiling wgpu-core-deps-windows-linux-android v30.0.1
   Compiling wgpu-core v30.0.1
   Compiling parley v0.11.1
   Compiling photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-compose)
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-text)
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-pipeline)
   Compiling wgpu v30.0.1
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-seg)
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-gpu)
   Compiling lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-merge)
   Compiling exr v1.74.2
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-catalog)
   Compiling wayland-protocols-wlr v0.3.12
   Compiling photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-codecs)
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-engine)
   Compiling photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-format)
   Compiling async-signal v0.2.14
   Compiling async-process v2.5.0
   Compiling zbus v5.19.0
   Compiling wayland-cursor v0.31.14
   Compiling wayland-csd-frame v0.3.0
   Compiling gethostname v1.1.0
   Compiling x11rb v0.13.2
   Compiling epaint v0.36.2
   Compiling calloop v0.13.0
   Compiling tempfile v3.27.0
   Compiling calloop-wayland-source v0.3.0
   Compiling smithay-client-toolkit v0.19.2
   Compiling rusty-fork v0.3.1
   Compiling wayland-protocols-plasma v0.3.12
   Compiling egui v0.36.2
   Compiling photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pathops)
   Compiling calloop v0.14.4
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-io)
   Compiling atspi-common v0.13.0
   Compiling accesskit_consumer v0.36.0
   Compiling atspi-proxies v0.13.0
   Compiling accesskit_atspi_common v0.18.1
   Compiling atspi v0.29.0
   Compiling winit v0.30.13
   Compiling calloop-wayland-source v0.4.1
   Compiling proptest v1.11.0
   Compiling wayland-protocols-experimental v20250721.0.1
   Compiling wayland-protocols-misc v0.3.12
   Compiling subsetter v0.2.6
   Compiling tree_magic_mini v3.2.2
   Compiling wasmparser v0.228.0
   Compiling os_pipe v1.2.3
   Compiling ref-cast v1.0.27
   Compiling io-extras v0.19.0
   Compiling ureq v3.4.2
   Compiling wasmi v2.0.0
   Compiling krilla v0.8.2
   Compiling wl-clipboard-rs v0.9.4
   Compiling hayro-interpret v0.7.0
   Compiling smithay-client-toolkit v0.20.0
   Compiling accesskit_unix v0.21.1
   Compiling darling_macro v0.24.1
   Compiling serde_derive_internals v0.30.0
   Compiling tokio-macros v2.7.2
   Compiling ref-cast-impl v1.0.27
   Compiling mio v1.2.3
   Compiling socket2 v0.6.5
   Compiling cap-primitives v4.0.3
   Compiling darling v0.24.1
   Compiling accesskit_winit v0.32.2
   Compiling fs-set-times v0.20.3
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-ai)
   Compiling photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-plugins)
   Compiling photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pdf)
   Compiling smithay-clipboard v0.7.3
   Compiling arboard v3.6.1
   Compiling photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-svg)
   Compiling tokio v1.53.1
   Compiling photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vecfx)
   Compiling photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-affinity)
   Compiling photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ops)
   Compiling pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-trace)
   Compiling rustix-linux-procfs v0.1.1
   Compiling futures-executor v0.3.34
   Compiling schemars_derive v1.2.2
   Compiling chrono v0.4.45
   Compiling ipnet v2.12.2
   Compiling rmcp v3.5.0
   Compiling maybe-owned v0.3.4
   Compiling ambient-authority v0.0.2
   Compiling cap-std v4.0.3
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-engine)
   Compiling futures v0.3.34
   Compiling egui-wgpu v0.36.2
   Compiling tokio-util v0.7.19
   Compiling egui-winit v0.36.2
   Compiling egui_extras v0.36.2
   Compiling li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-attributions)
   Compiling rmcp-macros v3.5.0
   Compiling winresource v0.1.31
   Compiling lightcraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-ui-egui)
   Compiling rfd v0.17.2
   Compiling eframe v0.36.2
   Compiling local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/apps/local-image)
   Compiling photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-gpu)
   Compiling photocraft-tablet v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-tablet)
   Compiling schemars v1.2.2
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ui-egui)
   Compiling photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-automation)
    Finished `test` profile [optimized + debuginfo] target(s) in 2m 14s
     Running unittests src/main.rs (target/debug/deps/local_image-24e52c54d774f790)

running 8 tests
test services::tests::save_filters_lead_with_every_export_format ... ok
test services::tests::tga_is_trusted_only_with_its_extension ... ok
test services::tests::crlf_from_a_uri_list_is_ignored ... ok
test services::tests::non_images_are_skipped_for_the_first_image ... ok
test services::tests::copied_image_file_pastes_its_pixels ... ok
test services::tests::a_damaged_image_is_skipped_not_a_panic ... ok
test services::tests::hotfix_native_roundtrip_export_supports_placed_vector_crop ... ok
test services::tests::recovered_documents_survive_a_second_crash_until_saved_or_closed ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 84 filtered out; finished in 0.02s

     Running tests/mac_menu_appkit.rs (target/debug/deps/mac_menu_appkit-3282c4949ca5ee8a)
     Running unittests src/lib.rs (target/debug/deps/photocraft_io-afa85e400222971b)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 111 filtered out; finished in 0.00s

     Running tests/coverage/main.rs (target/debug/deps/coverage-3d5a5a2a2ae42574)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 120 filtered out; finished in 0.00s

     Running tests/it/main.rs (target/debug/deps/it-3733cbdb0053f9f8)

running 99 tests
test flat::garbage_import_errors ... ok
test flat::alpha_to_jpeg_warns ... ok
test flat::animation_imports_the_first_frame_with_a_warning ... ok
test flat::jpeg_gray8 ... ok
test flat::extension_forms ... ok
test flat::sixteen_bit_to_jpeg_warns_depth ... ok
test flat::background_lock_follows_alpha ... ok
test flat::png_rgba16 ... ok
test flat::tga_gray8 ... ok
test flat::png_graya16 ... ok
test flat::tga_rgb8_opaque ... ok
test flat::tga_rgba8 ... ok
test flat::metadata_roundtrip_png ... ok
test flat::png_rgb8_opaque ... ok
test flat::exr_rgba32 ... ok
test flat::jpeg_rgb8 ... ok
test flat::png_gray8 ... ok
test flat::png_rgba8 ... ok
test flat::unknown_extension_errors ... ok
test flat::truncated_jpeg_imports_with_a_warning ... ok
test flat::tiff_gray8 ... ok
test flat::tiff_rgb16 ... ok
test flat::tiff_cmyk8 ... ok
test flat::tiff_rgba8 ... ok
test flat::tiff_cmyka16 ... ok
test psd_structure::adjustment_and_fill_layers_from_psd ... ok
test psd_structure::flattened_psd_becomes_background ... ok
test psd_structure::fill_layer_keeps_photoshop_pixels ... ok
test psd_structure::builder_psd_imports ... ok
test psd_structure::every_blend_mode_maps_both_ways ... ok
test psd_structure::indexed_colors_match_palette ... ok
test psd_structure::group_nesting_at_the_cap_imports ... ok
test psd_structure::layered_import_skips_the_unused_merged_composite ... ok
test psd_structure::group_nesting_past_the_cap_is_rejected ... ok
test psd_structure::locks_and_labels_from_psd ... ok
test psd_structure::merged_composite_unmattes_photoshop_white ... ok
test psd_structure::multichannel_imports_ink_channels ... ok
test psd_structure::resolution_in_cm_converted ... ok
test flat::layered_to_png_warns_flatten ... ok
test psd_structure::guides_and_alpha_names_roundtrip ... ok
test flat::transparency_is_composited_over_white_for_formats_without_alpha ... ok
test flat::non_srgb_rgb_is_converted_to_srgb_for_formats_without_a_profile ... ok
test psd32_export::flat_32bit_export_layout ... ok
test psd_structure::stale_index_resources_dropped ... ok
test flat::cmyk_layered_to_png_warns_conversion ... ok
test psd_structure::artboard_lock_round_trips_byte_for_byte ... ok
test psd_structure::testgen_import_details ... ok
test psd_structure::fallback_modes_import_flattened ... ok
test psd_structure::testgen_gray16_rle ... ok
test psd_structure::testgen_gray32_raw ... ok
test psd_structure::testgen_rgb16_zip ... ok
test flat::psd_to_png_to_psd_chain ... ok
test psd_structure::testgen_gray8_zipp ... ok
test psd_structure::text_and_smart_detected ... ok
test psd_structure::unmodelled_blocks_are_preserved ... ok
test psd_structure::vector_rendered_mask_not_doubled_on_shapes ... ok
test psd_structure::testgen_psb_cmyk16_zipp ... ok
test tiff_layers::a_plain_background_needs_no_layer_data ... ok
test psd_structure::testgen_lab8_rle ... ok
test flat::effects_are_written_to_psd_and_read_back ... ok
test psd_structure::testgen_psb_rgb8_rle ... ok
test tiff_layers::psdtags_fixtures_open_with_their_layers ... ok
test psd_structure::testgen_cmyk16_zip ... ok
test tiff_layers::discard_layers_writes_a_flat_file ... ok
test tiff_layers::default_export_options_write_a_flat_tiff ... ok
test psd_structure::testgen_rgb8_raw ... ok
test psd_structure::testgen_rgb32_zipp ... ok
test psd_structure::testgen_cmyk8_rle ... ok
test psd32_export::only_32bit_exports_carry_toning_data ... ok
test tiff_layers::lab_documents_are_saved_flat_with_a_warning ... ok
test psd_structure::testgen_small_all_compressions ... ok
test psd_structure::testgen_rgb8_rle ... ok
test tiff_layers::rt_gray8_all ... ok
test tiff_layers::rt_gray16_pixels ... ok
test tiff_layers::transparency_is_straight_in_the_composite ... ok
test tiff_layers::rt_cmyk16_all ... ok
test tiff_layers::big_documents_use_the_psb_signature ... ok
test tiff_layers::rt_gray16_all ... ok
test psd32_export::layered_32bit_export_reimports_render_equal ... ok
test vector_fallback::vector_fallback_integrated_normal_save_refuses_missing_cache_before_bytes ... ok
test tiff_layers::rt_rgb16_all ... ok
test vector::saved_paths_work_path_and_clipping_roundtrip ... ok
test vector_fallback::vector_fallback_integrated_path_text_user_vector_mask_applies_once ... ok
test vector_fallback::vector_fallback_integrated_profile_brush_proxy_uses_cache_and_warns_once ... ok
test tiff_layers::rt_gray32_all ... ok
test vector_fallback::vector_fallback_integrated_path_text_symbol_proxy_caches_and_empty_stacks ... ok
test vector::shape_layer_typed_roundtrip ... ok
test vector::vector_mask_roundtrip_and_removal ... ok
test tiff_layers::rt_rgb8_pixels ... ok
test tiff_layers::rt_cmyk8_pixels ... ok
test tiff_layers::rt_rgb8_all ... ok
test tiff_layers::rt_cmyk8_all ... ok
test vector_fallback::vector_fallback_integrated_shape_user_mask_and_gradient_effect_preserve_pixels ... ok
test tiff_layers::rt_rgb32_all ... ok
test vector_fallback::vector_fallback_integrated_psd_psb_all_depths_reopen_pixels_and_raw_blocks ... ok
test psd32_export::layered_32bit_export_layout ... ok
test tiff_layers::every_compression_keeps_the_layers ... ok
test tiff_layers::tiff_and_psd_open_as_the_same_document ... ok
test tiff_layers::damaged_layer_data_opens_flattened ... ok

test result: ok. 99 passed; 0 failed; 0 ignored; 0 measured; 113 filtered out; finished in 0.39s

     Running tests/overwrite_original.rs (target/debug/deps/overwrite_original-d2bafd9d71a0d278)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s

   Doc-tests photocraft_io

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s


```

</details>

### Workspace check

```bash
cargo check --offline --workspace > /tmp/codex-hotfix-critical-logs/workspace-check.log 2>&1
```

Exit code: `0`.

<details>
<summary>Captured output</summary>

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
    Checking bytemuck v1.25.2
    Checking zerocopy v0.8.59
    Checking zeroize v1.9.1
    Checking linux-raw-sys v0.12.1
    Checking simd-adler32 v0.3.10
    Checking thiserror v2.0.21
    Checking pin-project-lite v0.2.17
    Checking equivalent v1.0.2
    Checking stable_deref_trait v1.2.1
    Checking typenum v1.20.1
    Checking adler2 v2.0.1
    Checking itoa v1.0.18
    Checking hashbrown v0.17.1
    Checking subtle v2.6.1
    Checking crc32fast v1.5.2
    Checking zmij v1.0.23
    Checking libloading v0.8.9
    Checking tracing-core v0.1.36
    Checking miniz_oxide v0.9.1
    Checking allocator-api2 v0.2.21
    Checking const-oid v0.9.6
    Checking crossbeam-epoch v0.9.21
    Checking unicode-ident v1.0.26
    Checking scopeguard v1.2.0
    Checking dlib v0.5.3
    Checking miniz_oxide v0.8.9
    Checking rand_core v0.10.1
    Checking nom v8.0.0
    Checking lock_api v0.4.14
    Checking cpufeatures v0.2.17
    Checking wayland-sys v0.31.11
    Checking tracing v0.1.44
    Checking crossbeam-deque v0.8.8
    Checking zune-core v0.5.3
    Checking fdeflate v0.3.7
    Checking slab v0.4.12
    Checking flate2 v1.1.10
    Checking generic-array v0.14.7
    Checking spin v0.9.9
    Checking getrandom v0.3.4
    Checking getrandom v0.4.3
    Checking num-traits v0.2.19
    Checking rayon-core v1.13.0
    Checking memmap2 v0.9.11
    Checking zerofrom v0.1.8
    Checking anyhow v1.0.104
    Checking zune-jpeg v0.5.15
    Checking quick-error v2.0.1
    Checking rand_core v0.9.5
    Checking futures-core v0.3.34
    Checking lazy_static v1.5.0
    Checking typeid v1.0.3
    Checking yoke v0.8.3
    Checking getrandom v0.2.17
    Checking itertools v0.15.0
    Checking weezl v0.1.12
    Checking pxfm v0.1.30
    Checking profiling v1.0.18
    Checking color_quant v1.1.0
    Checking crypto-common v0.1.7
    Checking block-buffer v0.10.4
    Checking static_assertions v1.1.0
    Checking rayon v1.12.0
    Checking byteorder-lite v0.1.0
    Checking rand_core v0.6.4
    Checking fax v0.2.6
    Checking siphasher v1.0.4
    Checking base64ct v1.8.3
    Checking digest v0.10.7
    Checking qoi v0.4.1
    Checking gif v0.14.2
    Checking num-integer v0.1.47
    Checking serde v1.0.229
    Checking bitflags v2.13.2
    Checking serde_json v1.0.151
    Checking hashbrown v0.16.1
    Checking num-complex v0.4.6
    Checking indexmap v2.14.2
    Checking image-webp v0.2.4
    Checking futures-io v0.3.34
    Checking pem-rfc7468 v0.7.0
    Checking rustix v1.1.5
    Checking png v0.18.1
    Checking uuid v1.26.1
    Checking raw-window-handle v0.6.2
    Checking der v0.7.10
    Checking smallvec v1.16.2
    Checking arrayvec v0.7.8
    Checking photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-geom)
    Checking font-types v0.12.5
    Checking rawpointer v0.2.1
    Checking parking_lot_core v0.9.12
    Checking dyn-clone v1.0.20
    Checking polycool v0.4.0
    Checking scoped-tls v1.0.1
    Checking downcast-rs v1.2.1
    Checking linebender_resource_handle v0.1.1
    Checking byteorder v1.5.0
    Checking parking_lot v0.12.5
    Checking matrixmultiply v0.3.11
    Checking bytes v1.12.1
    Checking zerovec v0.11.8
    Checking kurbo v0.13.1
    Checking string-interner v0.20.0
    Checking lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-geom)
    Checking jxl-bitstream v1.1.0
    Checking strength_reduce v0.2.4
    Checking downcast-rs v2.0.2
    Checking nom-language v0.1.0
    Checking ndarray v0.17.2
    Checking maplit v1.0.2
    Checking dyn-hash v1.0.0
    Checking rustc-hash v1.1.0
    Checking spki v0.7.3
    Checking photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-cms)
    Checking inventory v0.3.25
    Checking parking v2.2.1
    Checking transpose v0.2.3
    Checking primal-check v0.3.4
    Checking naga-types v30.0.1
    Checking num-bigint v0.4.8
    Checking raw-cpuid v11.6.0
    Checking pkcs5 v0.7.1
    Checking sha2 v0.10.9
    Checking read-fonts v0.41.0
    Checking unicode-width v0.2.2
    Checking bit-vec v0.10.1
    Checking pulp-wasm-simd-flag v0.1.1
    Checking pkcs8 v0.10.2
    Checking reborrow v0.5.5
    Checking writeable v0.6.4
    Checking bit-vec v0.9.1
    Checking fearless_simd v0.4.1
    Checking ppv-lite86 v0.2.21
    Checking codespan-reporting v0.13.1
    Checking lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-color)
    Checking moxcms v0.8.1
    Checking bit-set v0.10.0
    Checking rustfft v6.4.1
    Checking photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-color)
    Checking rand_chacha v0.9.0
    Checking erased-serde v0.4.10
    Checking jxl-oxide-common v1.0.0
    Checking spirv v0.4.0+sdk-1.4.341.0
    Checking jxl-grid v0.6.2
    Checking filetime v0.2.29
    Checking rand v0.9.5
    Checking anymap3 v1.1.0
    Checking photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-raster)
    Checking num-rational v0.4.2
    Checking cpufeatures v0.3.1
    Checking alloc-no-stdlib v2.0.4
    Checking ash v0.38.0+1.3.281
    Checking tinystr v0.8.4
    Checking photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-doc)
    Checking alloc-stdlib v0.2.4
    Checking bit-set v0.11.1
    Checking rand_distr v0.5.1
    Checking wgpu-types v30.0.1
    Checking jxl-coding v1.0.1
    Checking jxl-threadpool v1.0.0
    Checking same-file v1.0.6
    Checking litemap v0.8.3
    Checking winnow v1.0.4
    Checking pulp v0.22.3
    Checking presser v0.3.1
    Checking icu_locale_core v2.3.0
    Checking walkdir v2.5.0
    Checking half v2.7.1
    Checking equator v0.4.2
    Checking brotli-decompressor v5.0.3
    Checking tiff v0.11.3
    Checking naga v30.0.1
    Checking tract-data v0.23.8
    Checking khronos-egl v6.0.0
    Checking wayland-backend v0.3.17
    Checking polling v3.11.0
    Checking xattr v1.6.1
    Checking potential_utf v0.1.6
    Checking zerotrie v0.2.5
    Checking hmac v0.12.1
    Checking tar v0.4.46
    Checking ordered-float v5.5.0
    Checking image v0.25.10
    Checking proc-macro2 v1.0.107
    Checking wayland-client v0.31.15
    Checking glow v0.17.0
    Checking renderdoc-sys v1.1.0
    Checking fastrand v2.5.0
    Checking rustc-hash v2.1.3
    Checking quote v1.0.47
    Checking aligned-vec v0.6.4
    Checking jxl-modular v0.11.3
    Checking jxl-image v0.13.0
    Checking tract-linalg v0.23.8
    Checking icu_provider v2.3.1
    Checking chacha20 v0.10.2
    Checking lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-raster)
    Checking event-listener v5.4.2
    Checking signature v2.2.0
    Checking ff v0.13.1
    Checking float-cmp v0.9.0
    Checking inout v0.1.4
    Checking concurrent-queue v2.5.0
    Checking hex v0.4.3
    Checking base16ct v0.2.0
    Checking utf8_iter v1.0.4
    Checking cipher v0.4.4
    Checking sec1 v0.7.3
    Checking strict-num v0.1.1
    Checking icu_collections v2.3.0
    Checking group v0.13.0
    Checking jxl-vardct v0.11.1
    Checking rand v0.10.3
    Checking v_frame v0.3.9
    Checking syn v3.0.6
    Checking wayland-protocols v0.32.13
    Checking futures-lite v2.6.1
    Checking event-listener-strategy v0.5.4
    Checking hkdf v0.12.4
    Checking crypto-bigint v0.5.5
    Checking universal-hash v0.5.1
    Checking as-slice v0.2.1
    Checking rustls-pki-types v1.15.1
    Checking percent-encoding v2.3.2
    Checking opaque-debug v0.3.1
    Checking arrayref v0.3.9
    Checking jxl-frame v0.13.3
    Checking aligned v0.4.3
    Checking rand_distr v0.6.0
    Checking enumflags2 v0.7.12
    Checking zcheapstr v1.1.0
    Checking no_std_io2 v0.9.4
    Checking quick-xml v0.38.4
    Checking rgb v0.8.53
    Checking endi v1.1.1
    Checking futures-sink v0.3.34
    Checking untrusted v0.9.0
    Checking y4m v0.8.0
    Checking roxmltree v0.20.0
    Checking base64 v0.22.1
    Checking float-ord v0.3.2
    Checking elliptic-curve v0.13.8
    Checking prost v0.14.4
    Checking av-scenechange v0.14.1
    Checking photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vector)
    Checking icu_properties_data v2.3.0
    Checking icu_locale_fallback_data v2.3.0
    Checking bitstream-io v4.10.0
    Checking av1-grain v0.2.5
    Checking jxl-color v0.11.0
    Checking rfc6979 v0.4.0
    Checking tract-core v0.23.8
    Checking rand_chacha v0.3.1
    Checking maybe-rayon v0.1.1
    Checking lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-tiff)
    Checking skrifa v0.44.0
    Checking harfrust v0.12.0
    Checking phf_shared v0.13.1
    Checking errno v0.3.14
    Checking itertools v0.14.0
    Checking web-time v1.1.0
    Checking imgref v1.12.3
    Checking async-task v4.7.1
    Checking new_debug_unreachable v1.0.6
    Checking phf v0.13.1
    Checking jxl-render v0.12.4
    Checking accesskit v0.24.1
    Checking loop9 v0.1.5
    Checking curve25519-dalek v4.1.3
    Checking rand v0.8.8
    Checking lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-meta)
    Checking ecdsa v0.16.9
    Checking icu_properties v2.3.0
    Checking primeorder v0.13.6
    Checking icu_locale_fallback v2.3.0
    Checking zvariant_utils v4.2.0
    Checking jxl-jbr v0.2.1
    Checking rav1e v0.8.1
    Checking async-io v2.6.0
    Checking icu_normalizer_data v2.3.0
    Checking icu_segmenter_data v2.3.0
    Checking rustls-webpki v0.103.15
    Checking futures-channel v0.3.34
    Checking polyval v0.6.2
    Checking async-channel v2.5.0
    Checking lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-develop)
    Checking zvariant v5.15.0
    Checking avif-serialize v0.8.9
    Checking num-iter v0.1.46
    Checking euclid v0.22.14
    Checking aead v0.5.2
    Checking aho-corasick v1.1.5
    Checking font-types v0.11.3
    Checking color v0.3.3
    Checking jpeg-encoder v0.7.1
    Checking atomic-waker v1.1.2
    Checking signal-hook-registry v1.4.8
    Checking futures-task v0.3.34
    Checking parlance v0.1.0
    Checking regex-syntax v0.8.11
    Checking futures-util v0.3.34
    Checking guillotiere v0.7.0
    Checking peniko v0.6.1
    Checking fontique v0.11.1
    Checking read-fonts v0.39.2
    Checking piper v0.2.5
    Checking rustls v0.23.45
    Checking wayland-protocols-wlr v0.3.12
    Checking num-bigint-dig v0.8.6
    Checking parley_data v0.11.1
    Checking ghash v0.5.1
    Checking jxl-oxide v0.12.6
    Checking icu_segmenter v2.3.0
    Checking zbus_names v4.3.4
    Checking icu_normalizer v2.3.0
    Checking wgpu-naga-bridge v30.0.1
    Checking gpu-allocator v0.28.0
    Checking ahash v0.8.12
    Checking tiny-skia-path v0.11.4
    Checking poly1305 v0.8.0
    Checking async-lock v3.4.2
    Checking ctr v0.9.2
    Checking regex-automata v0.4.18
    Checking aes v0.8.4
    Checking chacha20 v0.9.1
    Checking ed25519 v2.2.3
    Checking wgpu-hal v30.0.1
    Checking moxcms v0.9.1
    Checking pkcs1 v0.7.5
    Checking jpeg-decoder v0.3.2
    Checking kurbo v0.11.3
    Checking pollster v1.0.1
    Checking ttf-parser v0.25.1
    Checking async-signal v0.2.14
    Checking ed25519-dalek v2.2.0
    Checking rsa v0.9.10
    Checking chacha20poly1305 v0.10.1
    Checking aes-gcm v0.10.3
    Checking async-process v2.5.0
    Checking parley v0.11.1
    Checking lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-pipeline)
    Checking zbus_xml v5.2.1
    Checking vello_common v0.1.0
    Checking ravif v0.13.0
    Checking blocking v1.7.0
    Checking httparse v1.10.1
    Checking svgtypes v0.15.3
    Checking p256 v0.13.2
    Checking p384 v0.13.1
    Checking lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-raw)
    Checking x25519-dalek v2.0.1
    Checking async-executor v1.14.0
    Checking rustls-webpki v0.102.8
    Checking webpki-roots v1.0.9
    Checking lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-codecs)
    Checking async-broadcast v0.7.2
    Checking lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-scenes)
    Checking http v1.5.0
    Checking emath v0.36.2
    Checking regex v1.13.1
    Checking ordered-stream v0.2.0
    Checking photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-psd)
    Checking simplecss v0.2.2
    Checking xmlwriter v0.1.0
    Checking xkeysym v0.2.1
   Compiling dyn-stack-macros v0.1.3
    Checking owned_ttf_parser v0.25.1
    Checking xcursor v0.3.11
    Checking imagesize v0.13.0
    Checking cursor-icon v1.2.0
    Checking ab_glyph_rasterizer v0.1.10
    Checking linux-raw-sys v0.4.15
    Checking pico-args v0.5.0
    Checking data-url v0.3.2
    Checking wayland-cursor v0.31.14
    Checking ab_glyph v0.2.32
    Checking wayland-csd-frame v0.3.0
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking ecolor v0.36.2
    Checking lensfun v0.7.0
    Checking rustls-rustcrypto v0.0.2-alpha
    Checking zbus v5.19.0
    Checking thiserror v1.0.69
    Checking usvg v0.45.1
    Checking rustix v0.38.44
    Checking vello_cpu v0.1.0
    Checking dyn-stack v0.13.2
    Checking zbus-lockstep v0.5.2
    Checking unicode-general-category v1.1.0
    Checking lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-catalog)
    Checking lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-preview)
    Checking photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-compose)
    Checking wgpu-core-deps-windows-linux-android v30.0.1
    Checking linesweeper v0.4.0
    Checking gethostname v1.1.0
    Checking photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-algo)
    Checking photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-text)
    Checking zune-inflate v0.2.54
    Checking unicode-segmentation v1.13.3
    Checking as-raw-xcb-connection v1.0.1
    Checking hayro-ccitt v0.3.0
    Checking base64 v0.23.1
    Checking epaint_default_fonts v0.36.2
    Checking self_cell v1.3.0
    Checking nohash-hasher v0.2.0
   Compiling seq-macro v0.3.6
    Checking bit_field v0.10.3
    Checking twox-hash v2.1.4
    Checking lebe v0.5.3
    Checking x11rb-protocol v0.13.2
    Checking ureq-proto v0.6.4
    Checking ruzstd v0.9.0
    Checking epaint v0.36.2
    Checking photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pathops)
    Checking exr v1.74.2
    Checking lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-merge)
    Checking hayro-jbig2 v0.3.0
    Checking hayro-jpeg2000 v0.3.5
    Checking skrifa v0.42.1
    Checking write-fonts v0.48.1
    Checking brotli v8.0.4
    Checking ron v0.12.2
    Checking wgpu-core v30.0.1
    Checking wasmi_core v2.0.0
    Checking jpeg-encoder v0.6.1
    Checking gemm-common v0.19.0
    Checking constant_time_eq v0.4.2
    Checking ryu v1.0.23
    Checking utf8-zero v0.8.1
    Checking foldhash v0.1.5
    Checking option-ext v0.2.0
    Checking calloop v0.13.0
    Checking hayro-postscript v0.1.0
    Checking hashbrown v0.15.5
    Checking dirs-sys v0.5.0
    Checking ureq v3.4.2
    Checking blake3 v1.8.7
    Checking calloop-wayland-source v0.3.0
    Checking egui v0.36.2
    Checking wasmi_ir v2.0.0
    Checking hayro-syntax v0.7.2
    Checking wasmparser v0.228.0
    Checking tiny-skia-path v0.12.0
    Checking sha1 v0.10.7
    Checking httpdate v1.0.3
    Checking fixedbitset v0.5.7
    Checking pdf-writer v0.15.0
    Checking data-encoding v2.11.1
    Checking imagesize v0.14.0
    Checking hayro-cmap v0.1.0
    Checking atspi-common v0.13.0
    Checking photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-codecs)
    Checking utf-8 v0.7.6
    Checking ascii v1.1.0
    Checking bumpalo v3.20.3
    Checking xmp-writer v0.3.3
    Checking wasmi_collections v2.0.0
    Checking chunked_transfer v1.5.0
    Checking tungstenite v0.27.0
    Checking petgraph v0.8.3
    Checking hayro-interpret v0.7.0
    Checking smithay-client-toolkit v0.19.2
    Checking x11-dl v2.21.0
    Checking dirs v6.0.0
    Checking io-lifetimes v2.0.4
    Checking io-lifetimes v3.0.1
    Checking photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-format)
    Checking xkbcommon-dl v0.4.2
    Checking tiny_http v0.12.0
    Checking subsetter v0.2.6
    Checking wayland-protocols-plasma v0.3.12
    Checking accesskit_consumer v0.36.0
    Checking calloop v0.14.4
    Checking photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-paint)
    Checking krilla v0.8.2
    Checking photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-raw)
    Checking socket2 v0.6.5
    Checking mio v1.2.3
    Checking dpi v0.1.2
    Checking tract-nnef v0.23.8
    Checking tract-hir v0.23.8
    Checking smol_str v0.2.2
    Checking bitflags v1.3.2
    Checking wasmi v2.0.0
    Checking tokio v1.53.1
    Checking png v0.17.16
    Checking ref-cast v1.0.27
    Checking calloop-wayland-source v0.4.1
    Checking wgpu v30.0.1
    Checking tree_magic_mini v3.2.2
    Checking io-extras v0.19.0
    Checking fs-set-times v0.20.3
    Checking photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vecfx)
    Checking photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-svg)
    Checking photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-affinity)
    Checking futures-executor v0.3.34
    Checking wayland-protocols-experimental v20250721.0.1
    Checking wayland-protocols-misc v0.3.12
    Checking atspi-proxies v0.13.0
    Checking accesskit_atspi_common v0.18.1
    Checking photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pdf)
    Checking pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-trace)
    Checking photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ops)
    Checking rustix-linux-procfs v0.1.1
    Checking chrono v0.4.45
    Checking os_pipe v1.2.3
    Checking ipnet v2.12.2
    Checking maybe-owned v0.3.4
    Checking ambient-authority v0.0.2
    Checking tract-extra v0.23.8
    Checking tract-transformers v0.23.8
    Checking wl-clipboard-rs v0.9.4
    Checking futures v0.3.34
    Checking tiny-skia v0.11.4
    Checking cap-primitives v4.0.3
    Checking smithay-client-toolkit v0.20.0
    Checking tract-onnx-opl v0.23.8
    Checking gemm-f32 v0.19.0
    Checking typed-path v0.12.3
    Checking enum-map v2.7.3
    Checking atspi v0.29.0
    Checking schemars v1.2.2
    Checking gemm-f64 v0.19.0
    Checking accesskit_unix v0.21.1
    Checking gemm-c32 v0.19.0
    Checking cap-std v4.0.3
    Checking gemm-c64 v0.19.0
    Checking gemm-f16 v0.19.0
    Checking type-map v0.5.1
    Checking li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-attributions)
    Checking float8 v0.6.1
    Checking safetensors v0.7.0
    Checking num_cpus v1.17.0
    Checking is-docker v0.2.0
    Checking smithay-clipboard v0.7.3
    Checking resvg v0.45.1
    Checking pollster v0.4.0
    Checking photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-gpu)
    Checking tract-onnx v0.23.8
    Checking toml_parser v1.1.3+spec-1.1.0
    Checking is-wsl v0.4.0
    Checking zip v7.2.0
    Checking serde_spanned v1.1.1
    Checking tokio-util v0.7.19
    Checking toml_datetime v1.1.1+spec-1.1.0
    Checking rfd v0.17.2
    Checking toml_writer v1.1.2+spec-1.1.0
    Checking sys-locale v0.3.2
    Checking gemm v0.19.0
    Checking open v5.4.4
    Checking egui_extras v0.36.2
    Checking lightcraft-fetch v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-fetch)
    Checking heic-rs v0.1.1
    Checking memory-stats v1.2.0
    Checking lightcraft-sysmem v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-sysmem)
    Checking x11rb v0.13.2
    Checking toml v1.1.6+spec-1.1.0
    Checking rmcp v3.5.0
    Checking photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-testkit)
    Checking candle-core v0.9.2
    Checking xtask v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/xtask)
    Checking photocraft-heif v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-heif)
    Checking winit v0.30.13
    Checking arboard v3.6.1
    Checking photocraft-tablet v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-tablet)
warning: fields `tool`, `label`, and `recognition` are never read
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
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `xtask` (bin "xtask") generated 1 warning
    Checking photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-plugins)
    Checking li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-seg)
    Checking candle-nn v0.9.2
    Checking lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-gpu)
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-ai)
    Checking accesskit_winit v0.32.2
    Checking egui-wgpu v0.36.2
    Checking egui-winit v0.36.2
    Checking lightcraft-segment v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-segment)
    Checking eframe v0.36.2
    Checking lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-engine)
    Checking photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-io)
    Checking lightcraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-ui-egui)
    Checking lightcraft-mcp v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-mcp)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-engine)
    Checking photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-automation)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ui-egui)
    Checking local-image-cli v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/apps/local-image-cli)
    Checking local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/apps/local-image)
    Finished `dev` profile [optimized + debuginfo] target(s) in 42.79s

```

</details>

### Strict Clippy on touched crates

```bash
cargo clippy --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-io -p local-image --all-targets -- -D warnings > /tmp/codex-hotfix-critical-logs/clippy.log 2>&1
```

Exit code: `0`.

<details>
<summary>Captured output</summary>

```text
    Checking cfg-if v1.0.5
    Checking log v0.4.34
    Checking once_cell v1.21.4
    Checking memchr v2.8.3
    Checking either v1.18.0
    Checking serde_core v1.0.229
    Checking libc v0.2.189
    Checking libm v0.2.16
    Checking crossbeam-utils v0.8.23
    Checking linux-raw-sys v0.12.1
    Checking foldhash v0.2.0
   Compiling syn v2.0.119
    Checking pin-project-lite v0.2.17
    Checking bytemuck v1.25.2
    Checking thiserror v2.0.21
    Checking simd-adler32 v0.3.10
    Checking equivalent v1.0.2
   Compiling generic-array v0.14.7
    Checking typenum v1.20.1
    Checking adler2 v2.0.1
    Checking libloading v0.8.9
    Checking itoa v1.0.18
    Checking hashbrown v0.17.1
    Checking subtle v2.6.1
    Checking crc32fast v1.5.2
    Checking tracing-core v0.1.36
    Checking rand_core v0.10.1
    Checking zmij v1.0.23
    Checking crossbeam-epoch v0.9.21
    Checking miniz_oxide v0.9.1
    Checking dlib v0.5.3
    Checking unicode-ident v1.0.26
    Checking allocator-api2 v0.2.21
    Checking miniz_oxide v0.8.9
    Checking const-oid v0.9.6
    Checking scopeguard v1.2.0
    Checking getrandom v0.4.3
    Checking wayland-sys v0.31.11
    Checking stable_deref_trait v1.2.1
    Checking nom v8.0.0
    Checking zune-core v0.5.3
    Checking fdeflate v0.3.7
    Checking lock_api v0.4.14
    Checking flate2 v1.1.10
    Checking num-traits v0.2.19
    Checking spin v0.9.9
    Checking quick-error v2.0.1
    Checking crossbeam-deque v0.8.8
    Checking hashbrown v0.16.1
    Checking anyhow v1.0.104
    Checking weezl v0.1.12
    Checking zune-jpeg v0.5.15
    Checking cpufeatures v0.2.17
    Checking pxfm v0.1.30
    Checking typeid v1.0.3
    Checking memmap2 v0.9.11
    Checking itertools v0.15.0
    Checking rayon-core v1.13.0
    Checking color_quant v1.1.0
    Checking byteorder-lite v0.1.0
    Checking serde v1.0.229
    Checking bitflags v2.13.2
    Checking serde_json v1.0.151
    Checking indexmap v2.14.2
    Checking zerofrom v0.1.8
    Checking lazy_static v1.5.0
    Checking gif v0.14.2
    Checking static_assertions v1.1.0
    Checking image-webp v0.2.4
    Checking rustix v1.1.5
    Checking png v0.18.1
    Checking yoke v0.8.3
    Checking num-integer v0.1.47
    Checking num-complex v0.4.6
    Checking smallvec v1.16.2
    Checking arrayvec v0.7.8
    Checking rayon v1.12.0
    Checking photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-geom)
    Checking qoi v0.4.1
    Checking siphasher v1.0.4
    Checking parking_lot_core v0.9.12
    Checking font-types v0.12.5
    Checking polycool v0.4.0
    Checking getrandom v0.3.4
    Checking unicode-width v0.2.2
    Checking linebender_resource_handle v0.1.1
    Checking parking_lot v0.12.5
    Checking raw-window-handle v0.6.2
    Checking rawpointer v0.2.1
    Checking kurbo v0.13.1
    Checking dyn-clone v1.0.20
    Checking zerovec v0.11.8
    Checking read-fonts v0.41.0
    Checking rustc-hash v1.1.0
    Checking matrixmultiply v0.3.11
    Checking lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-geom)
    Checking string-interner v0.20.0
    Checking downcast-rs v2.0.2
    Checking bit-vec v0.9.1
    Checking naga-types v30.0.1
    Checking maplit v1.0.2
    Checking dyn-hash v1.0.0
    Checking moxcms v0.8.1
    Checking strength_reduce v0.2.4
    Checking ndarray v0.17.2
    Checking inventory v0.3.25
    Checking bytes v1.12.1
    Checking writeable v0.6.4
    Checking bit-set v0.10.0
    Checking codespan-reporting v0.13.1
    Checking num-bigint v0.4.8
    Checking nom-language v0.1.0
    Checking primal-check v0.3.4
    Checking transpose v0.2.3
    Checking spirv v0.4.0+sdk-1.4.341.0
    Checking uuid v1.26.1
    Checking bit-vec v0.10.1
    Checking byteorder v1.5.0
    Checking rustfft v6.4.1
    Checking lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-color)
    Checking ash v0.38.0+1.3.281
    Checking bit-set v0.11.1
    Checking erased-serde v0.4.10
    Checking wgpu-types v30.0.1
    Checking cpufeatures v0.3.1
    Checking anymap3 v1.1.0
    Checking tinystr v0.8.4
    Checking litemap v0.8.3
    Checking presser v0.3.1
    Checking alloc-no-stdlib v2.0.4
    Checking photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-cms)
    Checking num-rational v0.4.2
    Checking potential_utf v0.1.6
    Checking khronos-egl v6.0.0
    Checking icu_locale_core v2.3.0
    Checking alloc-stdlib v0.2.4
    Checking zerotrie v0.2.5
    Checking ordered-float v5.5.0
    Checking filetime v0.2.29
    Checking getrandom v0.2.17
    Checking glow v0.17.0
    Checking same-file v1.0.6
    Checking renderdoc-sys v1.1.0
    Checking rand_core v0.6.4
    Checking brotli-decompressor v5.0.3
    Checking walkdir v2.5.0
    Checking utf8_iter v1.0.4
    Checking lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-raster)
    Checking chacha20 v0.10.2
    Checking icu_collections v2.3.0
    Checking base64ct v1.8.3
    Checking as-slice v0.2.1
    Checking fearless_simd v0.4.1
    Checking photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-color)
    Checking rand v0.10.3
    Checking aligned v0.4.3
    Checking pem-rfc7468 v0.7.0
    Checking icu_locale_fallback_data v2.3.0
    Checking icu_properties_data v2.3.0
    Checking no_std_io2 v0.9.4
    Checking rgb v0.8.53
    Checking web-time v1.1.0
    Checking icu_provider v2.3.1
    Checking float-ord v0.3.2
    Checking roxmltree v0.20.0
    Checking y4m v0.8.0
    Checking bitstream-io v4.10.0
    Checking icu_normalizer_data v2.3.0
    Checking photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-raster)
    Checking icu_segmenter_data v2.3.0
    Checking icu_properties v2.3.0
    Checking icu_locale_fallback v2.3.0
    Checking xattr v1.6.1
    Checking maybe-rayon v0.1.1
    Checking lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-tiff)
    Checking quick-xml v0.38.4
    Checking itertools v0.14.0
    Checking rand_distr v0.6.0
    Checking photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-doc)
    Checking imgref v1.12.3
    Checking new_debug_unreachable v1.0.6
    Checking tar v0.4.46
    Checking slab v0.4.12
    Checking parlance v0.1.0
    Checking regex-syntax v0.8.11
    Checking loop9 v0.1.5
    Checking icu_segmenter v2.3.0
    Checking icu_normalizer v2.3.0
    Checking lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-develop)
    Checking rand_core v0.9.5
    Checking avif-serialize v0.8.9
    Checking aho-corasick v1.1.5
    Checking lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-meta)
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-text)
    Checking fastrand v2.5.0
    Checking pollster v1.0.1
    Checking jpeg-encoder v0.7.1
    Checking rustc-hash v2.1.3
    Checking jpeg-decoder v0.3.2
    Checking moxcms v0.9.1
    Checking raw-cpuid v11.6.0
    Checking parley_data v0.11.1
    Checking photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-psd)
    Checking reborrow v0.5.5
    Checking pulp-wasm-simd-flag v0.1.1
    Checking futures-core v0.3.34
    Checking ttf-parser v0.25.1
    Checking lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-scenes)
    Checking float-cmp v0.9.0
    Checking zune-inflate v0.2.54
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-engine)
    Checking lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-raw)
    Checking twox-hash v2.1.4
    Checking regex-automata v0.4.18
    Checking hex v0.4.3
    Checking lebe v0.5.3
    Checking bit_field v0.10.3
    Checking ab_glyph_rasterizer v0.1.10
    Checking strict-num v0.1.1
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking ruzstd v0.9.0
    Checking lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-preview)
    Checking photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-algo)
    Checking arrayref v0.3.9
    Checking constant_time_eq v0.4.2
    Checking percent-encoding v2.3.2
    Checking jpeg-encoder v0.6.1
    Checking blake3 v1.8.7
    Checking phf_shared v0.13.1
    Checking lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-pipeline)
    Checking lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-catalog)
    Checking scoped-tls v1.0.1
    Checking pulp v0.22.3
    Checking downcast-rs v1.2.1
    Checking photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-paint)
    Checking polling v3.11.0
    Checking wayland-backend v0.3.17
    Checking ff v0.13.1
    Checking photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-raw)
    Checking futures-io v0.3.34
    Checking skrifa v0.44.0
    Checking harfrust v0.12.0
    Checking fontique v0.11.1
    Checking regex v1.13.1
    Checking owned_ttf_parser v0.25.1
    Checking ab_glyph v0.2.32
   Compiling semver v1.0.28
    Checking base64 v0.22.1
    Checking base16ct v0.2.0
   Compiling rustc_version v0.4.1
    Checking group v0.13.0
    Checking lensfun v0.7.0
    Checking font-types v0.11.3
    Checking opaque-debug v0.3.1
    Checking parking v2.2.1
    Checking wayland-client v0.31.15
    Checking tiny-skia-path v0.11.4
    Checking kurbo v0.11.3
    Checking photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vector)
    Checking read-fonts v0.39.2
    Checking untrusted v0.9.0
    Checking simplecss v0.2.2
   Compiling rustls v0.23.45
    Checking xmlwriter v0.1.0
   Compiling curve25519-dalek v4.1.3
   Compiling num-bigint-dig v0.8.6
    Checking pico-args v0.5.0
    Checking imagesize v0.13.0
    Checking data-url v0.3.2
    Checking parley v0.11.1
    Checking httparse v1.10.1
    Checking linesweeper v0.4.0
    Checking http v1.5.0
    Checking svgtypes v0.15.3
    Checking num-iter v0.1.46
    Checking hayro-ccitt v0.3.0
    Checking bumpalo v3.20.3
    Checking hayro-jpeg2000 v0.3.5
    Checking brotli v8.0.4
    Checking wasmi_core v2.0.0
    Checking usvg v0.45.1
    Checking proc-macro2 v1.0.107
    Checking hayro-jbig2 v0.3.0
    Checking hayro-postscript v0.1.0
    Checking ryu v1.0.23
    Checking photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-compose)
    Checking winnow v1.0.4
    Checking option-ext v0.2.0
    Checking base64 v0.23.1
    Checking pdf-writer v0.15.0
    Checking wasmi_ir v2.0.0
    Checking quote v1.0.47
    Checking dirs-sys v0.5.0
    Checking ureq-proto v0.6.4
    Checking hayro-syntax v0.7.2
    Checking wayland-protocols v0.32.13
    Checking wasmparser v0.228.0
    Checking event-listener v5.4.2
    Checking photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pathops)
    Checking tiny-skia-path v0.12.0
    Checking concurrent-queue v2.5.0
    Checking ascii v1.1.0
    Checking httpdate v1.0.3
    Checking utf-8 v0.7.6
    Checking xmp-writer v0.3.3
    Checking hayro-cmap v0.1.0
    Checking wasmi_collections v2.0.0
    Checking imagesize v0.14.0
    Checking gpu-allocator v0.28.0
    Checking chunked_transfer v1.5.0
    Checking utf8-zero v0.8.1
    Checking data-encoding v2.11.1
    Checking tiny_http v0.12.0
    Checking event-listener-strategy v0.5.4
    Checking syn v3.0.6
    Checking dirs v6.0.0
    Checking futures-lite v2.6.1
    Checking pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-trace)
    Checking photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ops)
    Checking zcheapstr v1.1.0
    Checking endi v1.1.1
    Checking errno v0.3.14
    Checking async-task v4.7.1
    Checking async-channel v2.5.0
    Checking signal-hook-registry v1.4.8
    Checking euclid v0.22.14
    Checking photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-vecfx)
    Checking color v0.3.3
    Checking atomic-waker v1.1.2
    Checking futures-sink v0.3.34
    Checking piper v0.2.5
    Checking async-lock v3.4.2
    Checking async-broadcast v0.7.2
    Checking ordered-stream v0.2.0
    Checking async-io v2.6.0
    Checking futures-channel v0.3.34
    Checking async-executor v1.14.0
    Checking peniko v0.6.1
    Checking guillotiere v0.7.0
    Checking blocking v1.7.0
    Checking emath v0.36.2
    Checking xkeysym v0.2.1
    Checking futures-task v0.3.34
    Checking async-signal v0.2.14
    Checking xcursor v0.3.11
    Checking cursor-icon v1.2.0
    Checking vello_common v0.1.0
    Checking linux-raw-sys v0.4.15
    Checking futures-util v0.3.34
    Checking unicode-general-category v1.1.0
    Checking wayland-csd-frame v0.3.0
    Checking async-process v2.5.0
    Checking wayland-cursor v0.31.14
    Checking gethostname v1.1.0
    Checking as-raw-xcb-connection v1.0.1
    Checking rustix v0.38.44
    Checking nohash-hasher v0.2.0
    Checking self_cell v1.3.0
    Checking unicode-segmentation v1.13.3
    Checking x11rb-protocol v0.13.2
    Checking epaint_default_fonts v0.36.2
    Checking ecolor v0.36.2
    Checking ron v0.12.2
    Checking foldhash v0.1.5
    Checking wayland-protocols-wlr v0.3.12
    Checking fixedbitset v0.5.7
    Checking x11-dl v2.21.0
    Checking wasmi v2.0.0
    Checking vello_cpu v0.1.0
    Checking hashbrown v0.15.5
    Checking wayland-protocols-plasma v0.3.12
    Checking xkbcommon-dl v0.4.2
    Checking tempfile v3.27.0
    Checking smol_str v0.2.2
    Checking dpi v0.1.2
    Checking zvariant_utils v4.2.0
    Checking wayland-protocols-misc v0.3.12
    Checking petgraph v0.8.3
    Checking wayland-protocols-experimental v20250721.0.1
    Checking os_pipe v1.2.3
    Checking bitflags v1.3.2
    Checking skrifa v0.42.1
    Checking write-fonts v0.48.1
    Checking png v0.17.16
   Compiling li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-attributions)
    Checking type-map v0.5.1
    Checking is-docker v0.2.0
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-ui-egui)
    Checking pollster v0.4.0
    Checking is-wsl v0.4.0
    Checking rfd v0.17.2
    Checking sys-locale v0.3.2
    Checking toml_parser v1.1.3+spec-1.1.0
    Checking tiny-skia v0.11.4
    Checking open v5.4.4
    Checking getopts v0.2.24
    Checking colored v2.2.0
    Checking toml_datetime v1.1.1+spec-1.1.0
    Checking serde_spanned v1.1.1
    Checking memory-stats v1.2.0
    Checking leb128fmt v0.1.0
    Checking wait-timeout v0.2.1
    Checking bit-vec v0.8.0
    Checking tree_magic_mini v3.2.2
    Checking toml v1.1.6+spec-1.1.0
    Checking fnv v1.0.7
    Checking quick-error v1.2.3
    Checking wasm-encoder v0.261.0
    Checking rand_xorshift v0.4.0
    Checking unarray v0.1.4
    Checking io-lifetimes v3.0.1
    Checking rusty-fork v0.3.1
    Checking bit-set v0.8.0
    Checking io-lifetimes v2.0.4
    Checking socket2 v0.6.5
    Checking mio v1.2.3
    Checking wl-clipboard-rs v0.9.4
    Checking ref-cast v1.0.27
    Checking io-extras v0.19.0
    Checking fs-set-times v0.20.3
    Checking futures-executor v0.3.34
    Checking rustix-linux-procfs v0.1.1
    Checking chrono v0.4.45
    Checking maybe-owned v0.3.4
    Checking ipnet v2.12.2
    Checking ambient-authority v0.0.2
    Checking futures v0.3.34
   Compiling lightcraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-ui-egui)
   Compiling local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/apps/local-image)
    Checking tokio v1.53.1
    Checking resvg v0.45.1
    Checking cap-primitives v4.0.3
    Checking schemars v1.2.2
    Checking cap-std v4.0.3
    Checking wast v261.0.0
    Checking subsetter v0.2.6
   Compiling zerocopy-derive v0.8.59
   Compiling zeroize_derive v1.5.0
   Compiling tracing-attributes v0.1.31
   Compiling fax_derive v0.2.0
   Compiling profiling-procmacros v1.0.18
   Compiling derive-new v0.7.0
   Compiling equator-macro v0.4.2
   Compiling prost-derive v0.14.4
   Compiling arg_enum_proc_macro v0.3.4
   Compiling num-derive v0.4.2
   Compiling phf_macros v0.13.1
   Compiling enumflags2_derive v0.7.12
   Compiling curve25519-dalek-derive v0.1.1
    Checking krilla v0.8.2
   Compiling enumn v0.1.14
    Checking profiling v1.0.18
    Checking photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-plugins)
    Checking fax v0.2.6
   Compiling thiserror-impl v1.0.69
   Compiling async-recursion v1.1.1
    Checking x11rb v0.13.2
   Compiling enum-map-derive v0.17.0
    Checking wat v1.261.0
   Compiling enumflags2 v0.7.12
    Checking phf v0.13.1
   Compiling zvariant v5.15.0
    Checking tokio-util v0.7.19
    Checking accesskit v0.24.1
    Checking hayro-interpret v0.7.0
    Checking equator v0.4.2
    Checking aligned-vec v0.6.4
    Checking v_frame v0.3.9
    Checking av-scenechange v0.14.1
    Checking av1-grain v0.2.5
    Checking photocraft-tablet v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-tablet)
    Checking zbus_names v4.3.4
    Checking zeroize v1.9.1
    Checking accesskit_consumer v0.36.0
    Checking accesskit_consumer v0.35.0
    Checking der v0.7.10
    Checking rav1e v0.8.1
    Checking rustls-pki-types v1.15.1
    Checking crypto-common v0.1.7
    Checking block-buffer v0.10.4
    Checking inout v0.1.4
    Checking digest v0.10.7
    Checking cipher v0.4.4
    Checking spki v0.7.3
    Checking sha2 v0.10.9
    Checking hmac v0.12.1
    Checking signature v2.2.0
    Checking pkcs5 v0.7.1
    Checking hkdf v0.12.4
    Checking universal-hash v0.5.1
    Checking crypto-bigint v0.5.5
    Checking rfc6979 v0.4.0
    Checking polyval v0.6.2
    Checking pkcs8 v0.10.2
    Checking rustls-webpki v0.103.15
    Checking aead v0.5.2
    Checking ghash v0.5.1
    Checking sec1 v0.7.3
    Checking pkcs1 v0.7.5
    Checking ed25519 v2.2.3
    Checking poly1305 v0.8.0
    Checking ctr v0.9.2
    Checking aes v0.8.4
    Checking tracing v0.1.44
    Checking chacha20 v0.9.1
    Checking ed25519-dalek v2.2.0
    Checking prost v0.14.4
    Checking x25519-dalek v2.0.1
    Checking jxl-bitstream v1.1.0
    Checking jxl-grid v0.6.2
    Checking jxl-threadpool v1.0.0
    Checking elliptic-curve v0.13.8
    Checking sha1 v0.10.7
    Checking rustls-webpki v0.102.8
    Checking chacha20poly1305 v0.10.1
    Checking aes-gcm v0.10.3
    Checking webpki-roots v1.0.9
   Compiling zbus_xml v5.2.1
    Checking jxl-oxide-common v1.0.0
    Checking jxl-coding v1.0.1
    Checking calloop v0.14.4
    Checking ecdsa v0.16.9
    Checking primeorder v0.13.6
    Checking enum-map v2.7.3
    Checking kittest v0.4.0
    Checking jxl-image v0.13.0
    Checking jxl-modular v0.11.3
    Checking rmcp v3.5.0
    Checking p384 v0.13.1
    Checking p256 v0.13.2
    Checking jxl-color v0.11.0
    Checking zbus-lockstep v0.5.2
   Compiling zbus_macros v5.19.0
    Checking calloop-wayland-source v0.4.1
    Checking ravif v0.13.0
    Checking zerocopy v0.8.59
    Checking smithay-client-toolkit v0.20.0
    Checking thiserror v1.0.69
    Checking jxl-vardct v0.11.1
    Checking calloop v0.13.0
    Checking calloop-wayland-source v0.3.0
    Checking jxl-frame v0.13.3
    Checking smithay-client-toolkit v0.19.2
   Compiling zbus-lockstep-macros v0.5.2
    Checking smithay-clipboard v0.7.3
    Checking ureq v3.4.2
    Checking jxl-jbr v0.2.1
    Checking jxl-render v0.12.4
   Compiling half v2.7.1
    Checking jxl-oxide v0.12.6
    Checking ppv-lite86 v0.2.21
    Checking ahash v0.8.12
   Compiling tract-linalg v0.23.8
    Checking epaint v0.36.2
    Checking winit v0.30.13
    Checking tiff v0.11.3
    Checking tract-data v0.23.8
    Checking naga v30.0.1
    Checking exr v1.74.2
    Checking rand_chacha v0.9.0
    Checking rand_chacha v0.3.1
    Checking rand v0.8.8
    Checking rand v0.9.5
    Checking tungstenite v0.27.0
    Checking zbus v5.19.0
    Checking proptest v1.11.0
    Checking image v0.25.10
    Checking egui v0.36.2
    Checking rsa v0.9.10
    Checking rustls-rustcrypto v0.0.2-alpha
    Checking lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-codecs)
    Checking photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-codecs)
    Checking photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-affinity)
    Checking photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-svg)
    Checking photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-pdf)
    Checking arboard v3.6.1
    Checking dify v0.8.0
    Checking photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-testkit)
    Checking lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-merge)
    Checking tract-core v0.23.8
    Checking photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-format)
    Checking atspi-common v0.13.0
    Checking wgpu-naga-bridge v30.0.1
    Checking wgpu-hal v30.0.1
    Checking egui_extras v0.36.2
    Checking atspi-proxies v0.13.0
    Checking accesskit_atspi_common v0.18.1
    Checking wgpu-core-deps-windows-linux-android v30.0.1
    Checking wgpu-core v30.0.1
    Checking atspi v0.29.0
    Checking accesskit_unix v0.21.1
    Checking accesskit_winit v0.32.2
    Checking egui-winit v0.36.2
    Checking wgpu v30.0.1
    Checking photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-gpu)
    Checking egui-wgpu v0.36.2
    Checking eframe v0.36.2
    Checking egui_kittest v0.36.2
    Checking tract-nnef v0.23.8
    Checking tract-hir v0.23.8
    Checking tract-extra v0.23.8
    Checking tract-transformers v0.23.8
    Checking tract-onnx-opl v0.23.8
    Checking tract-onnx v0.23.8
    Checking li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-seg)
    Checking lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/lc-gpu)
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/li-ai)
    Checking photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-io)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-engine)
    Checking photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-hotfix-critical/crates/pc-automation)
    Finished `dev` profile [optimized + debuginfo] target(s) in 1m 31s

```

</details>

### Format check

```bash
cargo fmt --all -- --check > /tmp/codex-hotfix-critical-logs/fmt-check.log 2>&1
```

Exit code: `0`.

No output.

### Toolchain and environment

```text
$ rustc --version
rustc 1.98.1 (48a229cea 2026-09-01) (Fedora 1.98.1-1.fc44)
$ cargo --version
cargo 1.98.1 (797e8a9bc 2026-08-05) (Fedora 1.98.1-1.fc44)
$ cargo clippy --version
clippy 0.1.98 (48a229ceae 2026-09-01)
$ cargo fmt --version
rustfmt 1.9.0-stable (48a229ceae 2026-09-01)
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate
CARGO_BUILD_JOBS=(unset)
```

### Final whitespace verification

```bash
git diff --check
```

Exit code: `0`. No output.
