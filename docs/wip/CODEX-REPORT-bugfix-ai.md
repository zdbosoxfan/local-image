# AI final-review bug fixes

Continued `wip/codex-bugfix-ai` at `c8ca0b63`, retaining the existing implementation and the owner's task-file edit. The initial working-tree edit was `docs/wip/CODEX-TASK.md`. Already landed hotfixes and known-test fixes were not redone. Changes are uncommitted.

| Item | Verdict after current-code re-check | Fix | Regression coverage |
|---|---|---|---|
| F06 | Real: a callback unwind leaves the detached watcher updating the job; normal completion waits for its polling sleep. | Scoped watcher; a completion-channel Drop guard wakes it on return or unwind; scope joins it before returning. | `final_review_bridge_stops_on_unwind_and_preserves_results`: unwind/sentinel, unchanged result bytes, cancellation forwarding; existing AI command tests. |
| F07 | Real: results and cached verdicts can be published after model/inventory changes. The hash cache also misses a same-size rewrite with restored mtime. | Normalize the initial loader inventory, then capture its generation and file identity, check identity before submitting and before publishing, and rehash without the cache before publication. Unix file identity includes device/inode/ctime as well as size/mtime. Errors remain Unchecked, without saving a stale verdict/default. | Before cached publication and after prompt submission file swaps; inventory replacement after submission; same-size/restored-mtime rewrite; unchanged first refresh accepted; uploads ignored while weights and node schemas stay guarded; existing refine discovery/cache tests. |
| F12 | Real: Template/Universal recipes discard styles, and incompatible Family styles are silently filtered. | Validate the resolved recipe. Family recipes preserve compatible and unidentified styles and explicitly reject incompatible styles. Template/Universal recipes currently reject selected styles before submission. Picker capability follows the verified recipe, explains unsupported styles, and offers explicit clearing. Validate both passes before submitting the draft. | Family/Template/Universal × compatible/incompatible/unidentified graph cases; standalone Refine and RefineStep service rejection with zero prompts; existing compatible service/picker cases; headless unsupported-picker click. |
| F13 | Real: UI snapshots requested numbers before saved recipe resolution; numeric equality guesses whether values are explicit. | Carry explicit default-selection flags. Resolve sampling once on the operation worker; return effective parameters with pixels and merge them into both generation-history paths. Replay keeps explicit sampling values and restores the resolved second pass. Older serialized numbers remain explicit. | Explicit numbers equal to nominal defaults; cold/saved Template and Universal defaults; custom numbers; RefineStep defaults/options against submitted graphs; UI keeps unset second-pass sampling for worker resolution and marks typed nominal values explicit; legacy saved values remain explicit; persisted history pixels/JSON and explicit replay, including the second pass. |
| O-024 | The original ordinary-photo-selection case is already correct: the job captures its original photo ID. Residual bugs are real on library/catalog replacement and same-ID source replacement. | Cancel and discard running **and completed** jobs on library close/open; check source identity and cancellation before applying outcomes. Retain the existing Denoise source check. | Replaced-photo rejection; opening/closing discards jobs and cancels their controls; deterministic original-target selection test; existing Remove/Regenerate/Denoise tests. |
| O-044 | Real: transferred masks retain source `seg`/`detail` buffers. | Strip photo-bound cached selections while retaining mask intent, clicks/text, adjustments and manual geometry. Recompute quick Subject/Background/Sky/Depth and available SAM Object/Prompt selections for each target in Paste, Previous, Sync, Auto Sync and presets. Preserve target-owned preset masks and deduplicate adaptive presets by selection intent. | All cached mask variants and manual geometry/native round trip; Paste/Sync/Auto Sync/imported preset target masks; repeated adaptive preset preserves one target mask and its cache; existing Develop/session tests. |
| RealVisXL V5 Lightning | Real: installed Lightning files inherit generic SDXL 30-step/CFG-6/DPM++ 2M SDE settings. | Narrow RealVis V5 Lightning filename overrides: five steps, CFG 1.5, DPM++ SDE Karras. Other SDXL/Lightning models retain their profiles; controls remain editable. | Two installed author filename forms, resolved catalog recommendations and actual KSampler graph; negative controls for ordinary RealVis and other Lightning checkpoints. |
| Qwen Remove boundary | Real: Qwen only alpha-composites its independently coloured fill, whereas Klein uses seamless blending. | Apply the same Poisson blend used by Klein, then preserve mask coverage and exact unmasked pixels. | Mock Qwen edit with a colour offset and a hard rectangular mask: boundary jump and every unmasked pixel; existing Poisson detail/edge and Qwen operation tests. |

The Lightning recipe follows the [model author's Hugging Face card](https://huggingface.co/SG161222/RealVisXL_V5.0_Lightning/blob/main/README.md): five steps, DPM++ SDE or DPM++ SDE Karras, CFG 1–2. The default here uses CFG 1.5 and Karras. This is a model-specific override, not a general rule for distilled SDXL files.

Mask transfer follows [Adobe's Lightroom Classic masking help](https://helpx.adobe.com/be_en/lightroom-classic/desktop/process-and-develop-photos/masking.html), which describes recomputing AI masks for target photos through copy/paste, Sync and Auto Sync. Without installed quick models, existing classical Subject/Sky/Background behavior remains available; missing SAM/depth models leave the incoming selection uncomputed rather than carrying foreign pixels. Existing target masks and native serialization retain their own cached selections.

TEST-CHANGED: `li-ai::mock::tests::refine_step_loras_reach_the_comfy_graph_and_validate_the_refine_family` previously expected an incompatible selected style to be silently filtered and the job to succeed. F12 requires explicit rejection. The test now checks the incompatibility error and unchanged prompt count, while retaining compatible strength/CLIP wiring, invalid-strength, legacy-deserialization and Qwen coverage. No tests were removed.

Changes outside the standalone AI backend are limited to its progress bridge, generation/picker/history consumers, library job lifecycle, and Develop mask transfer callers/helpers. New English messages are listed in `pc-ui-egui/src/i18n/mod.rs`'s pending AI translations.

The checker excludes temporary uploaded-image choices from the server inventory signature. [ComfyUI's official LoadImage implementation](https://github.com/Comfy-Org/ComfyUI/blob/master/nodes.py) builds those choices from its input directory, so the check's own source upload changes them. Model choices and the remaining node schemas are still compared. A regression covers upload changes, checkpoint choices and a removed decoder node.

Publication identity is checked locally; this cannot prove which cached bytes a remote ComfyUI loader executed. That stronger guarantee needs immutable/versioned weights or coordination with the loader. Tests operate on tiny isolated fixtures and mock servers and never modify installed weights.

A read-only `GET http://127.0.0.1:8188/queue` returned connection refused. No real prompts were submitted and no AI service was started or stopped. Fresh real-model aesthetic assessment was not performed after this failed queue probe; the fixes are validated with graph, pixel and mock-service regressions rather than a claim of new real-model quality results. No GUI windows were opened.

## Validation

Rust/Cargo 1.98.1; every Cargo build/test/check/Clippy invocation uses `--offline` and the configured absolute `li-rustc-gate` wrapper. `CARGO_BUILD_JOBS` is unset. Post-fix runtime tests use a separate `/tmp/codex-bugfix-ai-test-profile` and model directory so mock checker state does not enter the application profile.

The full run passed all existing AI tests (154 passing in that run, with two new fixture failures and two existing ignored tests), all 34 Develop tests, and 457 library tests (four existing ignored). After fixing the new template fixtures, all 12 AI regressions passed. All eight existing/new AI bridge tests passed. The initial headless UI run passed 72 tests and exposed a new inventory-guard false positive during the first UI refresh. The focused first-refresh fixture initially declared no target; after correcting its initial catalog, it reproduced that rejection before normalization; the checker now captures a normalized initial snapshot. Actual inventory replacement still invalidates it. A new keyboard regression then exposed that the shared number widget reports only numeric changes; generation controls now track focused text/paste input as explicit provenance, including unchanged numbers, without changing the shared widget. Its original assertions remain intact. The second-pass controls also use the widget edit response to record typed nominal values as explicit. Final UI and compiler results are recorded below.

Final gates: **PASS**. The final AI/refine/graph run passed 31 tests, the final generation UI/i18n run passed all 74 tests, and the current workspace check and strict Clippy over all five touched crates and all targets passed. Develop (34), library (457; four ignored), and AI bridge (8) suites passed as recorded above. Formatting and whitespace checks passed. Additional runs were limited to new changes, failures and the affected files; tests were not removed or weakened.

Before-fix testing reproduced every assigned defect; the service-style fixture error and the ordinary-selection fixture timeout are tracked separately below. The first compile caught a new fixture cloning `Arc<Photo>` instead of `Photo`; this was corrected before the reproduction run. The additional ordinary-selection test's asynchronous demo rendering exceeded its five-second fixture deadline under concurrent builds; its post-fix fixture uses a completed mock outcome to test target binding deterministically. The behavior assertions were retained. The first post-fix build was stopped before any tests ran and restarted with the isolated profile. The full AI run then caught two new template fixtures declaring a different checkpoint from their installed target; they were corrected to use that target’s API graph without changing assertions. The initial UI test compile caught two new fixtures assuming an app Default constructor; they now use the existing headless constructor with the assertions unchanged. Strict Clippy found a redundant f64-to-f64 conversion in a new assertion; the conversion was removed without changing the comparison. Review of the second-pass UI also found and fixed premature conversion of unset steps/guidance into explicit numbers; its regression checks both worker resolution and explicit history replay.

Validation results and captured output follow. Build warnings about missing optional `craft-fonts` are pre-existing. The external `li-test-runner` emitted process-exit arithmetic diagnostics after the library suite had reported its passing result; Cargo identified only the two AI fixture failures as a failing target. The wrapper was not edited.

<details>
<summary>Toolchain and environment — exit 0</summary>

```bash
rustc --version
cargo --version
# Inspect RUSTC_WRAPPER and CARGO_BUILD_JOBS
```

```text
$ rustc --version
rustc 1.98.1 (48a229cea 2026-09-01) (Fedora 1.98.1-1.fc44)
exit 0
$ cargo --version
cargo 1.98.1 (797e8a9bc 2026-08-05) (Fedora 1.98.1-1.fc44)
exit 0
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate
CARGO_BUILD_JOBS=None
```

</details>

<details>
<summary>Read-only real-service queue probe — exit 7</summary>

```bash
curl --max-time 5 --fail --silent --show-error http://127.0.0.1:8188/queue
```

```text
curl: (7) Failed to connect to 127.0.0.1 port 8188 after 0 ms: Could not connect to server
```

</details>

<details>
<summary>Initial before-fix fixture compile — exit 101</summary>

```bash
cargo test --offline --no-fail-fast -p li-ai -p photocraft-engine -p lightcraft-engine -p lightcraft-develop --lib final_review -- --test-threads=1
```

```text
   Compiling proc-macro2 v1.0.107
   Compiling unicode-ident v1.0.26
   Compiling quote v1.0.47
   Compiling cfg-if v1.0.5
   Compiling serde_core v1.0.229
   Compiling serde v1.0.229
   Compiling autocfg v1.5.1
   Compiling libm v0.2.16
   Compiling libc v0.2.189
   Compiling memchr v2.8.3
   Compiling either v1.18.0
   Compiling version_check v0.9.5
   Compiling zerocopy v0.8.59
   Compiling bitflags v2.13.2
   Compiling crossbeam-utils v0.8.23
   Compiling crossbeam-epoch v0.9.21
   Compiling crossbeam-deque v0.8.8
   Compiling once_cell v1.21.4
   Compiling rayon-core v1.13.0
   Compiling log v0.4.34
   Compiling typenum v1.20.1
   Compiling simd-adler32 v0.3.10
   Compiling thiserror v2.0.21
   Compiling foldhash v0.2.0
   Compiling subtle v2.6.1
   Compiling adler2 v2.0.1
   Compiling itoa v1.0.18
   Compiling zmij v1.0.23
   Compiling generic-array v0.14.7
   Compiling equivalent v1.0.2
   Compiling serde_json v1.0.151
   Compiling const-oid v0.9.6
   Compiling num-traits v0.2.19
   Compiling stable_deref_trait v1.2.1
   Compiling crc32fast v1.5.2
   Compiling anyhow v1.0.104
   Compiling miniz_oxide v0.9.1
   Compiling allocator-api2 v0.2.21
   Compiling cpufeatures v0.2.17
   Compiling parking_lot_core v0.9.12
   Compiling hashbrown v0.17.1
   Compiling miniz_oxide v0.8.9
   Compiling scopeguard v1.2.0
   Compiling nom v8.0.0
   Compiling find-msvc-tools v0.1.14
   Compiling spin v0.9.9
   Compiling lock_api v0.4.14
   Compiling zune-core v0.5.3
   Compiling shlex v2.0.1
   Compiling getrandom v0.2.17
   Compiling rustix v1.1.5
   Compiling lazy_static v1.5.0
   Compiling hashbrown v0.16.1
   Compiling flate2 v1.1.10
   Compiling matrixmultiply v0.3.11
   Compiling tracing-core v0.1.36
   Compiling cc v1.5.1
   Compiling quick-error v2.0.1
   Compiling rand_core v0.6.4
   Compiling pin-project-lite v0.2.17
   Compiling num-integer v0.1.47
   Compiling linux-raw-sys v0.12.1
   Compiling zune-jpeg v0.5.15
   Compiling cfg_aliases v0.2.2
   Compiling weezl v0.1.12
   Compiling tracing v0.1.44
   Compiling base64ct v1.8.3
   Compiling memo-map v0.3.4
   Compiling same-file v1.0.6
   Compiling rayon v1.12.0
   Compiling pxfm v0.1.30
   Compiling getrandom v0.4.3
   Compiling rawpointer v0.2.1
   Compiling rand_core v0.10.1
   Compiling walkdir v2.5.0
   Compiling indexmap v2.14.2
   Compiling fdeflate v0.3.7
   Compiling color_quant v1.1.0
   Compiling photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-cms)
   Compiling pem-rfc7468 v0.7.0
   Compiling byteorder-lite v0.1.0
   Compiling typeid v1.0.3
   Compiling moxcms v0.8.1
   Compiling gif v0.14.2
   Compiling png v0.18.1
   Compiling nom-language v0.1.0
   Compiling itertools v0.15.0
   Compiling erased-serde v0.4.10
   Compiling dyn-clone v1.0.20
   Compiling inventory v0.3.25
   Compiling maplit v1.0.2
   Compiling paste v1.0.15
   Compiling downcast-rs v2.0.2
   Compiling syn v3.0.6
   Compiling syn v2.0.119
   Compiling strength_reduce v0.2.4
   Compiling dyn-hash v1.0.0
   Compiling minijinja v2.24.0
   Compiling transpose v0.2.3
   Compiling jxl-bitstream v1.1.0
   Compiling primal-check v0.3.4
   Compiling num-bigint v0.4.8
   Compiling pastey v0.2.3
   Compiling byteorder v1.5.0
   Compiling bit-vec v0.10.1
   Compiling writeable v0.6.4
   Compiling bit-set v0.11.1
   Compiling memmap2 v0.9.11
   Compiling itertools v0.14.0
   Compiling cpufeatures v0.3.1
   Compiling unicode-width v0.2.2
   Compiling siphasher v1.0.4
   Compiling num-rational v0.4.2
   Compiling anymap3 v1.1.0
   Compiling xattr v1.6.1
   Compiling image-webp v0.2.4
   Compiling jxl-grid v0.6.2
   Compiling filetime v0.2.29
   Compiling jxl-oxide-common v1.0.0
   Compiling litemap v0.8.3
   Compiling rustc-hash v1.1.0
   Compiling bytes v1.12.1
   Compiling alloc-no-stdlib v2.0.4
   Compiling jxl-coding v1.0.1
   Compiling jxl-threadpool v1.0.0
   Compiling alloc-stdlib v0.2.4
   Compiling naga v30.0.1
   Compiling ash v0.38.0+1.3.281
   Compiling bit-vec v0.9.1
   Compiling synstructure v0.14.0
   Compiling codespan-reporting v0.13.1
   Compiling chacha20 v0.10.2
   Compiling tar v0.4.46
   Compiling spirv v0.4.0+sdk-1.4.341.0
   Compiling bit-set v0.10.0
   Compiling libloading v0.8.9
   Compiling utf8_iter v1.0.4
   Compiling raw-window-handle v0.6.2
   Compiling static_assertions v1.1.0
   Compiling getrandom v0.3.4
   Compiling wgpu-hal v30.0.1
   Compiling icu_locale_fallback_data v2.3.0
   Compiling icu_properties_data v2.3.0
   Compiling presser v0.3.1
   Compiling ff v0.13.1
   Compiling serde_derive v1.0.229
   Compiling bytemuck_derive v1.12.1
   Compiling zeroize_derive v1.5.0
   Compiling zerocopy-derive v0.8.59
   Compiling thiserror-impl v2.0.21
   Compiling zerofrom-derive v0.1.8
   Compiling yoke-derive v0.8.3
   Compiling fax_derive v0.2.0
   Compiling bytemuck v1.25.2
   Compiling num-complex v0.4.6
   Compiling zerovec-derive v0.11.6
   Compiling zerofrom v0.1.8
   Compiling displaydoc v0.2.7
   Compiling zeroize v1.9.1
   Compiling qoi v0.4.1
   Compiling fax v0.2.6
   Compiling der v0.7.10
   Compiling ndarray v0.17.2
   Compiling derive-new v0.7.0
   Compiling spki v0.7.3
   Compiling rustfft v6.4.1
   Compiling profiling-procmacros v1.0.18
   Compiling equator-macro v0.4.2
   Compiling pkcs5 v0.7.1
   Compiling naga-types v30.0.1
   Compiling brotli-decompressor v5.0.3
   Compiling jxl-modular v0.11.3
   Compiling wgpu-types v30.0.1
   Compiling half v2.7.1
   Compiling crypto-common v0.1.7
   Compiling smallvec v1.16.2
   Compiling arrayvec v0.7.8
   Compiling block-buffer v0.10.4
   Compiling photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-color)
   Compiling photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-geom)
   Compiling tract-linalg v0.23.8
   Compiling lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-geom)
   Compiling string-interner v0.20.0
   Compiling tiff v0.11.3
   Compiling parking_lot v0.12.5
   Compiling lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-color)
   Compiling image v0.25.10
   Compiling profiling v1.0.18
   Compiling pkcs8 v0.10.2
   Compiling digest v0.10.7
   Compiling ppv-lite86 v0.2.21
   Compiling equator v0.4.2
   Compiling photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raster)
   Compiling sha2 v0.10.9
   Compiling hmac v0.12.1
   Compiling polycool v0.4.0
   Compiling aligned-vec v0.6.4
   Compiling photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-doc)
   Compiling lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raster)
   Compiling font-types v0.12.5
   Compiling rand v0.10.3
   Compiling jxl-image v0.13.0
   Compiling signature v2.2.0
   Compiling rand_distr v0.6.0
   Compiling jxl-vardct v0.11.1
   Compiling inout v0.1.4
   Compiling v_frame v0.3.9
   Compiling prost-derive v0.14.4
   Compiling ordered-float v5.5.0
   Compiling tract-data v0.23.8
   Compiling as-slice v0.2.1
   Compiling semver v1.0.28
   Compiling wgpu-naga-bridge v30.0.1
   Compiling kurbo v0.13.1
   Compiling rustc-hash v2.1.3
   Compiling yoke v0.8.3
   Compiling av-scenechange v0.14.1
   Compiling read-fonts v0.41.0
   Compiling float-ord v0.3.2
   Compiling gpu-allocator v0.28.0
   Compiling icu_normalizer_data v2.3.0
   Compiling base16ct v0.2.0
   Compiling icu_segmenter_data v2.3.0
   Compiling zerovec v0.11.8
   Compiling zerotrie v0.2.5
   Compiling renderdoc-sys v1.1.0
   Compiling built v0.8.1
   Compiling rustc_version v0.4.1
   Compiling prost v0.14.4
   Compiling aligned v0.4.3
   Compiling photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-vector)
   Compiling cipher v0.4.4
   Compiling jxl-frame v0.13.3
   Compiling rand_core v0.9.5
   Compiling hkdf v0.12.4
   Compiling tract-core v0.23.8
   Compiling universal-hash v0.5.1
   Compiling crypto-bigint v0.5.5
   Compiling group v0.13.0
   Compiling rustls-pki-types v1.15.1
   Compiling arg_enum_proc_macro v0.3.4
   Compiling rav1e v0.8.1
   Compiling wgpu-core v30.0.1
   Compiling no_std_io2 v0.9.4
   Compiling regex-syntax v0.8.11
   Compiling litrs v1.0.0
   Compiling opaque-debug v0.3.1
   Compiling y4m v0.8.0
   Compiling pastey v0.1.1
   Compiling curve25519-dalek v4.1.3
   Compiling av1-grain v0.2.5
   Compiling tinystr v0.8.4
   Compiling potential_utf v0.1.6
   Compiling icu_locale_core v2.3.0
   Compiling tract-nnef v0.23.8
   Compiling icu_collections v2.3.0
   Compiling sec1 v0.7.3
   Compiling icu_provider v2.3.1
   Compiling tract-hir v0.23.8
   Compiling icu_locale_fallback v2.3.0
   Compiling wgpu-core-deps-windows-linux-android v30.0.1
   Compiling icu_properties v2.3.0
   Compiling bitstream-io v4.10.0
   Compiling jxl-color v0.11.0
   Compiling lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-tiff)
   Compiling tract-extra v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling font-types v0.11.3
   Compiling num-derive v0.4.2
   Compiling simd_helpers v0.1.0
   Compiling maybe-rayon v0.1.1
   Compiling wgpu v30.0.1
   Compiling float-cmp v0.9.0
   Compiling quick-xml v0.38.4
   Compiling tract-onnx-opl v0.23.8
   Compiling noop_proc_macro v0.3.0
   Compiling httparse v1.10.1
   Compiling untrusted v0.9.0
   Compiling new_debug_unreachable v1.0.6
   Compiling linebender_resource_handle v0.1.1
   Compiling document-features v0.2.12
   Compiling elliptic-curve v0.13.8
   Compiling imgref v1.12.3
   Compiling parlance v0.1.0
   Compiling fontique v0.11.1
   Compiling loop9 v0.1.5
   Compiling jxl-render v0.12.4
   Compiling tract-onnx v0.23.8
   Compiling lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-meta)
   Compiling icu_normalizer v2.3.0
   Compiling read-fonts v0.39.2
   Compiling icu_segmenter v2.3.0
   Compiling rand_chacha v0.9.0
   Compiling jxl-jbr v0.2.1
   Compiling skrifa v0.44.0
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-seg)
   Compiling harfrust v0.12.0
   Compiling parley_data v0.11.1
   Compiling rfc6979 v0.4.0
   Compiling rand_chacha v0.3.1
   Compiling avif-serialize v0.8.9
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-develop)
   Compiling curve25519-dalek-derive v0.1.1
   Compiling strict-num v0.1.1
   Compiling pulp v0.22.3
   Compiling aho-corasick v1.1.5
   Compiling jpeg-encoder v0.7.1
   Compiling rustls v0.23.45
   Compiling roxmltree v0.20.0
   Compiling base64 v0.22.1
   Compiling num-bigint-dig v0.8.6
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-text)
   Compiling arrayref v0.3.9
   Compiling rgb v0.8.53
   Compiling rand v0.8.8
   Compiling regex-automata v0.4.18
   Compiling ecdsa v0.16.9
   Compiling primeorder v0.13.6
   Compiling rand v0.9.5
   Compiling jxl-oxide v0.12.6
   Compiling lensfun v0.7.0
   Compiling rustls-webpki v0.103.15
   Compiling polyval v0.6.2
   Compiling aead v0.5.2
   Compiling photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-psd)
   Compiling http v1.5.0
   Compiling phf_shared v0.13.1
   Compiling moxcms v0.9.1
   Compiling jpeg-decoder v0.3.2
   Compiling num-iter v0.1.46
   Compiling raw-cpuid v11.6.0
   Compiling reborrow v0.5.5
   Compiling parley v0.11.1
   Compiling fearless_simd v0.4.1
   Compiling ttf-parser v0.25.1
   Compiling pulp-wasm-simd-flag v0.1.1
   Compiling fastrand v2.5.0
   Compiling phf_generator v0.13.1
   Compiling ghash v0.5.1
   Compiling regex v1.13.1
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-pipeline)
   Compiling ravif v0.13.0
   Compiling skrifa v0.42.1
   Compiling lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raw)
   Compiling poly1305 v0.8.0
   Compiling chacha20 v0.9.1
   Compiling ctr v0.9.2
   Compiling aes v0.8.4
   Compiling photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-compose)
   Compiling owned_ttf_parser v0.25.1
   Compiling linesweeper v0.4.0
   Compiling ed25519 v2.2.3
   Compiling lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-scenes)
   Compiling pkcs1 v0.7.5
   Compiling blake3 v1.8.7
   Compiling zune-inflate v0.2.54
   Compiling hayro-ccitt v0.3.0
   Compiling bit_field v0.10.3
   Compiling pollster v1.0.1
   Compiling wasmparser v0.228.0
   Compiling bumpalo v3.20.3
   Compiling ab_glyph_rasterizer v0.1.10
   Compiling web-time v1.1.0
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
   Compiling lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-codecs)
   Compiling twox-hash v2.1.4
   Compiling base64 v0.23.1
   Compiling option-ext v0.2.0
   Compiling lebe v0.5.3
   Compiling wasmi_ir v2.0.0
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling ab_glyph v0.2.32
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-gpu)
   Compiling exr v1.74.2
   Compiling ureq-proto v0.6.4
   Compiling rsa v0.9.10
   Compiling aes-gcm v0.10.3
   Compiling hayro-jpeg2000 v0.3.5
   Compiling hayro-jbig2 v0.3.0
   Compiling ed25519-dalek v2.2.0
   Compiling dirs-sys v0.5.0
   Compiling ruzstd v0.9.0
   Compiling chacha20poly1305 v0.10.1
   Compiling photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-pathops)
   Compiling lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-merge)
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-catalog)
   Compiling phf_macros v0.13.1
   Compiling p256 v0.13.2
   Compiling p384 v0.13.1
   Compiling x25519-dalek v2.0.1
   Compiling lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-preview)
   Compiling write-fonts v0.48.1
   Compiling rustls-webpki v0.102.8
   Compiling webpki-roots v1.0.9
   Compiling photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-algo)
   Compiling brotli v8.0.4
   Compiling kurbo v0.11.3
   Compiling sha1 v0.10.7
   Compiling wasmi_core v2.0.0
   Compiling hayro-postscript v0.1.0
   Compiling chunked_transfer v1.5.0
   Compiling httpdate v1.0.3
   Compiling wasmi v2.0.0
   Compiling leb128fmt v0.1.0
   Compiling constant_time_eq v0.4.2
   Compiling data-encoding v2.11.1
   Compiling utf8-zero v0.8.1
   Compiling percent-encoding v2.3.2
   Compiling jpeg-encoder v0.6.1
   Compiling ryu v1.0.23
   Compiling ascii v1.1.0
   Compiling utf-8 v0.7.6
   Compiling wasm-encoder v0.261.0
   Compiling tempfile v3.27.0
   Compiling ureq v3.4.2
   Compiling photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-codecs)
   Compiling pdf-writer v0.15.0
   Compiling svgtypes v0.15.3
   Compiling hayro-cmap v0.1.0
   Compiling tungstenite v0.27.0
   Compiling rustls-rustcrypto v0.0.2-alpha
   Compiling tiny_http v0.12.0
   Compiling subsetter v0.2.6
   Compiling phf v0.13.1
   Compiling hayro-syntax v0.7.2
   Compiling dirs v6.0.0
   Compiling tiny-skia-path v0.12.0
   Compiling tiny-skia-path v0.11.4
   Compiling uuid v1.26.1
   Compiling wait-timeout v0.2.1
   Compiling simplecss v0.2.2
   Compiling bit-vec v0.8.0
   Compiling xmlwriter v0.1.0
   Compiling xmp-writer v0.3.3
   Compiling wasmi_collections v2.0.0
   Compiling imagesize v0.14.0
   Compiling hex v0.4.3
   Compiling pico-args v0.5.0
   Compiling quick-error v1.2.3
   Compiling data-url v0.3.2
   Compiling fnv v1.0.7
   Compiling imagesize v0.13.0
   Compiling hayro-interpret v0.7.0
   Compiling krilla v0.8.2
   Compiling wast v261.0.0
   Compiling photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-format)
   Compiling rand_xorshift v0.4.0
   Compiling photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-paint)
   Compiling usvg v0.45.1
   Compiling photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raw)
   Compiling unarray v0.1.4
   Compiling wat v1.261.0
   Compiling photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-affinity)
   Compiling photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-svg)
   Compiling photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-plugins)
   Compiling photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-vecfx)
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
   Compiling photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-gpu)
   Compiling pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-trace)
   Compiling photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ops)
   Compiling rusty-fork v0.3.1
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-io)
   Compiling bit-set v0.8.0
   Compiling proptest v1.11.0
error[E0308]: mismatched types
   --> crates/lc-engine/src/tests_enhance.rs:237:52
    |
237 |     s.catalog.apply(Op::AddPhoto { photo: Box::new(replacement) }).unwrap();
    |                                           -------- ^^^^^^^^^^^ expected `Photo`, found `Arc<Photo>`
    |                                           |
    |                                           arguments to this function are incorrect
    |
    = note: expected struct `lightcraft_catalog::Photo`
               found struct `std::sync::Arc<lightcraft_catalog::Photo>`
note: associated function defined here
   --> library/alloc/src/boxed.rs:284:11

For more information about this error, try `rustc --explain E0308`.
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
error: could not compile `lightcraft-engine` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

</details>

<details>
<summary>Before-fix regressions — exit 101</summary>

```bash
cargo test --offline --no-fail-fast -p li-ai -p photocraft-engine -p lightcraft-engine -p lightcraft-develop --lib final_review -- --test-threads=1
```

```text
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
   Compiling photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-pdf)
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-develop)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
    Finished `test` profile [optimized + debuginfo] target(s) in 3m 05s
     Running unittests src/lib.rs (target/debug/deps/li_ai-2a1aea6bcd77ed32)

running 10 tests
test catalog::final_review_realvis_lightning_uses_author_sampling_profile ... FAILED
test ops::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels ... FAILED
test refine::tests::final_review_checker_rejects_inventory_change_after_submission ... FAILED
test refine::tests::final_review_checker_rejects_model_swap_after_submission ... FAILED
test refine::tests::final_review_checker_rejects_model_swap_before_cached_publication ... FAILED
test refine::tests::final_review_family_styles_are_preserved_or_rejected_explicitly ... FAILED
test refine::tests::final_review_hash_detects_same_size_rewrite_with_restored_mtime ... FAILED
test refine::tests::final_review_recipes_never_silently_drop_styles ... FAILED
test refine::tests::final_review_refine_keeps_explicit_values_equal_to_nominal_defaults ... FAILED
test refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes ... FAILED

failures:

---- catalog::final_review_realvis_lightning_uses_author_sampling_profile stdout ----

thread 'catalog::final_review_realvis_lightning_uses_author_sampling_profile' (1740616) panicked at crates/li-ai/src/catalog.rs:593:9:
assertion `left == right` failed
  left: (30.0, 6.0)
 right: (5.0, 1.5)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- ops::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels stdout ----

thread 'ops::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels' (1740624) panicked at crates/li-ai/src/ops.rs:962:5:
Qwen repair must meet the surrounding colour: jump 22

---- refine::tests::final_review_checker_rejects_inventory_change_after_submission stdout ----

thread 'refine::tests::final_review_checker_rejects_inventory_change_after_submission' (1741437) panicked at crates/li-ai/src/refine/tests.rs:587:9:
changed inventory must not publish a verdict

---- refine::tests::final_review_checker_rejects_model_swap_after_submission stdout ----

thread 'refine::tests::final_review_checker_rejects_model_swap_after_submission' (1743310) panicked at crates/li-ai/src/refine/tests.rs:482:9:
changed weights must not publish a verdict

---- refine::tests::final_review_checker_rejects_model_swap_before_cached_publication stdout ----

thread 'refine::tests::final_review_checker_rejects_model_swap_before_cached_publication' (1745051) panicked at crates/li-ai/src/refine/tests.rs:562:9:
changed weights must invalidate cached publication

---- refine::tests::final_review_family_styles_are_preserved_or_rejected_explicitly stdout ----

thread 'refine::tests::final_review_family_styles_are_preserved_or_rejected_explicitly' (1745170) panicked at crates/li-ai/src/refine/tests.rs:606:18:
incompatible styles must be rejected, not filtered

---- refine::tests::final_review_hash_detects_same_size_rewrite_with_restored_mtime stdout ----

thread 'refine::tests::final_review_hash_detects_same_size_rewrite_with_restored_mtime' (1745173) panicked at crates/li-ai/src/refine/tests.rs:653:5:
assertion `left != right` failed: filesystem change identity must invalidate the hash cache
  left: "02c1ec8afe5f6dabc3ae003bf5fc8c9d3572e74b2bfa5e4b01974e52a36bda29"
 right: "02c1ec8afe5f6dabc3ae003bf5fc8c9d3572e74b2bfa5e4b01974e52a36bda29"

---- refine::tests::final_review_recipes_never_silently_drop_styles stdout ----

thread 'refine::tests::final_review_recipes_never_silently_drop_styles' (1745178) panicked at crates/li-ai/src/refine/tests.rs:498:13:
template:edit must reject unsupported styles before submission

---- refine::tests::final_review_refine_keeps_explicit_values_equal_to_nominal_defaults stdout ----

thread 'refine::tests::final_review_refine_keeps_explicit_values_equal_to_nominal_defaults' (1745180) panicked at crates/li-ai/src/refine/tests.rs:532:5:
assertion `left == right` failed: explicit steps must not be guessed from numeric equality
  left: Number(12)
 right: 20

---- refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes stdout ----

thread 'refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes' (1746347) panicked at crates/li-ai/src/refine/tests.rs:623:134:
called `Result::unwrap()` on an `Err` value: Template has no loader for the declared model file


failures:
    catalog::final_review_realvis_lightning_uses_author_sampling_profile
    ops::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels
    refine::tests::final_review_checker_rejects_inventory_change_after_submission
    refine::tests::final_review_checker_rejects_model_swap_after_submission
    refine::tests::final_review_checker_rejects_model_swap_before_cached_publication
    refine::tests::final_review_family_styles_are_preserved_or_rejected_explicitly
    refine::tests::final_review_hash_detects_same_size_rewrite_with_restored_mtime
    refine::tests::final_review_recipes_never_silently_drop_styles
    refine::tests::final_review_refine_keeps_explicit_values_equal_to_nominal_defaults
    refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes

test result: FAILED. 0 passed; 10 failed; 0 ignored; 0 measured; 146 filtered out; finished in 4.50s

error: test failed, to rerun pass `-p li-ai --lib`
     Running unittests src/lib.rs (target/debug/deps/lightcraft_develop-9f8a860c7fa12f2a)

running 1 test
test tests_ai::final_review_mask_transfer_strips_segmentation_and_detail_only ... FAILED

failures:

---- tests_ai::final_review_mask_transfer_strips_segmentation_and_detail_only stdout ----

thread 'tests_ai::final_review_mask_transfer_strips_segmentation_and_detail_only' (1746781) panicked at crates/lc-develop/src/tests_ai.rs:127:9:
copied selection must be recomputed on its target
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests_ai::final_review_mask_transfer_strips_segmentation_and_detail_only

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 32 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p lightcraft-develop --lib`
     Running unittests src/lib.rs (target/debug/deps/lightcraft_engine-dfd75a6af4d21eee)

running 3 tests
test tests_enhance::final_review_changing_selection_keeps_the_original_remove_target ... FAILED
test tests_enhance::final_review_paste_sync_auto_sync_and_presets_recompute_ai_masks ... FAILED
test tests_enhance::final_review_remove_rejects_replaced_photo_and_library_jobs_are_discarded ... FAILED

failures:

---- tests_enhance::final_review_changing_selection_keeps_the_original_remove_target stdout ----

thread 'tests_enhance::final_review_changing_selection_keeps_the_original_remove_target' (1747057) panicked at crates/lc-engine/src/tests_enhance.rs:292:5:
assertion failed: !s.enhance.busy()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- tests_enhance::final_review_paste_sync_auto_sync_and_presets_recompute_ai_masks stdout ----

thread 'tests_enhance::final_review_paste_sync_auto_sync_and_presets_recompute_ai_masks' (1753078) panicked at crates/lc-engine/src/tests_enhance.rs:264:33:
target must use its own selection

---- tests_enhance::final_review_remove_rejects_replaced_photo_and_library_jobs_are_discarded stdout ----

thread 'tests_enhance::final_review_remove_rejects_replaced_photo_and_library_jobs_are_discarded' (1753081) panicked at crates/lc-engine/src/tests_enhance.rs:239:5:
same ID with different source must not receive an old result


failures:
    tests_enhance::final_review_changing_selection_keeps_the_original_remove_target
    tests_enhance::final_review_paste_sync_auto_sync_and_presets_recompute_ai_masks
    tests_enhance::final_review_remove_rejects_replaced_photo_and_library_jobs_are_discarded

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 458 filtered out; finished in 5.01s

error: test failed, to rerun pass `-p lightcraft-engine --lib`
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-0e249b799fc381ca)

running 1 test
test ai_cmds::tests::final_review_bridge_stops_on_unwind_and_preserves_results ... FAILED

failures:

---- ai_cmds::tests::final_review_bridge_stops_on_unwind_and_preserves_results stdout ----

thread 'ai_cmds::tests::final_review_bridge_stops_on_unwind_and_preserves_results' (1753399) panicked at crates/pc-engine/src/ai_cmds.rs:779:17:
deliberate callback unwind
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

thread 'ai_cmds::tests::final_review_bridge_stops_on_unwind_and_preserves_results' (1753399) panicked at crates/pc-engine/src/ai_cmds.rs:785:9:
assertion `left == right` failed: no watcher may survive callback unwinding
  left: "Preparing"
 right: "sentinel"


failures:
    ai_cmds::tests::final_review_bridge_stops_on_unwind_and_preserves_results

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1318 filtered out; finished in 0.27s

error: test failed, to rerun pass `-p photocraft-engine --lib`
error: 4 targets failed:
    `-p li-ai --lib`
    `-p lightcraft-develop --lib`
    `-p lightcraft-engine --lib`
    `-p photocraft-engine --lib`
```

</details>

<details>
<summary>Post-fix build interrupted before tests to isolate profile — exit 130</summary>

```bash
cargo test --offline --no-fail-fast -p li-ai -p lightcraft-develop -p lightcraft-engine --lib -- --test-threads=1
```

```text
   Compiling syn v3.0.6
   Compiling log v0.4.34
   Compiling smallvec v1.16.2
   Compiling spin v0.9.9
   Compiling serde_json v1.0.151
   Compiling num-complex v0.4.6
   Compiling arrayvec v0.7.8
   Compiling hashbrown v0.17.1
   Compiling stable_deref_trait v1.2.1
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling indexmap v2.14.2
   Compiling rustls v0.23.45
   Compiling ureq-proto v0.6.4
   Compiling tiny_http v0.12.0
   Compiling av1-grain v0.2.5
   Compiling avif-serialize v0.8.9
   Compiling as-slice v0.2.1
   Compiling aligned v0.4.3
   Compiling parking_lot_core v0.9.12
   Compiling ndarray v0.17.2
   Compiling rustfft v6.4.1
   Compiling lazy_static v1.5.0
   Compiling serde_derive v1.0.229
   Compiling thiserror-impl v2.0.21
   Compiling bytemuck_derive v1.12.1
   Compiling num-bigint-dig v0.8.6
   Compiling thiserror v2.0.21
   Compiling bytemuck v1.25.2
   Compiling parking_lot v0.12.5
   Compiling naga-types v30.0.1
   Compiling gpu-allocator v0.28.0
   Compiling av-scenechange v0.14.1
   Compiling lensfun v0.7.0
   Compiling tungstenite v0.27.0
   Compiling rsa v0.9.10
   Compiling serde v1.0.229
   Compiling image v0.25.10
   Compiling jxl-render v0.12.4
   Compiling wgpu-types v30.0.1
   Compiling naga v30.0.1
   Compiling rav1e v0.8.1
   Compiling lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-geom)
   Compiling string-interner v0.20.0
   Compiling lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-tiff)
   Compiling photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-color)
   Compiling photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-geom)
   Compiling ureq v3.4.2
   Compiling lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-color)
   Compiling jxl-oxide v0.12.6
   Compiling lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raster)
   Compiling lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-meta)
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-develop)
   Compiling lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-scenes)
   Compiling lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-preview)
   Compiling wgpu-naga-bridge v30.0.1
   Compiling ravif v0.13.0
   Compiling tract-data v0.23.8
   Compiling lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raw)
   Compiling wgpu-hal v30.0.1
   Compiling wgpu-core-deps-windows-linux-android v30.0.1
   Compiling wgpu-core v30.0.1
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-pipeline)
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-catalog)
   Compiling rustls-rustcrypto v0.0.2-alpha
   Compiling tract-linalg v0.23.8
   Compiling lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-codecs)
   Compiling photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raster)
   Compiling photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-algo)
   Compiling tract-core v0.23.8
   Compiling lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-merge)
   Compiling wgpu v30.0.1
   Compiling tract-nnef v0.23.8
   Compiling tract-hir v0.23.8
   Compiling tract-extra v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling tract-onnx-opl v0.23.8
   Compiling tract-onnx v0.23.8
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-seg)
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-gpu)
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
```

</details>

<details>
<summary>Full AI, Develop and library suites (two new AI fixture errors) — exit 101</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline --no-fail-fast -p li-ai -p lightcraft-develop -p lightcraft-engine --lib -- --test-threads=1
```

```text
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-develop)
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-pipeline)
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-catalog)
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-gpu)
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
    Finished `test` profile [optimized + debuginfo] target(s) in 1m 39s
     Running unittests src/lib.rs (target/debug/deps/li_ai-19e674dab104fe9c)

running 158 tests
test arch::tests::gguf_headers ... ok
test arch::tests::lora_names_are_not_models ... ok
test arch::tests::metadata_and_loras ... ok
test arch::tests::names_guess_families ... ok
test arch::tests::stable_diffusion_checkpoints ... ok
test arch::tests::transformer_families ... ok
test browser::tests::browse_and_install_against_the_mock ... ok
test browser::tests::civitai_cards_skip_nsfw_and_pickles ... ok
test browser::tests::gated_and_text_loras ... ok
test browser::tests::hugging_face_cards ... ok
test browser::tests::lora_files_are_named_after_their_repo ... ok
test browser::tests::manager_list_maps_types_bases_and_sizes ... ok
test browser::tests::plans_verify_add_components_and_refuse_unverified ... ok
test browser::tests::queries_build_urls_and_filter ... ok
test browser::tests::real_template_index_parses ... ok
test browser::tests::templates_list_their_files_including_subgraphs ... ok
test browser::tests::untagged_lora_repos_and_files_go_to_the_loras ... ok
test builders::ds2_variety_nodes_are_in_capability_requirements ... ok
test builders::ds_finding_flux_refine_batch_is_not_squared ... ok
test builders::tests::checkpoint_families_use_one_loader_and_clip_skip ... ok
test builders::tests::conventional_refine_families_keep_the_source_and_partial_denoise ... ok
test builders::tests::every_family_builds_closed_graphs_for_every_task ... ok
test builders::tests::flux1_fill_refine_keeps_the_official_image_and_mask_conditioning ... ok
test builders::tests::flux1_kontext_refine_keeps_the_official_scaled_source_latent ... ok
test builders::tests::flux1_schnell_refine_keeps_its_official_four_step_path ... ok
test builders::tests::flux2_dev_refine_follows_the_official_reference_scheduler_path ... ok
test builders::tests::flux2_klein_base_refine_keeps_the_official_cfg_negative_encoding ... ok
test builders::tests::flux2_klein_refines_by_reference_like_the_official_template ... ok
test builders::tests::flux_kontext_edit_chains_reference_latents ... ok
test builders::tests::hidream_dev_and_fast_refine_use_the_official_distilled_samplers ... ok
test builders::tests::inpaint_adds_a_noise_mask_and_differential_diffusion ... ok
test builders::tests::original_models_keep_their_node_sets ... ok
test builders::tests::original_qwen_edit_refines_with_its_official_single_image_encoder ... ok
test builders::tests::qwen21_refines_like_the_official_edit_and_encodes_for_partial_denoise ... ok
test builders::tests::qwen_edit_feeds_images_to_the_encoder ... ok
test builders::tests::qwen_edit_refine_and_edit_follow_the_official_scaled_cfg_norm_path ... ok
test builders::tests::z_image_base_refine_uses_the_official_cfg_and_negative_text ... ok
test builders::tests::zimage_variety_does_not_change_base_other_families_or_non_create_tasks ... ok
test builders::tests::zimage_variety_graph_restores_prompt_and_is_seed_deterministic ... ok
test builders::tests::zimage_variety_request_defaults_off_and_rejects_invalid_levels ... ok
test catalog::final_review_realvis_lightning_uses_author_sampling_profile ... ok
test catalog::tests::availability_reports_missing_nodes_then_files ... ok
test catalog::tests::every_original_model_has_a_preset_and_hashes_are_hex ... ok
test catalog::tests::installed_models_join_the_catalogue ... ok
test catalog::tests::installed_weight_variants_publish_their_official_sampling_defaults ... ok
test catalog::tests::refine_method_and_default_follow_the_model_pipeline ... ok
test cloud::tests::every_model_runs_against_the_mock ... ok
test cloud::tests::requests_are_validated ... ok
test cloud::tests::sizes_and_hosts ... ok
test comfy::tests::choices_parse_both_shapes ... ok
test comfy::tests::output_prefers_saved_images ... ok
test comfy::tests::stages_follow_node_classes ... ok
test custom::tests::markers_become_fields_and_values_are_set ... ok
test custom::tests::official_templates_convert_with_basic_controls ... ok
test custom::tests::subgraph_instances_wire_through ... ok
test custom::tests::ui_format_converts_with_object_info ... ok
test custom::tests::workflows_save_and_list ... ok
test download::ds2_verified_external_preset_can_already_be_installed ... ok
test download::tests::body_timeout_grows_with_the_file ... ok
test download::tests::existing_files_are_verified ... ok
test download::tests::host_allow_list ... ok
test download::tests::listed_files_stay_inside_the_model_folder ... ok
test download::tests::removing_a_preset_keeps_shared_and_foreign_files ... ok
test enhance::ds2_requested_scale_does_not_reject_smaller_native_model ... ok
test enhance::selection::tests::ds2_choices_preserve_concurrent_updates_and_corrupt_files ... ok
test enhance::selection::tests::enhance_function_choices_default_custom_validation_and_shared_execution ... ok
test enhance::tests::enhance_cancelled_before_loading_or_network ... ok
test enhance::tests::enhance_core_gpu_graph_keeps_alpha_and_exact_dimensions ... ok
test enhance::tests::enhance_curated_profiles_accept_pinned_core_weights_only ... ok
test enhance::tests::enhance_dimensions_odd_edges_and_old_settings ... ok
test enhance::tests::enhance_download_flow_verifies_retries_and_cancels_in_temp_dir ... ok
test enhance::tests::enhance_real_gpu_models ... ignored, Opt-in: pinned test-cache models and a real CUDA GPU through ComfyUI
test family::tests::bad_profiles_are_rejected ... ok
test family::tests::built_in_profiles_load_and_inherit ... ok
test family::tests::model_overrides_apply ... ok
test family::tests::updates_are_fetched_checked_and_cached ... ok
test imaging::tests::crop_box_has_context_and_stays_inside ... ok
test imaging::tests::cutout_alpha_validation ... ok
test imaging::tests::highlight_is_model_sized_and_outlined ... ok
test imaging::tests::poisson_matches_the_edge_and_keeps_detail ... ok
test imaging::tests::qwen_sizes_align ... ok
test imaging::tests::regions_split_at_wide_gaps ... ok
test inpaint::tests::blur_fill_uses_the_surroundings ... ok
test inpaint::tests::masks_fade_and_composite_stays_inside ... ok
test inpaint::tests::plan_pads_mask_bounds_and_context ... ok
test inventory::ds_metadata_tests::ds2_metadata_flat_modelspecific_and_kohya_classification ... ok
test inventory::ds_metadata_tests::ds2_metadata_safetensors_header_keeps_tensor_shapes_and_metadata ... ok
test inventory::tests::components_resolve_by_name_then_pattern ... ok
test inventory::tests::local_headers_decide_the_family ... ok
test inventory::tests::loras_in_model_folders_are_not_models ... ok
test inventory::tests::scan_finds_models_by_name_and_keeps_unknown_files ... ok
test library::tests::add_list_load_delete ... ok
test mock::tests::errors_and_cancellation_are_reported ... ok
test mock::tests::every_family_runs_against_the_mock ... ok
test mock::tests::generate_edit_remove_cutout_upscale_against_the_mock ... ok
test mock::tests::interrupt_stops_only_our_running_prompt ... ok
test mock::tests::refine_step_loras_reach_the_comfy_graph_and_validate_the_refine_family ... ok
test mock_hub::tests::synthetic_files_are_safetensors ... ok
test ops::ds2_refinement_request_rejects_nonfinite_and_out_of_range_parameters ... ok
test ops::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels ... ok
test presets::tests::templates_apply ... ok
test presets::tests::user_presets_round_trip ... ok
test refine::tests::autodetect_bypassed_template_model_and_its_output_are_activated ... ok
test refine::tests::autodetect_cached_template_rebinds_only_its_main_model_after_rename ... ok
test refine::tests::autodetect_cached_templates_work_offline_and_prefer_edit_variants ... ok
test refine::tests::autodetect_custom_sampler_template_prunes_other_models_and_splits_sigmas ... ok
test refine::tests::autodetect_fallback_order_is_family_template_universal ... ok
test refine::tests::autodetect_hash_changes_when_file_content_changes_and_cancellation_is_not_cached ... ok
test refine::tests::autodetect_metric_rejects_destroyed_composition ... ok
test refine::tests::autodetect_mock_all_recipes_fail_is_unsupported_and_cancellation_is_transient ... ok
test refine::tests::autodetect_mock_busy_queue_defers_without_a_verdict_or_second_prompt ... ok
test refine::tests::autodetect_mock_family_failure_falls_back_to_universal ... ok
test refine::tests::autodetect_mock_unknown_checkpoint_verifies_once_persists_and_retests ... ok
test refine::tests::autodetect_mock_unknown_split_weights_require_a_template_without_universal_guessing ... ok
test refine::tests::autodetect_template_match_checks_exact_file_role_size_and_hash ... ok
test refine::tests::autodetect_template_preserves_wiring_and_marks_refine_controls ... ok
test refine::tests::autodetect_text_template_becomes_img2img_with_its_decoder_vae ... ok
test refine::tests::autodetect_universal_graph_uses_node_defaults_and_standard_metadata ... ok
test refine::tests::autodetect_verdict_roundtrip_defaults_and_retest_preserves_other_hashes ... ok
test refine::tests::final_review_checker_rejects_inventory_change_after_submission ... ok
test refine::tests::final_review_checker_rejects_model_swap_after_submission ... ok
test refine::tests::final_review_checker_rejects_model_swap_before_cached_publication ... ok
test refine::tests::final_review_effective_metadata_resolves_saved_defaults_and_honours_explicit_values ... FAILED
test refine::tests::final_review_family_styles_are_preserved_or_rejected_explicitly ... ok
test refine::tests::final_review_hash_detects_same_size_rewrite_with_restored_mtime ... ok
test refine::tests::final_review_recipes_never_silently_drop_styles ... ok
test refine::tests::final_review_refine_keeps_explicit_values_equal_to_nominal_defaults ... ok
test refine::tests::final_review_second_pass_metadata_uses_resolved_strength_and_explicit_options ... ok
test refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes ... FAILED
test restoration::byo::ds2_byo_same_name_in_different_folders_remains_visible ... ok
test restoration::byo::tests::restoration_byo_acknowledgement_is_persisted_per_model_and_host ... ok
test restoration::byo::tests::restoration_byo_detects_all_approved_kinds_and_excludes_suppixel ... ok
test restoration::byo::tests::restoration_byo_download_entry_points_refuse_all_user_only_models ... ok
test restoration::byo::tests::restoration_byo_mock_run_is_gated_before_submission ... ok
test restoration::tests::ds2_admission_rejects_changed_models_before_submission ... ok
test restoration::tests::ds2_probe_honours_timeout_and_cancellation ... ok
test restoration::tests::restoration_allowlist_rejects_tampering_and_extra_architectures ... ok
test restoration::tests::restoration_browser_offline_and_strict_install_plan ... ok
test restoration::tests::restoration_editable_profiles_cannot_widen_allowlist ... ok
test restoration::tests::restoration_graph_per_task_uses_only_four_core_nodes ... ok
test restoration::tests::restoration_missing_and_changed_files_are_refused_before_comfy ... ok
test restoration::tests::restoration_mock_comfy_run_preserves_size_alpha_and_cancel ... ok
test restoration::tests::restoration_real_5090_each_supported_model ... ignored, requires pinned downloads and idle real RTX 5090 ComfyUI
test settings::tests::round_trip_keeps_unknown_keys_and_rejects_the_app_port ... ok
test setup::tests::finds_a_source_install_with_a_venv ... ok
test setup::tests::parses_system_stats_devices ... ok
test tls::tests::an_agent_with_the_pure_rust_provider_builds_and_refuses_a_closed_port ... ok
test trash::ds2_copy_to_trash_keeps_dangling_symlinks ... ok
test trash::ds_finding_deletion_date_uses_process_local_timezone ... ok
test trash::ds_finding_trash_preserves_non_utf8_names_and_symlinks ... ok
test trash::tests::dates_and_paths_are_formatted_for_the_spec ... ok
test trash::tests::files_move_into_the_trash_with_a_record ... ok
test trash::tests::overwrite_trash_copies_keep_source_and_reserve_versions ... ok
test trash::timezone::tests::ds_finding_local_offsets_cover_seasons_and_future_tzif_footer ... ok
test trash::timezone::tests::ds_finding_tzif_truncation_and_invalid_rules_are_rejected ... ok
test workflows::tests::ds2_qwen_partial_denoise_uses_first_reference_latent ... ok
test workflows::tests::other_graphs_are_closed ... ok
test workflows::tests::qwen_text_to_image_and_edit ... ok

failures:

---- refine::tests::final_review_effective_metadata_resolves_saved_defaults_and_honours_explicit_values stdout ----

thread 'refine::tests::final_review_effective_metadata_resolves_saved_defaults_and_honours_explicit_values' (2616161) panicked at crates/li-ai/src/refine/tests.rs:694:138:
called `Result::unwrap()` on an `Err` value: Template has no loader for the declared model file
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes stdout ----

thread 'refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes' (2622414) panicked at crates/li-ai/src/refine/tests.rs:635:134:
called `Result::unwrap()` on an `Err` value: Template has no loader for the declared model file


failures:
    refine::tests::final_review_effective_metadata_resolves_saved_defaults_and_honours_explicit_values
    refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes

test result: FAILED. 154 passed; 2 failed; 2 ignored; 0 measured; 0 filtered out; finished in 118.96s

error: test failed, to rerun pass `-p li-ai --lib`
     Running unittests src/lib.rs (target/debug/deps/lightcraft_develop-17060499d080d036)

running 34 tests
test controls::tests::every_control_reads_its_default ... ok
test controls::tests::formatting ... ok
test controls::tests::ids_unique_and_settable ... ok
test controls::tests::indexed_controls_address_list_elements ... ok
test presets::tests::amount_scales_numbers ... ok
test presets::tests::deep_merge_objects ... ok
test presets::tests::extract_and_apply_roundtrip ... ok
test segmask::tests::a_hostile_document_keeps_at_most_max_detail_patches ... ok
test segmask::tests::base64_round_trips ... ok
test segmask::tests::damaged_data_is_none ... ok
test segmask::tests::logits_round_trip_quantized ... ok
test settings::ds2_denoise_references_reject_invalid_non_null_values ... ok
test settings::ds_tests::ds_finding_layer_skin_tone_controls_are_routed ... ok
test tests::copy_paste_presets_and_sync_carry_layer_tools ... ok
test tests::layer_tools_are_sparse_and_round_trip ... ok
test tests::layer_tools_merge_like_presets ... ok
test tests::masks_written_before_layers_load_and_serialize_unchanged ... ok
test tests::merge_partial ... ok
test tests::missing_fields_take_defaults_and_unknown_ignored ... ok
test tests::negative_defaults_off_and_old_json_reads_as_disabled ... ok
test tests::negative_merges_copies_and_resets ... ok
test tests::old_process_fields_are_ignored_and_looks_roundtrip ... ok
test tests::section_toggles ... ok
test tests::serde_roundtrip_default_and_edited ... ok
test tests::toolset_sections_default_off_and_left_out_of_old_json ... ok
test tests::unedited_and_reset ... ok
test tests_ai::ai_denoise_settings_roundtrip_and_copies_keep_only_controls ... ok
test tests_ai::ai_spots_round_trip_and_denoise_restore_compatibility ... ok
test tests_ai::copies_leave_photo_bound_ai_results_behind ... ok
test tests_ai::final_review_mask_transfer_strips_segmentation_and_detail_only ... ok
test tests_ai::final_review_reapplying_adaptive_preset_keeps_the_target_mask_once ... ok
test tests_ai::older_settings_load_and_serialize_unchanged ... ok
test tests_ai::pasted_spots_keep_the_targets_ai_removals ... ok
test tools::raw_quality_tests::option_serialization_and_menu_compatibility ... ok

test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

     Running unittests src/lib.rs (target/debug/deps/lightcraft_engine-6aea7f9adc39d5f6)

running 461 tests
test availability::tests::background_answers_from_the_cache_without_blocking ... ok
test availability::tests::synchronous_by_default ... ok
test camera_preview::tests::accepts_a_much_better_fit_despite_local_camera_processing ... ok
test camera_preview::tests::chroma_curve_follows_highlight_bleaching ... ok
test camera_preview::tests::corpus_nef_gets_a_camera_look ... ok
test camera_preview::tests::corpus_rw2_with_an_in_camera_crop_gets_a_camera_look ... ok
test camera_preview::tests::hue_sat_passes_black_and_non_finite_pixels_through ... ok
test camera_preview::tests::hue_table_follows_a_hue_dependent_camera_rendering ... ok
test camera_preview::tests::linear_arw_gets_a_sensor_proxy_without_demosaicing ... ok
test camera_preview::tests::profile_colour_is_pooled_and_then_used_as_given ... ok
test camera_preview::tests::rejects_monochrome_invalid_and_unrelated_previews ... ok
test camera_preview::tests::separates_nonlinear_tone_from_colour_and_keeps_sensor_headroom ... ok
test camera_preview::tests::sony_nikon_and_panasonic_raws_get_a_file_local_look ... ok
test camera_profiles::tests::bundled_profiles_are_valid_and_named_after_their_model ... ok
test camera_profiles::tests::file_names_are_safe ... ok
test camera_profiles::tests::pool_skips_files_that_cannot_contribute ... ok
test camera_profiles::tests::profiles_round_trip_and_reject_bad_files ... ok
test cmd::convert::tests::reload_fills_only_missing_camera_fields ... ok
test cmd::library::gps_tests::coordinates_people_type ... ok
test cmd::masks::ds_finding_dust_source_settings_filter_foreign_once ... ok
test cmd::prefs::ds_tests::ds_finding_invalid_preferences_have_no_partial_effects ... ok
test cmd::save_over::ds_sampling_tests::ds_finding_jpeg_requires_matching_chroma_components ... ok
test cmd::save_over::tests::chroma_of_our_own_jpegs ... ok
test cmd::save_over::tests::stamps_are_folder_names ... ok
test crs::detail_interchange_tests::all_detail_fields_and_import_bounds ... ok
test crs::tests::element_form_bw_and_incremental_wb ... ok
test crs::tests::maps_common_fields ... ok
test crs::tests::named_wb_and_already_applied ... ok
test crs::tests::only_present_fields_are_in_the_partial ... ok
test crs::tests::xmp_preset_with_name_and_group ... ok
test crs_masks::tests::corrections_become_masks ... ok
test crs_masks::tests::lrtemplate_corrections ... ok
test crs_masks::tests::masks_render ... ok
test crs_masks::tests::presets_add_masks_once ... ok
test crs_masks::tests::sidecar_radials_fit_the_photo ... ok
test dcp_profiles::tests::user_folder_is_read_only_bounded_and_skips_invalid_profiles ... ok
test devices::tests::slow_scans_never_block_and_hot_plug_still_arrives ... ok
test devices::tests::volumes_with_a_dcim_folder_are_devices ... ok
test enhance::cmds::ds2_inline_job_does_not_report_an_unrelated_failure ... ok
test enhance::denoise::integration_tests::ai_denoise_auto_routes_a_rendered_file_through_display_rgb ... ok
test enhance::denoise::integration_tests::ai_denoise_exports_new_tiff_and_dng_with_neutral_working_wb ... ok
test enhance::denoise::integration_tests::ai_denoise_packing_uses_sensor_values_in_all_four_phases ... ok
test enhance::denoise::integration_tests::ai_denoise_result_is_foreign_after_settings_copy_and_preview_cannot_export ... ok
test enhance::denoise::tests::ai_denoise_camera_source_keeps_post_demosaic_opcodes_crop_and_orientation ... ok
test enhance::denoise::tests::denoise_is_refused_where_it_cannot_run_and_can_be_cancelled ... ok
test enhance::denoise::tests::denoise_runs_stores_and_mixes_by_amount ... ok
test enhance::remove::tests::joined_lassos_and_dabs_cover_their_union ... ok
test enhance::remove::tests::keys_change_with_every_input ... ok
test enhance::remove::tests::local_heal_fills_from_the_surroundings ... ok
test enhance::remove::tests::strokes_and_lassos_cover_their_area ... ok
test enhance::store::tests::files_round_trip_within_the_encoding_steps ... ok
test enhance::store::tests::keys_name_their_inputs ... ok
test enhance::store::tests::results_are_kept_in_memory_or_in_a_library_folder ... ok
test enhance::tests::background_jobs_cancel_and_panics_become_errors ... ok
test enhance::tests::jobs_run_report_and_are_picked_up_once ... ok
test ephemeral::tests::clear_dir_removes_leftovers_only ... ok
test ephemeral::tests::edits_of_an_ephemeral_photo_never_reach_the_log_and_closing_restores_everything ... ok
test ephemeral::tests::the_file_decodes_back_to_the_same_linear_pixels ... ok
test export::ds2_original_export_borrows_photo_unless_people_keywords_are_requested ... ok
test export::tests::avif_ten_bit ... ok
test export::tests::bit_depth_options_per_format ... ok
test export::tests::enhance_samples_keep_source_profile_float_encoding ... ok
test export::tests::every_space_embeds_its_own_profile_and_round_trips ... ok
test export::tests::float_tiff_is_linear_with_a_linear_profile ... ok
test export::tests::formats_have_magic ... ok
test export::tests::graphic_watermark_is_placed_scaled_and_blended ... ok
test export::tests::japanese_watermark_options_and_legacy_defaults ... ok
test export::tests::japanese_watermarks_support_vertical_columns ... ok
test export::tests::metadata_policies ... ok
test export::tests::naming_and_params ... ok
test export::tests::naming_uses_the_rename_tokens ... ok
test export::tests::p3_colour_survives_a_p3_export_and_is_clipped_in_srgb ... ok
test export::tests::params_round_trip ... ok
test export::tests::resize_modes ... ok
test export::tests::sharpen_increases_edge_contrast ... ok
test export::tests::sixteen_bit_tiff_and_png_have_no_banding ... ok
test export::tests::size_limit_respected ... ok
test export::tests::source_profile_encoding_converts_pixels_and_stays_out_of_presets ... ok
test export::tests::srgb_watermark_colour_is_converted ... ok
test export::tests::watermark_draws_in_the_anchored_corner_only ... ok
test export::tests::watermarks_work_without_craft_fonts ... ok
test files::tests::dng_profile_look_is_applied ... ok
test files::tests::ds_finding_sensor_confidence_matches_normalized_active_samples ... ok
test files::tests::quick_jobs_show_embedded_previews_then_cached_renders ... ok
test files::tests::unsupported_raw_falls_back_to_embedded_preview ... ok
test files::tests::unsupported_raw_imports_as_preview_only_and_survives_reopen ... ok
test fonts::tests::a_language_gets_its_own_script_first ... ok
test fonts::tests::craft_fonts_are_well_formed_or_absent ... ok
test guard::tests::a_panic_becomes_an_error ... ok
test lens_db::tests::database_loads_and_detects_cameras ... ok
test lens_db::tests::matches_the_lensfun_modifier ... ok
test lens_db::tests::unknown_or_incomplete_requests_give_nothing ... ok
test media::tests::budget_evicts_least_recently_used_across_caches ... ok
test media::tests::decoder_info_survives_render_jobs_cache_and_eviction ... ok
test media::tests::ds_finding_smart_preview_keeps_distinct_look_curve ... ok
test media::tests::levels_cover_sizes_and_large_exports_use_the_original ... ok
test media::tests::variant_jobs_have_their_own_keys_and_hit_the_cache ... ok
test media::tests::work_gate_waits_for_room ... ok
test media::thumbnail_hash_tests::hash_cache_is_bounded_weak_and_shared_settings_remain_independent ... ok
test media::thumbnail_hash_tests::hashes_match_uncached_keys_and_are_not_recomputed_for_sizes_metadata_or_before ... ok
test media::thumbnail_hash_tests::source_from_before_content_reload_is_not_accepted ... ok
test originals::tests::originals_and_their_sidecars_are_protected_by_identity ... ok
test preset_import::tests::generic_folder_names_are_not_groups ... ok
test preset_import::tests::import_command_reads_folders_bundles_and_applies ... ok
test preset_import::tests::lrtemplate_maps_settings_and_reports_the_rest ... ok
test preset_import::tests::lua_tables ... ok
test preset_import::tests::old_process_version_fields_are_approximated ... ok
test preset_import::tests::photos_carrying_edits_become_looks ... ok
test preset_import::tests::zip_bundles_with_folders_as_groups ... ok
test preset_luminar::tests::classic_looks_and_bundles ... ok
test preset_luminar::tests::ds_finding_nonfinite_layer_amount_is_skipped ... ok
test preset_luminar::tests::hostile_nesting_is_an_error_not_a_crash ... ok
test preset_luminar::tests::import_command_reads_looks_and_bundle_folders ... ok
test preset_luminar::tests::look_sliders_map_and_the_rest_is_reported ... ok
test preset_luminar::tests::packs_group_by_their_name ... ok
test preset_luminar::tests::plists_parse ... ok
test presets::tests::builtin_presets_parse_and_apply ... ok
test presets::tests::creative_profiles_render_without_moving_the_sliders ... ok
test presets::tests::every_profile_has_a_look_and_ids_are_unique ... ok
test presets::tests::id_clashes_get_fresh_ids_and_bad_files_are_reported ... ok
test presets::tests::lcpreset_export_import_roundtrip_keeps_groups ... ok
test presets::tests::slugs ... ok
test presets::tests::xmp_preset_file_imports_and_applies ... ok
test quickseg::tests::probabilities_become_a_logit_grid ... ok
test quickseg::tests::without_models_masks_keep_the_classical_estimate ... ok
test rename::tests::case_only_rename_on_disk ... ok
test rename::tests::case_only_renames_never_replace_another_file ... ok
test rename::tests::failed_rollbacks_are_reported_and_followed ... ok
test rename::tests::folder_templates_stay_inside_the_destination ... ok
test rename::tests::number_folder_and_metadata_tokens ... ok
test rename::tests::shared_stem_sidecar_is_copied_not_taken ... ok
test rename::tests::templates ... ok
test rename::tests::token_help_matches_the_implementation ... ok
test segment::tests::detail_region_frames_the_selection_with_a_margin ... ok
test segment::tests::no_region_for_nothing_or_most_of_the_photo ... ok
test sidecar::tests::foreign_sidecar_rating_reject_label_and_crs ... ok
test sidecar::tests::naming ... ok
test sidecar::tests::op_photo_collection ... ok
test sidecar::tests::packet_roundtrip_restores_everything ... ok
test smart::ds_finding_header_limit_includes_jpeg_start ... ok
test smart::tests::camera_tone_travels_with_the_proxy ... ok
test smart::tests::damaged_proxies_are_invalid_and_never_kept_over_good_ones ... ok
test smart::tests::deferred_dcp_and_camera_white_travel_with_the_proxy ... ok
test smart::tests::malformed_stored_profile_tables_and_curves_are_rejected ... ok
test smart::tests::migrate_moves_leaves_or_discards ... ok
test smart::tests::roundtrip_keeps_the_picture ... ok
test smart::tests::writable_check_does_not_create_missing_parents ... ok
test smart_sort::bursts::tests::best_shot_tie_break_order ... ok
test smart_sort::bursts::tests::chaining_compares_with_previous_photo_only ... ok
test smart_sort::bursts::tests::constants_are_pinned ... ok
test smart_sort::bursts::tests::cosine_threshold_at_each_strictness ... ok
test smart_sort::bursts::tests::deterministic_under_permutation_and_covers_every_id ... ok
test smart_sort::bursts::tests::export_modes ... ok
test smart_sort::bursts::tests::missing_time_or_embedding_are_singletons ... ok
test smart_sort::bursts::tests::settings_serde_camel_case_with_defaults ... ok
test smart_sort::bursts::tests::sharpness_ranks_sharp_above_blurred ... ok
test smart_sort::bursts::tests::strictness_changes_the_result ... ok
test smart_sort::bursts::tests::time_threshold_at_each_strictness ... ok
test smart_sort::examples::tests::combine_gamma_caps_at_ten_exemplars ... ok
test smart_sort::examples::tests::combine_text_and_exemplars_uses_gamma ... ok
test smart_sort::examples::tests::combine_text_only_returns_normalized_text ... ok
test smart_sort::examples::tests::combine_without_text_returns_exemplar_prototype ... ok
test smart_sort::examples::tests::exemplar_prototype_averages_direction ... ok
test smart_sort::examples::tests::exemplar_prototype_rejects_empty_mismatched_zero ... ok
test smart_sort::examples::tests::rank_orders_by_cosine_and_ties_stable ... ok
test smart_sort::examples::tests::suggest_name_counts_case_insensitive_and_ties_alphabetical ... ok
test smart_sort::examples::tests::suggest_name_hierarchical_keywords_are_valid_category_names ... ok
test smart_sort::people::tests::advanced_person_rules_resolve_nested_ids_and_report_unknowns ... ok
test smart_sort::people::tests::analysis_handles_videos_unreadable_unknown_and_retries_pending_save ... ok
test smart_sort::people::tests::clear_deletes_all_versions_preserves_tags_and_only_removes_auto_regions ... ok
test smart_sort::people::tests::clear_without_remove_names_keeps_names_and_legacy_people_centroids_load ... ok
test smart_sort::people::tests::combined_analysis_requires_opt_in_and_caches_both_models ... ok
test smart_sort::people::tests::deterministic_clusters_counts_covers_hidden_pin_and_ignore ... ok
test smart_sort::people::tests::ds_finding_deleted_face_cache_is_rebuilt_completely ... ok
test smart_sort::people::tests::ds_finding_identity_exhaustion_is_an_error_without_mutation ... ok
test smart_sort::people::tests::ds_finding_manual_region_keeps_identity_without_detection ... ok
test smart_sort::people::tests::ds_finding_people_shared_pass_preserves_margins_and_rejections ... ok
test smart_sort::people::tests::face_cache_atomic_append_failed_retry_corruption_and_no_faces_records ... ok
test smart_sort::people::tests::face_gates_unknown_counts_manual_override_and_zero_faces ... ok
test smart_sort::people::tests::find_by_existing_name_refines_identity_from_an_automatic_match ... ok
test smart_sort::people::tests::finding_a_face_locates_existing_bubble_and_reject_never_returns ... ok
test smart_sort::people::tests::inactive_people_conditions_allow_tag_planning_after_opt_out ... ok
test smart_sort::people::tests::malformed_people_json_recovers_and_failed_change_is_atomic ... ok
test smart_sort::people::tests::merge_keeps_saved_folder_ids_working_and_undo_restores_them ... ok
test smart_sort::people::tests::merge_split_suggestions_and_undo_restore_identity_and_regions ... ok
test smart_sort::people::tests::merged_identity_keeps_library_filters_and_smart_albums_working_and_undoable ... ok
test smart_sort::people::tests::mixed_tag_people_layout_resolves_ids_overlap_and_unsorted ... ok
test smart_sort::people::tests::mock_headshots_seed_exactly_one_face_name_list_and_gallery_matches ... ok
test smart_sort::people::tests::naming_is_one_undo_step_and_catalog_people_and_rules_work ... ok
test smart_sort::people::tests::offline_cropped_thumbnail_analysis_stores_uncropped_face_rects ... ok
test smart_sort::people::tests::opt_in_default_and_command_registry_privacy ... ok
test smart_sort::people::tests::people_folders_any_everyone_tag_and_or_toggle_and_unknown_notice ... ok
test smart_sort::people::tests::people_json_rejections_flags_names_and_opt_in_persist_per_library ... ok
test smart_sort::people::tests::real_analysis_jobs_cancel_resume_deduplicate_and_save_after_all_cached ... ok
test smart_sort::people::tests::real_weight_people_end_to_end ... ignored, coordinator: set LI_SEG_TEST_MODELS to verified YuNet/SFace directory
test smart_sort::people::tests::single_photo_people_are_hidden_by_default_and_can_be_shown_and_picked ... ok
test smart_sort::people::tests::thumbnail_and_rotated_detections_map_to_full_upright_mwg_frame ... ok
test smart_sort::people::tests::uncertain_queue_least_sure_first_confirm_reranks_and_gates_folders ... ok
test smart_sort::people::tests::unnamed_bubbles_can_go_in_folders_and_preset_layout_roundtrips ... ok
test smart_sort::people::tests::xmp_regions_seed_names_but_preserve_imported_and_manual_regions ... ok
test smart_sort::plan::ds_tests::ds_finding_destination_guard_reuses_distinct_original_parents ... ok
test smart_sort::plan::ds_tests::ds_finding_session_restrictions_are_dormant_without_sessions ... ok
test smart_sort::presets::ds_tests::ds_finding_contradictory_face_gate_is_rejected ... ok
test smart_sort::sessions::tests::custom_name_by_start_time ... ok
test smart_sort::sessions::tests::ds_finding_index_preserves_first_membership_and_missing_time ... ok
test smart_sort::sessions::tests::equal_times_preserve_input_order ... ok
test smart_sort::sessions::tests::exactly_20_minutes_does_not_split ... ok
test smart_sort::sessions::tests::gap_10_minutes_does_not_split ... ok
test smart_sort::sessions::tests::gap_25_minutes_splits ... ok
test smart_sort::sessions::tests::gap_clamp_high ... ok
test smart_sort::sessions::tests::gap_clamp_low ... ok
test smart_sort::sessions::tests::no_time_photos_are_separate ... ok
test smart_sort::sessions::tests::unknown_ids_are_ignored ... ok
test smart_sort::tests::analysis_is_idempotent_and_virtual_copies_share_embeddings ... ok
test smart_sort::tests::analysis_retries_a_failed_save_even_when_every_photo_is_cached ... ok
test smart_sort::tests::both_legacy_folder_layouts_load_and_combined_preset_roundtrips ... ok
test smart_sort::tests::cancelled_analysis_keeps_partial_cache_and_resumes ... ok
test smart_sort::tests::category_whitespace_keeps_keywords_and_folder_rules_consistent ... ok
test smart_sort::tests::classification_of_a_red_dominant_demo_scene ... ok
test smart_sort::tests::classify_red_unrelated_and_manual_overrides ... ok
test smart_sort::tests::clip_analysis_ignores_retired_cache_and_resumes_after_reopen ... ok
test smart_sort::tests::ds_finding_cancelled_analysis_does_not_load_model ... ok
test smart_sort::tests::ds_finding_deleted_embedding_cache_is_rebuilt_completely ... ok
test smart_sort::tests::exemplars_move_a_borderline_photo ... ok
test smart_sort::tests::folder_export_overlap_unsorted_custom_rules_and_guards ... ok
test smart_sort::tests::inputs_fall_back_to_a_cached_thumbnail_when_original_is_offline ... ok
test smart_sort::tests::inputs_prefer_embedded_raw_and_support_offline_smart_previews ... ok
test smart_sort::tests::invalid_assignments_are_atomic ... ok
test smart_sort::tests::keywords_are_one_undo_step_and_replace_only_descendants ... ok
test smart_sort::tests::multi_and_ties_follow_category_order ... ok
test smart_sort::tests::old_prefs_and_model_isolation_and_corrupt_caches ... ok
test smart_sort::tests::p2b_analysis_sharpness_cache_survives_reload ... ok
test smart_sort::tests::p2b_bursts_commands_best_all_and_catalog_one_undo ... ok
test smart_sort::tests::p2b_legacy_embeddings_gain_sharpness_without_model_inference ... ok
test smart_sort::tests::p2b_missing_embedding_breaks_consecutive_burst ... ok
test smart_sort::tests::p2b_nested_export_tokens_and_duplicate_resolved_folders ... ok
test smart_sort::tests::p2b_sessions_commands_narrow_tag_custom_people_and_no_time_folders ... ok
test smart_sort::tests::p2b_sessions_rename_preset_persistence_and_old_defaults ... ok
test smart_sort::tests::p2b_three_example_photos_define_category_without_tags ... ok
test smart_sort::tests::plans_use_whole_keywords_case_insensitively_and_clean_names ... ok
test smart_sort::tests::prompts_defaults_and_empty_categories ... ok
test smart_sort::tests::rejected_videos_and_unreadable_inputs_are_handled ... ok
test smart_sort::tests::retired_tagging_preference_loads_and_preserves_library_settings ... ok
test smart_sort::tests::sensitivity_and_exemplar_edits_reuse_text_inference ... ok
test smart_sort::tests::store_and_presets_roundtrip_through_a_library ... ok
test smart_sort::tests::store_appends_atomically_and_retries_a_failed_batch ... ok
test smart_sort::tests::tag_any_all_use_max_min_and_each_tag_gets_exemplar_boost ... ok
test smart_sort::tests::tag_sets_and_folder_layout_persist_with_old_defaults ... ok
test smart_sort::tokens::ds_tests::ds_finding_duplicate_folders_and_bad_date ... ok
test smart_sort::tokens::tests::date_and_camera_defaults ... ok
test smart_sort::tokens::tests::dedupe_adds_suffix_case_insensitively ... ok
test smart_sort::tokens::tests::dotdot_cannot_escape ... ok
test smart_sort::tokens::tests::each_token_expands ... ok
test smart_sort::tokens::tests::empty_values_drop_folder_segments ... ok
test smart_sort::tokens::tests::file_pattern_rejects_slash ... ok
test smart_sort::tokens::tests::sequence_padding ... ok
test smart_sort::tokens::tests::unknown_token_is_an_error ... ok
test smart_sort::tokens::tests::unsafe_chars_are_sanitized_per_segment ... ok
test tests::ai_masks_without_the_model_explain_themselves ... ok
test tests::albums_crud ... ok
test tests::auto_bw_mix_separates_colours ... ok
test tests::auto_sync_carries_only_the_changed_settings ... ok
test tests::before_is_the_import_state_and_can_be_set_copied_and_swapped ... ok
test tests::build_previews_fills_the_cache ... ok
test tests::changing_one_wb_control_resolves_as_shot_without_stale_tint ... ok
test tests::color_range_sampling_selects_the_clicked_colour ... ok
test tests::command_sweep ... ok
test tests::copy_paste_sync_presets ... ok
test tests::crop_aspect_lock_current_and_toggle ... ok
test tests::curve_reset_by_channel_and_whole ... ok
test tests::demo_library_loads ... ok
test tests::develop_set_and_interaction_coalesces ... ok
test tests::ds_finding_reset_restores_import_preset_look ... ok
test tests::face_job_crops_a_square_inside_the_photo ... ok
test tests::face_job_follows_rotate_right ... ok
test tests::filter_presets_save_and_apply ... ok
test tests::filters_and_delete_restore ... ok
test tests::find_similar_filters_to_look_alikes ... ok
test tests::json_delta_keeps_only_changes ... ok
test tests::leaving_an_edited_photo_keeps_an_auto_version ... ok
test tests::mask_components_invert_duplicate_rename_change_mode_and_delete ... ok
test tests::masks_crop_and_render ... ok
test tests::masks_reorder_and_duplicate_and_invert ... ok
test tests::match_total_exposures ... ok
test tests::memory_report_counts_decoded_sources_and_renders ... ok
test tests::negative_film_base_pick_and_auto ... ok
test tests::op_log_replay_reproduces_catalog ... ok
test tests::paste_from_previous_and_copy_paste_metadata ... ok
test tests::paste_selected_settings_pastes_only_chosen_copied_groups ... ok
test tests::presets_versions_history_and_select_by ... ok
test tests::quick_develop_adds_to_each_photo ... ok
test tests::rating_flag_undo_redo ... ok
test tests::stacks_move_and_split ... ok
test tests::upright_and_guides_commands ... ok
test tests::versions_history_and_reset ... ok
test tests_color::pet_eye_catchlight_command ... ok
test tests_color::point_color_pick_and_adjust ... ok
test tests_color::red_eye_commands_and_controls ... ok
test tests_color::skin_reference_pick_controls_history_and_layers ... ok
test tests_color::targeted_adjustment_on_curve_and_mixer ... ok
test tests_enhance::ai_remove_is_one_undo_step_and_regenerates ... ok
test tests_enhance::content_aware_heal_needs_no_ai_engine ... ok
test tests_enhance::copies_and_other_photos_never_get_ai_results ... ok
test tests_enhance::final_review_changing_selection_keeps_the_original_remove_target ... ok
test tests_enhance::final_review_paste_sync_auto_sync_and_presets_recompute_ai_masks ... ok
test tests_enhance::final_review_remove_rejects_replaced_photo_and_library_jobs_are_discarded ... ok
test tests_enhance::lassos_jobs_and_engines ... ok
test tests_export::a_failed_export_write_keeps_the_previous_file ... ok
test tests_export::dng_export_rejects_non_raw_photos ... ok
test tests_export::enhance_export_restores_native_pixels_before_encoding_and_defaults_load ... ok
test tests_export::export_falls_back_to_the_cpu_when_gpu_work_is_lost ... ok
test tests_export::export_never_overwrites_an_original ... ok
test tests_export::export_protects_the_target_of_an_imported_symlink ... ok
test tests_export::export_sidecars_follow_the_conflict_policy ... ok
test tests_export::exports_are_atomic_without_a_sync ... ok
test tests_export::full_size_export_of_a_cropped_photo_is_not_upscaled ... ok
test tests_export::original_and_dng_need_a_file ... ok
test tests_export::original_export_copies_the_file_and_writes_its_edits_beside_it ... ok
test tests_export::people_names_export_opt_in_metadata_and_original_sidecar ... ok
test tests_export::prepared_exports_run_on_another_thread ... ok
test tests_export::presets_conflicts_and_subfolders ... ok
test tests_forget_local::opening_the_library_forgets_per_the_preference ... ok
test tests_forget_local::sidecar_values_are_not_user_changes_and_sidecars_stay ... ok
test tests_forget_local::untouched_records_of_unbrowsed_folders_are_forgotten_and_come_back ... ok
test tests_forget_local::upgraded_catalogs_forget_nothing_at_first ... ok
test tests_gpu::ai_remove_is_in_the_gpu_render ... ok
test tests_gpu::deep_corpus_raw_gpu_matches_cpu ... ignored, set LOCAL_IMAGE_TEST_RAW to a corpus raw file
test tests_gpu::deep_raw_output_uses_the_gpu_with_raw_processing_options ... ok
test tests_gpu::lens_database_profiles_render_on_the_gpu ... ok
test tests_gpu::native_tone_equalizer_layers_and_depth_use_engine_gpu_stages ... ok
test tests_gpu::new_cfa_methods_and_segmentation_render_on_the_gpu ... ok
test tests_gpu::raw_options_decode_again_and_render_on_the_gpu ... ok
test tests_import::assisted_culling_groups_bursts_and_scores_focus ... ok
test tests_import::auto_import_watched_folder ... ok
test tests_import::browsing_a_folder_lists_its_photos_without_adding_them ... ok
test tests_import::civil_dates ... ok
test tests_import::copy_into_library_and_persist ... ok
test tests_import::copy_with_a_folder_template ... ok
test tests_import::copy_with_destination_organize_rename_and_metadata_preset ... ok
test tests_import::cube_luts_become_profiles ... ok
test tests_import::damaged_smart_previews_are_rebuilt_and_failed_writes_leave_none ... ok
test tests_import::dust_spots_are_found_and_healed ... ok
test tests_import::find_missing_matches_renamed_files_by_content ... ok
test tests_import::find_missing_plan_is_rechecked_when_applied ... ok
test tests_import::find_missing_planned_on_a_worker_matches_the_command ... ok
test tests_import::folder_rename_and_move_undo_and_redo_on_disk ... ok
test tests_import::folders_rename_and_move_with_their_photos ... ok
test tests_import::missing_files_are_found_and_relinked ... ok
test tests_import::missing_photos_skip_local_browse_records ... ok
test tests_import::recently_added_covers_recent_imports_newest_first ... ok
test tests_import::recursive_import_with_duplicates ... ok
test tests_import::smart_previews_folder_is_chosen_per_library ... ok
test tests_import::smart_previews_stand_in_for_offline_originals ... ok
test tests_import_move::a_failed_or_corrupt_copy_keeps_the_source ... ok
test tests_import_move::a_shared_sidecar_stays_with_the_photo_left_behind ... ok
test tests_import_move::a_source_that_cannot_be_removed_is_kept_and_reported ... ok
test tests_import_move::collisions_are_numbered_and_files_in_the_destination_stay ... ok
test tests_import_move::copy_import_refuses_a_file_changed_since_the_review ... ok
test tests_import_move::copy_import_verifies_each_copy ... ok
test tests_import_move::copy_is_verified_against_the_probe_hash ... ok
test tests_import_move::duplicates_and_failures_keep_their_sources ... ok
test tests_import_move::move_across_volumes_copies_then_removes ... ok
test tests_import_move::move_into_a_folder_template_with_sidecars ... ok
test tests_import_move::move_into_the_library_is_saved_before_sources_go ... ok
test tests_import_move::write_policy_timing ... ignored
test tests_layers::copy_paste_presets_and_auto_sync_carry_layers ... ok
test tests_layers::opacity_and_reset ... ok
test tests_layers::point_color_on_a_layer ... ok
test tests_layers::set_tools_merges_values_and_partials ... ok
test tests_libops::batch_rename_files_collisions_undo_and_replay ... ok
test tests_libops::capture_time_set_shift_undo_and_replay ... ok
test tests_libops::choosing_random_starts_a_fresh_shuffle ... ok
test tests_libops::date_groups_follow_sort_and_grouping ... ok
test tests_libops::import_review_preview_and_options ... ok
test tests_libops::keyword_rename_merge_delete_undo_and_replay ... ok
test tests_libops::keyword_sets_and_recent_keywords ... ok
test tests_libops::label_names_set_undo_and_replay ... ok
test tests_libops::label_sets_and_xmp_label_names ... ok
test tests_libops::random_sort_applies_to_albums_too ... ok
test tests_libops::random_sort_is_stable_until_reshuffled ... ok
test tests_libops::random_sort_rejects_bad_params ... ok
test tests_libops::rename_demo_photos_in_catalog ... ok
test tests_libops::tracklog_auto_tag ... ok
test tests_library::a_library_open_elsewhere_is_refused ... ok
test tests_library::close_writes_snapshot_and_view_state ... ok
test tests_library::edits_survive_restart_without_close ... ok
test tests_library::filter_any_of_several_labels ... ok
test tests_library::new_library_without_seed_is_empty_and_compacts ... ok
test tests_library::profile_favorites_and_recent_survive_reopen ... ok
test tests_library::quick_collection_and_target_album ... ok
test tests_library::scale_100k_library_queries ... ignored
test tests_library::smart_album_rule_sets ... ok
test tests_library::threshold_compaction_runs_in_the_background ... ok
test tests_library::thumbnails_come_from_the_disk_cache_after_restart ... ok
test tests_merge::hdr_merge_command_creates_and_imports_a_dng ... ok
test tests_merge::panorama_command_with_auto_crop ... ok
test tests_organize::album_source_reports_in_library_state_and_persists_the_view ... ok
test tests_organize::filter_chips_clear_one_filter_and_total_ignores_the_filter ... ok
test tests_organize::filters_are_not_restored_when_a_library_reopens ... ok
test tests_organize::random_sort_persists_with_the_view ... ok
test tests_organize::smart_album_from_smart_album_view_keeps_its_rules ... ok
test tests_organize::smart_album_from_view_updates_live_and_persists ... ok
test tests_organize::stacks_group_collapse_and_replay ... ok
test tests_organize::virtual_copies_are_independent_and_persist ... ok
test tests_persist::close_fails_when_nothing_can_be_saved ... ok
test tests_persist::close_saves_queued_changes_in_the_snapshot_when_appends_fail ... ok
test tests_persist::failed_append_fails_the_command_and_is_retried ... ok
test tests_persist::failed_append_of_undo_is_reported_and_retried ... ok
test tests_persist::failed_snapshot_does_not_fail_the_command ... ok
test tests_prefs::curve_presets_save_apply_export_import_and_persist ... ok
test tests_prefs::default_copyright_and_creator_fill_gaps_on_import ... ok
test tests_prefs::metadata_presets_apply_to_photos_and_imports ... ok
test tests_prefs::preferences_persist_with_the_library ... ok
test tests_prefs::raw_and_other_defaults_are_applied_on_import ... ok
test tests_prefs::reopening_a_library_retires_the_old_cache_so_its_jobs_cannot_write_after_a_clear ... ok
test tests_prefs::resizing_the_disk_cache_keeps_the_cache_and_its_thumbnails ... ok
test tests_save_over::demo_photos_and_virtual_copies_have_no_original_to_replace ... ok
test tests_save_over::ds_finding_add_export_imports_only_new_ids_and_reports_stack_failure ... ok
test tests_save_over::export_to_the_same_folder_as_the_original ... ok
test tests_save_over::failed_backup_or_replacement_leaves_original_and_edits_intact ... ok
test tests_save_over::raws_are_never_replaced_a_jpeg_goes_beside_them ... ok
test tests_save_over::save_copy_beside_keeps_the_original ... ok
test tests_save_over::save_over_preserves_source_profile_and_camera_metadata ... ok
test tests_save_over::save_over_resets_an_existing_manual_xmp_sidecar ... ok
test tests_save_over::save_over_source_profile_gpu_matches_cpu ... ok
test tests_save_over::save_over_webp_and_float_tiff_preserve_depth_and_profile ... ok
test tests_save_over::saving_over_a_jpeg_bakes_the_edits_and_keeps_a_backup ... ok
test tests_save_over::saving_over_png_and_tiff_keeps_format_and_bit_depth ... ok
test tests_segment::background_requests_never_wait_and_failures_come_back_as_messages ... ok
test tests_segment::object_clicks_are_capped ... ok
test tests_segment::status_and_model_commands_without_the_model ... ok
test tests_segment::waiting_requests_with_a_damaged_model_are_errors ... ok
test tests_settings_files::damaged_prefs_are_kept_aside_and_reported ... ok
test tests_settings_files::unreadable_settings_are_not_overwritten ... ok
test tests_spots::spots_select_move_edit_refresh_and_delete ... ok
test tests_toolset::default_raw_loading_matches_colour_tone_goldens ... ok
test tests_toolset::lens_database_corrections_follow_the_settings ... ok
test tests_toolset::new_raw_option_keys_and_preview_highlights ... ok
test tests_toolset::raw_options_change_the_decoding ... ok
test tests_toolset::sources_are_cached_per_raw_options ... ok
test tests_xmp::all_metadata_lists_exif_and_xmp ... ok
test tests_xmp::auto_write_and_naming_preference ... ok
test tests_xmp::convert_raw_to_dng ... ok
test tests_xmp::convert_to_dng_is_verified_and_atomic ... ok
test tests_xmp::copyright_status_usage_terms_and_url_round_trip ... ok
test tests_xmp::demo_photos_have_no_sidecar ... ok
test tests_xmp::dng_embedded_crs_settings_are_read_on_import ... ok
test tests_xmp::dng_export_compression_choices ... ok
test tests_xmp::duplicate_copies_the_file_and_the_edits ... ok
test tests_xmp::external_edit_copy_is_stacked ... ok
test tests_xmp::files_sharing_a_stem_get_their_own_sidecars ... ok
test tests_xmp::foreign_crs_sidecar_on_import_and_read_from_file ... ok
test tests_xmp::gps_typed_in_is_saved_to_xmp ... ok
test tests_xmp::regions_from_sidecar_are_read_on_import ... ok
test tests_xmp::reload_picks_up_changed_files ... ok
test tests_xmp::removing_a_region_is_undoable_and_leaves_the_sidecar_alone ... ok
test tests_xmp::rereading_a_sidecar_clears_regions_only_when_it_states_them ... ok
test tests_xmp::resizing_a_region_validates_clamps_undoes_and_leaves_the_sidecar_alone ... ok
test tests_xmp::save_delete_library_reimport_restores_edits ... ok
test tests_xmp::saving_merges_into_another_apps_sidecar ... ok
test tests_xmp::sidecar_capture_time_fills_in_when_the_file_has_none ... ok

test result: ok. 457 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 306.30s

/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner: line 13: total +  : arithmetic syntax error: operand expected (error token is "+  ")
/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner: line 18: [: : integer expected
error: 1 target failed:
    `-p li-ai --lib`
```

</details>

<details>
<summary>Corrected AI regressions — exit 0</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p li-ai --lib final_review_ -- --test-threads=1
```

```text
    Blocking waiting for file lock on build directory
   Compiling smallvec v1.16.2
   Compiling memchr v2.8.3
   Compiling hashbrown v0.16.1
   Compiling once_cell v1.21.4
   Compiling bytemuck v1.25.2
   Compiling parking_lot_core v0.9.12
   Compiling num-bigint-dig v0.8.6
   Compiling image v0.25.10
   Compiling rsa v0.9.10
   Compiling string-interner v0.20.0
   Compiling rustls v0.23.45
   Compiling nom v8.0.0
   Compiling serde_json v1.0.151
   Compiling parking_lot v0.12.5
   Compiling rustls-rustcrypto v0.0.2-alpha
   Compiling ureq v3.4.2
   Compiling nom-language v0.1.0
   Compiling tract-data v0.23.8
   Compiling tract-linalg v0.23.8
   Compiling tract-core v0.23.8
   Compiling tract-nnef v0.23.8
   Compiling tract-hir v0.23.8
   Compiling tract-extra v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling tract-onnx-opl v0.23.8
   Compiling tract-onnx v0.23.8
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-seg)
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Finished `test` profile [optimized + debuginfo] target(s) in 21m 15s
     Running unittests src/lib.rs (target/debug/deps/li_ai-6d31bc357dfba8f1)

running 12 tests
test catalog::final_review_realvis_lightning_uses_author_sampling_profile ... ok
test ops::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels ... ok
test refine::tests::final_review_checker_rejects_inventory_change_after_submission ... ok
test refine::tests::final_review_checker_rejects_model_swap_after_submission ... ok
test refine::tests::final_review_checker_rejects_model_swap_before_cached_publication ... ok
test refine::tests::final_review_effective_metadata_resolves_saved_defaults_and_honours_explicit_values ... ok
test refine::tests::final_review_family_styles_are_preserved_or_rejected_explicitly ... ok
test refine::tests::final_review_hash_detects_same_size_rewrite_with_restored_mtime ... ok
test refine::tests::final_review_recipes_never_silently_drop_styles ... ok
test refine::tests::final_review_refine_keeps_explicit_values_equal_to_nominal_defaults ... ok
test refine::tests::final_review_second_pass_metadata_uses_resolved_strength_and_explicit_options ... ok
test refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 146 filtered out; finished in 12.96s

```

</details>

<details>
<summary>Existing and new AI progress bridge tests — exit 0</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p photocraft-engine --lib ai_cmds:: -- --test-threads=1
```

```text
   Compiling rustix v1.1.5
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-develop)
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-pipeline)
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-catalog)
   Compiling xattr v1.6.1
   Compiling tar v0.4.46
   Compiling tract-nnef v0.23.8
   Compiling tract-extra v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling tract-onnx-opl v0.23.8
   Compiling tract-onnx v0.23.8
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-seg)
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-gpu)
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-io)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
    Finished `test` profile [optimized + debuginfo] target(s) in 8m 21s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-32901a71184913f6)

running 8 tests
test ai_cmds::ds_finding_mask_index_exceeds_u32_without_wrapping ... ok
test ai_cmds::tests::ai_commands_against_the_mock_server ... ok
test ai_cmds::tests::ai_commands_fail_cleanly_without_a_document_or_engine ... ok
test ai_cmds::tests::background_output_qwen_request_samples_the_target_and_keeps_hint_and_variant ... ok
test ai_cmds::tests::explicit_removal_mask_matches_document_and_preserves_subtracted_holes ... ok
test ai_cmds::tests::final_review_bridge_stops_on_unwind_and_preserves_results ... ok
test ai_cmds::tests::generation_size_keeps_proportions ... ok
test ai_cmds::tests::placed_generated_images_keep_their_generation_settings ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 1311 filtered out; finished in 7.89s

```

</details>

<details>
<summary>Initial generation UI fixture compile — exit 101</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p photocraft-ui-egui --lib -- generate_ui:: i18n:: --test-threads=1
```

```text
    Blocking waiting for file lock on build directory
   Compiling serde_core v1.0.229
   Compiling rustix v1.1.5
   Compiling linux-raw-sys v0.12.1
   Compiling syn v3.0.6
   Compiling syn v2.0.119
   Compiling serde v1.0.229
   Compiling pkg-config v0.3.34
   Compiling dlib v0.5.3
   Compiling winnow v1.0.4
   Compiling slab v0.4.12
   Compiling hashbrown v0.17.1
   Compiling wayland-backend v0.3.17
   Compiling scoped-tls v1.0.1
   Compiling parking v2.2.1
   Compiling downcast-rs v1.2.1
   Compiling uuid v1.26.1
   Compiling quick-xml v0.41.0
   Compiling wayland-client v0.31.15
   Compiling futures-core v0.3.34
   Compiling endi v1.1.1
   Compiling linebender_resource_handle v0.1.1
   Compiling futures-io v0.3.34
   Compiling concurrent-queue v2.5.0
   Compiling naga v30.0.1
   Compiling wayland-scanner v0.31.11
   Compiling async-io v2.6.0
   Compiling event-listener v5.4.2
   Compiling khronos-egl v6.0.0
   Compiling wayland-sys v0.31.11
   Compiling indexmap v2.14.2
   Compiling synstructure v0.14.0
   Compiling accesskit v0.24.1
   Compiling wgpu-hal v30.0.1
   Compiling errno v0.3.14
   Compiling async-task v4.7.1
   Compiling futures-lite v2.6.1
   Compiling signal-hook-registry v1.4.8
   Compiling ahash v0.8.12
   Compiling glow v0.17.0
   Compiling atomic-waker v1.1.2
   Compiling polling v3.11.0
   Compiling xattr v1.6.1
   Compiling piper v0.2.5
   Compiling tar v0.4.46
   Compiling wgpu-core v30.0.1
   Compiling euclid v0.22.14
   Compiling zerocopy-derive v0.8.59
   Compiling zeroize_derive v1.5.0
   Compiling tracing-attributes v0.1.31
   Compiling serde_derive v1.0.229
   Compiling bytemuck_derive v1.12.1
   Compiling thiserror-impl v2.0.21
   Compiling zerofrom-derive v0.1.8
   Compiling zeroize v1.9.1
   Compiling generic-array v0.14.7
   Compiling yoke-derive v0.8.3
   Compiling enumflags2_derive v0.7.12
   Compiling bytemuck v1.25.2
   Compiling fax_derive v0.2.0
   Compiling toml_parser v1.1.3+spec-1.1.0
   Compiling profiling-procmacros v1.0.18
   Compiling num-complex v0.4.6
   Compiling qoi v0.4.1
   Compiling font-types v0.12.5
   Compiling zerovec-derive v0.11.6
   Compiling displaydoc v0.2.7
   Compiling zerocopy v0.8.59
   Compiling der v0.7.10
   Compiling ndarray v0.17.2
   Compiling read-fonts v0.41.0
   Compiling profiling v1.0.18
   Compiling fax v0.2.6
   Compiling event-listener-strategy v0.5.4
   Compiling derive-new v0.7.0
   Compiling rustfft v6.4.1
   Compiling crypto-common v0.1.7
   Compiling thiserror v2.0.21
   Compiling tracing v0.1.44
   Compiling block-buffer v0.10.4
   Compiling naga-types v30.0.1
   Compiling skrifa v0.44.0
   Compiling spki v0.7.3
   Compiling gpu-allocator v0.28.0
   Compiling pkcs5 v0.7.1
   Compiling jxl-bitstream v1.1.0
   Compiling digest v0.10.7
   Compiling zerofrom v0.1.8
   Compiling wgpu-types v30.0.1
   Compiling jxl-oxide-common v1.0.0
   Compiling jxl-grid v0.6.2
   Compiling jxl-coding v1.0.1
   Compiling pkcs8 v0.10.2
   Compiling smallvec v1.16.2
   Compiling arrayvec v0.7.8
   Compiling photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-geom)
   Compiling photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-color)
   Compiling lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-geom)
   Compiling parking_lot_core v0.9.12
   Compiling yoke v0.8.3
   Compiling harfrust v0.12.0
   Compiling string-interner v0.20.0
   Compiling parking_lot v0.12.5
   Compiling zerovec v0.11.8
   Compiling zvariant_utils v4.2.0
   Compiling enumflags2 v0.7.12
   Compiling half v2.7.1
   Compiling polycool v0.4.0
   Compiling photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raster)
   Compiling kurbo v0.13.1
   Compiling tiff v0.11.3
   Compiling tinystr v0.8.4
   Compiling zcheapstr v1.1.0
   Compiling potential_utf v0.1.6
   Compiling image v0.25.10
   Compiling zerotrie v0.2.5
   Compiling lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-color)
   Compiling toml_datetime v1.1.1+spec-1.1.0
   Compiling toml_edit v0.25.15+spec-1.1.0
   Compiling minijinja v2.24.0
   Compiling icu_locale_core v2.3.0
   Compiling jxl-threadpool v1.0.0
   Compiling async-channel v2.5.0
   Compiling photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-doc)
   Compiling tract-data v0.23.8
   Compiling equator-macro v0.4.2
   Compiling icu_provider v2.3.1
   Compiling proc-macro-crate v3.5.0
   Compiling wgpu-naga-bridge v30.0.1
   Compiling async-signal v0.2.14
   Compiling icu_collections v2.3.0
   Compiling sha2 v0.10.9
   Compiling async-lock v3.4.2
   Compiling color v0.3.3
   Compiling equator v0.4.2
   Compiling rustix v0.38.44
   Compiling thiserror v1.0.69
   Compiling aligned-vec v0.6.4
   Compiling photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-vector)
   Compiling blocking v1.7.0
   Compiling jxl-modular v0.11.3
   Compiling lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raster)
   Compiling ppv-lite86 v0.2.21
   Compiling peniko v0.6.1
   Compiling wgpu-core-deps-windows-linux-android v30.0.1
   Compiling zvariant_derive v5.15.0
   Compiling async-process v2.5.0
   Compiling hmac v0.12.1
   Compiling jxl-image v0.13.0
   Compiling async-broadcast v0.7.2
   Compiling guillotiere v0.7.0
   Compiling async-trait v0.1.92
   Compiling serde_repr v0.1.21
   Compiling thiserror-impl v1.0.69
   Compiling phf_macros v0.13.1
   Compiling async-recursion v1.1.1
   Compiling async-executor v1.14.0
   Compiling ordered-stream v0.2.0
   Compiling zvariant v5.15.0
   Compiling zbus_names v4.3.4
   Compiling wayland-protocols v0.32.13
   Compiling phf_shared v0.13.1
   Compiling wgpu v30.0.1
   Compiling cursor-icon v1.2.0
   Compiling unicode-general-category v1.1.0
   Compiling zbus_xml v5.2.1
   Compiling linux-raw-sys v0.4.15
   Compiling xkeysym v0.2.1
   Compiling xcursor v0.3.11
   Compiling zbus_macros v5.19.0
   Compiling wayland-csd-frame v0.3.0
   Compiling phf v0.13.1
   Compiling wayland-cursor v0.31.14
   Compiling jxl-vardct v0.11.1
   Compiling v_frame v0.3.9
   Compiling tract-linalg v0.23.8
   Compiling wayland-protocols-wlr v0.3.12
   Compiling signature v2.2.0
   Compiling inout v0.1.4
   Compiling zbus-lockstep v0.5.2
   Compiling vello_common v0.1.0
   Compiling emath v0.36.2
   Compiling sec1 v0.7.3
   Compiling jxl-frame v0.13.3
   Compiling zbus-lockstep-macros v0.5.2
   Compiling ecolor v0.36.2
   Compiling vello_cpu v0.1.0
   Compiling calloop v0.13.0
   Compiling icu_properties v2.3.0
   Compiling tract-core v0.23.8
   Compiling cipher v0.4.4
   Compiling icu_locale_fallback v2.3.0
   Compiling hkdf v0.12.4
   Compiling universal-hash v0.5.1
   Compiling crypto-bigint v0.5.5
   Compiling font-types v0.11.3
   Compiling rgb v0.8.53
   Compiling rustls-pki-types v1.15.1
   Compiling arg_enum_proc_macro v0.3.4
   Compiling prost-derive v0.14.4
   Compiling gethostname v1.1.0
   Compiling x11-dl v2.21.0
   Compiling unicode-segmentation v1.13.3
   Compiling epaint_default_fonts v0.36.2
   Compiling self_cell v1.3.0
   Compiling nohash-hasher v0.2.0
   Compiling smithay-client-toolkit v0.19.2
   Compiling x11rb-protocol v0.13.2
   Compiling as-raw-xcb-connection v1.0.1
   Compiling fontique v0.11.1
   Compiling calloop-wayland-source v0.3.0
   Compiling icu_segmenter v2.3.0
   Compiling av1-grain v0.2.5
   Compiling av-scenechange v0.14.1
   Compiling epaint v0.36.2
   Compiling zbus v5.19.0
   Compiling read-fonts v0.39.2
   Compiling prost v0.14.4
   Compiling icu_normalizer v2.3.0
   Compiling parley_data v0.11.1
   Compiling jxl-color v0.11.0
   Compiling lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-tiff)
   Compiling num-derive v0.4.2
   Compiling winit v0.30.13
   Compiling lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-meta)
   Compiling atspi-common v0.13.0
   Compiling tract-nnef v0.23.8
   Compiling x11rb v0.13.2
   Compiling tract-hir v0.23.8
   Compiling parley v0.11.1
   Compiling elliptic-curve v0.13.8
   Compiling egui v0.36.2
   Compiling atspi-proxies v0.13.0
   Compiling jxl-jbr v0.2.1
   Compiling tract-extra v0.23.8
   Compiling tract-transformers v0.23.8
   Compiling wayland-protocols-plasma v0.3.12
   Compiling xkbcommon-dl v0.4.2
   Compiling tiny-skia-path v0.11.4
   Compiling rfc6979 v0.4.0
   Compiling rand_chacha v0.3.1
   Compiling lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-develop)
   Compiling kurbo v0.11.3
   Compiling tract-onnx-opl v0.23.8
   Compiling rav1e v0.8.1
   Compiling avif-serialize v0.8.9
   Compiling calloop v0.14.4
   Compiling photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-psd)
   Compiling accesskit_consumer v0.36.0
   Compiling futures-macro v0.3.34
   Compiling curve25519-dalek-derive v0.1.1
   Compiling smithay-client-toolkit v0.20.0
   Compiling futures-task v0.3.34
   Compiling smol_str v0.2.2
   Compiling jxl-render v0.12.4
   Compiling dpi v0.1.2
   Compiling pulp v0.22.3
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-text)
   Compiling jxl-oxide v0.12.6
   Compiling curve25519-dalek v4.1.3
   Compiling futures-util v0.3.34
   Compiling ravif v0.13.0
   Compiling accesskit_atspi_common v0.18.1
   Compiling calloop-wayland-source v0.4.1
   Compiling rand v0.8.8
   Compiling svgtypes v0.15.3
   Compiling ecdsa v0.16.9
   Compiling atspi v0.29.0
   Compiling primeorder v0.13.6
   Compiling skrifa v0.42.1
   Compiling serde_json v1.0.151
   Compiling tract-onnx v0.23.8
   Compiling rustls-webpki v0.103.15
   Compiling li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-seg)
   Compiling polyval v0.6.2
   Compiling wayland-protocols-experimental v20250721.0.1
   Compiling wayland-protocols-misc v0.3.12
   Compiling photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-compose)
   Compiling linesweeper v0.4.0
   Compiling aead v0.5.2
   Compiling usvg v0.45.1
   Compiling exr v1.74.2
   Compiling lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-codecs)
   Compiling num-bigint-dig v0.8.6
   Compiling rand_chacha v0.9.0
   Compiling photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-pathops)
   Compiling ghash v0.5.1
   Compiling accesskit_unix v0.21.1
   Compiling lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-pipeline)
   Compiling lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raw)
   Compiling rustls v0.23.45
   Compiling write-fonts v0.48.1
   Compiling poly1305 v0.8.0
   Compiling aes v0.8.4
   Compiling chacha20 v0.9.1
   Compiling ctr v0.9.2
   Compiling ed25519 v2.2.3
   Compiling lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-scenes)
   Compiling pkcs1 v0.7.5
   Compiling bitflags v1.3.2
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling blake3 v1.8.7
   Compiling png v0.17.16
   Compiling chacha20poly1305 v0.10.1
   Compiling photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-codecs)
   Compiling hayro-syntax v0.7.2
   Compiling smithay-clipboard v0.7.3
   Compiling accesskit_winit v0.32.2
   Compiling lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-gpu)
   Compiling subsetter v0.2.6
   Compiling lensfun v0.7.0
   Compiling p384 v0.13.1
   Compiling p256 v0.13.2
   Compiling ed25519-dalek v2.2.0
   Compiling rand v0.9.5
   Compiling lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-catalog)
   Compiling x25519-dalek v2.0.1
   Compiling aes-gcm v0.10.3
   Compiling lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-preview)
   Compiling arboard v3.6.1
   Compiling rustls-webpki v0.102.8
   Compiling webpki-roots v1.0.9
   Compiling lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-merge)
   Compiling tiny-skia-path v0.12.0
   Compiling rsa v0.9.10
   Compiling photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-algo)
   Compiling serde_spanned v1.1.1
   Compiling sha1 v0.10.7
   Compiling type-map v0.5.1
   Compiling is-docker v0.2.0
   Compiling toml_writer v1.1.2+spec-1.1.0
   Compiling ureq v3.4.2
   Compiling krilla v0.8.2
   Compiling egui-wgpu v0.36.2
   Compiling egui-winit v0.36.2
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
   Compiling tungstenite v0.27.0
   Compiling is-wsl v0.4.0
   Compiling toml v1.1.6+spec-1.1.0
   Compiling photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-format)
   Compiling hayro-interpret v0.7.0
   Compiling li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-attributions)
   Compiling photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-paint)
   Compiling accesskit_consumer v0.35.0
   Compiling tiny-skia v0.11.4
   Compiling rustls-rustcrypto v0.0.2-alpha
   Compiling enum-map-derive v0.17.0
   Compiling getopts v0.2.24
   Compiling colored v2.2.0
   Compiling photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raw)
   Compiling rfd v0.17.2
   Compiling enum-map v2.7.3
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
   Compiling resvg v0.45.1
   Compiling eframe v0.36.2
   Compiling photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-pdf)
   Compiling dify v0.8.0
   Compiling photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-io)
   Compiling kittest v0.4.0
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
   Compiling open v5.4.4
   Compiling photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-plugins)
   Compiling photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-svg)
   Compiling photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-vecfx)
   Compiling photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-affinity)
   Compiling pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-trace)
   Compiling photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ops)
   Compiling tempfile v3.27.0
   Compiling memory-stats v1.2.0
   Compiling pollster v0.4.0
   Compiling egui_extras v0.36.2
   Compiling photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-testkit)
   Compiling egui_kittest v0.36.2
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
   Compiling photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-gpu)
   Compiling sys-locale v0.3.2
error[E0599]: no associated function or constant named `default` found for struct `PhotocraftApp` in the current scope
    --> crates/pc-ui-egui/src/generate_ui.rs:3378:34
     |
3378 |     let mut app = PhotocraftApp::default();
     |                                  ^^^^^^^ associated function or constant not found in `PhotocraftApp`
     |
    ::: crates/pc-ui-egui/src/lib.rs:399:1
     |
 399 | pub struct PhotocraftApp {
     | ------------------------ associated function or constant `default` not found for this struct
     |
note: if you're trying to build a new `PhotocraftApp`, consider using `PhotocraftApp::new` which returns `PhotocraftApp`
    --> crates/pc-ui-egui/src/lib.rs:602:5
     |
 602 |     pub fn new(session: Session, services: Services) -> Self {
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     = help: items from traits can only be used if the trait is implemented and in scope
     = note: the following traits define an item `default`, perhaps you need to implement one of them:
             candidate #1: `harfrust::hb::cache::AtomicStorage`
             candidate #2: `std::default::Default`

error[E0599]: no associated function or constant named `default` found for struct `PhotocraftApp` in the current scope
    --> crates/pc-ui-egui/src/generate_ui.rs:3423:34
     |
3423 |     let mut app = PhotocraftApp::default();
     |                                  ^^^^^^^ associated function or constant not found in `PhotocraftApp`
     |
    ::: crates/pc-ui-egui/src/lib.rs:399:1
     |
 399 | pub struct PhotocraftApp {
     | ------------------------ associated function or constant `default` not found for this struct
     |
note: if you're trying to build a new `PhotocraftApp`, consider using `PhotocraftApp::new` which returns `PhotocraftApp`
    --> crates/pc-ui-egui/src/lib.rs:602:5
     |
 602 |     pub fn new(session: Session, services: Services) -> Self {
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     = help: items from traits can only be used if the trait is implemented and in scope
     = note: the following traits define an item `default`, perhaps you need to implement one of them:
             candidate #1: `harfrust::hb::cache::AtomicStorage`
             candidate #2: `std::default::Default`

For more information about this error, try `rustc --explain E0599`.
error: could not compile `photocraft-ui-egui` (lib test) due to 2 previous errors
```

</details>

<details>
<summary>Existing and new generation UI and i18n tests, headless — exit 101</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p photocraft-ui-egui --lib -- generate_ui:: i18n:: --test-threads=1
```

```text
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 12m 08s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-219d99becdc1f52f)

running 73 tests
test generate_ui::draft_refine::tests::autodetect_refine_picker_disables_failure_and_click_retest_verifies_on_worker_gpu ... ok
test generate_ui::draft_refine::tests::draft_refine_click_carries_current_composite_selected_layer_and_draft_pixels ... ok
test generate_ui::draft_refine::tests::draft_refine_dock_request_keeps_open_image_and_drops_hidden_create_references ... ok
test generate_ui::draft_refine::tests::draft_refine_fresh_and_changed_refinement_model_use_per_model_denoise ... ok
test generate_ui::draft_refine::tests::draft_refine_gpu_design_system_all_themes_and_resolutions ... ok
test generate_ui::draft_refine::tests::draft_refine_large_previews_and_window_open_at_both_resolutions ... ok
test generate_ui::draft_refine::tests::draft_refine_library_use_and_open_actions_are_one_undo_and_a_new_document ... ok
test generate_ui::draft_refine::tests::draft_refine_lora_picker_filters_family_and_carries_strength_in_request ... ok
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... FAILED
test generate_ui::draft_refine::tests::draft_refine_properties_sections_collapse_and_sampling_has_no_offline_ceiling ... ok
test generate_ui::draft_refine::tests::draft_refine_sampling_is_editable_and_bounded_only_by_comfyui ... ok
test generate_ui::draft_refine::tests::draft_refine_shared_aspect_presets_lock_and_model_grid ... ok
test generate_ui::draft_refine::tests::draft_refine_zoom_pan_comparison_and_saved_defaults ... ok
test generate_ui::draft_refine::tests::ds_finding_mock_request_guard_clears_during_unwind ... ok
test generate_ui::draft_refine::tests::zimage_turbo_variety_tip_follows_picker_clicks_in_generate_and_draft_gpu ... ok
test generate_ui::ds2_refine_lora_preset_roundtrip_and_invalid_step_limits ... ok
test generate_ui::ds2_restored_upscale_keeps_non_official_model_choice ... ok
test generate_ui::ds_finding_library_snapshots_share_entries_until_removal ... ok
test generate_ui::ds_finding_refine_preset_persists_custom_sampler_and_loads_old_defaults ... ok
test generate_ui::final_review_history_persists_effective_sampling_and_replays_it_explicitly ... ok
test generate_ui::final_review_history_replay_preserves_the_resolved_second_pass ... ok
test generate_ui::final_review_unsupported_recipe_picker_explains_and_can_clear_styles ... ok
test generate_ui::tests::aspect_sizes_are_about_a_megapixel_on_the_grid ... ok
test generate_ui::tests::ds_finding_selected_layer_refine_is_disabled_without_a_selected_layer ... ok
test generate_ui::tests::enhance_generate_upscale_clicks_three_methods_and_scale ... ok
test generate_ui::tests::model_tags_do_not_call_models_loras ... ok
test generate_ui::tests::natural_order ... ok
test generate_ui::tests::refine_model_groups_exclude_upscalers_tools_and_video ... ok
test generate_ui::tests::refine_panel_mode_and_second_pass_use_model_defaults_and_grouped_picker ... ok
test generate_ui::tests::refine_with_lora_picker_and_precision_carry_the_second_pass_settings ... ok
test generate_ui::tests::steps_and_guidance_follow_the_backend_limits_only ... ok
test generate_ui::tests::styles_row_opens_the_lora_browser ... ok
test generate_ui::tests::upscale_offers_the_official_upscalers_first ... ok
test generate_ui::zimage_variety_settings_presets_and_requests_keep_old_defaults ... ok
test i18n::live_tests::a_control_request_changes_the_drawing_language_in_the_same_frame ... ok
test i18n::live_tests::an_explicit_language_survives_restart_on_a_different_system_language ... ok
test i18n::live_tests::cjk_font_order_follows_the_selected_language ... ok
test i18n::live_tests::drawing_languages_are_isolated_between_threads ... ok
test i18n::live_tests::dynamic_brush_sections_have_translations_and_draw_in_the_selected_language ... ok
test i18n::live_tests::first_launch_renders_the_system_language_without_overwriting_auto ... ok
test i18n::live_tests::french_and_korean_plural_messages_keep_placeholders ... ok
test i18n::live_tests::keyboard_shortcuts_list_commands_in_the_selected_language ... ok
test i18n::live_tests::korean_glyphs_remain_available_after_repeated_language_switches ... ok
test i18n::live_tests::missing_or_unsupported_system_languages_render_english ... ok
test i18n::live_tests::older_preferences_without_a_language_follow_the_system ... ok
test i18n::live_tests::preferences_preview_does_not_commit_until_applied_and_survives_reload ... ok
test i18n::live_tests::requested_languages_resolve_regional_preferences ... ok
test i18n::live_tests::scoped_preview_restores_language_even_after_a_panic ... ok
test i18n::system::tests::native_locale_results_are_bounded_and_trimmed ... ok
test i18n::system::tests::system_preferences_are_negotiated_in_order ... ok
test i18n::system::tests::test_system_preferences_restore_and_remain_thread_local ... ok
test i18n::tests::blend_mode_names_are_translated ... ok
test i18n::tests::brazilian_portuguese_resolves_and_pluralises ... ok
test i18n::tests::brush_section_names_are_translated ... ok
test i18n::tests::bundled_catalogs_are_consistent ... ok
test i18n::tests::camera_raw_labels_are_translated_in_every_available_language ... ok
test i18n::tests::candidates_walk_from_specific_to_general ... ok
test i18n::tests::catalog_kinds_are_parsed_and_looked_up ... ok
test i18n::tests::complete_languages_translate_every_menu_string ... ok
test i18n::tests::czech_plural_rule ... ok
test i18n::tests::every_tl_literal_is_translated ... ok
test i18n::tests::french_resolves_and_pluralises ... ok
test i18n::tests::indonesian_tags_resolve ... ok
test i18n::tests::korean_uses_photoshop_terminology ... ok
test i18n::tests::lookups_fall_back_to_english ... ok
test i18n::tests::malformed_lines_are_reported_not_fatal ... ok
test i18n::tests::mixer_brush_ui_strings_have_translations_in_every_registered_language ... ok
test i18n::tests::plurals_and_placeholders ... ok
test i18n::tests::preferences_resolve_with_fallback ... ok
test i18n::tests::russian_plural_rules ... ok
test i18n::tests::simplified_chinese_covers_dynamic_shortcuts_and_layer_counts ... ok
test i18n::tests::spanish_resolves_and_pluralises ... ok
test i18n::tests::tags_map_to_languages ... ok

failures:

---- generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened stdout ----

thread 'generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened' (729160) panicked at crates/pc-ui-egui/src/generate_ui/draft_refine/tests.rs:399:5:
Generation failed: Model inventory changed during refinement testing; retry.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened

test result: FAILED. 72 passed; 1 failed; 0 ignored; 0 measured; 1256 filtered out; finished in 47.13s

error: test failed, to rerun pass `-p photocraft-ui-egui --lib`
```

</details>

<details>
<summary>Workspace check — exit 0</summary>

```bash
cargo check --offline --workspace
```

```text
    Blocking waiting for file lock on build directory
    Checking cfg-if v1.0.5
    Checking log v0.4.34
    Checking once_cell v1.21.4
    Checking memchr v2.8.3
    Checking either v1.18.0
    Checking serde_core v1.0.229
    Checking libc v0.2.189
    Checking libm v0.2.16
    Checking crossbeam-utils v0.8.23
    Checking foldhash v0.2.0
   Compiling rustix v1.1.5
    Checking bytemuck v1.25.2
    Checking zerocopy v0.8.59
    Checking zeroize v1.9.1
    Checking linux-raw-sys v0.12.1
    Checking thiserror v2.0.21
    Checking simd-adler32 v0.3.10
    Checking pin-project-lite v0.2.17
    Checking equivalent v1.0.2
    Checking stable_deref_trait v1.2.1
    Checking adler2 v2.0.1
    Checking typenum v1.20.1
    Checking itoa v1.0.18
    Checking num-traits v0.2.19
    Checking hashbrown v0.17.1
   Compiling serde_json v1.0.151
    Checking subtle v2.6.1
    Checking ppv-lite86 v0.2.21
    Checking zmij v1.0.23
    Checking tracing-core v0.1.36
    Checking tracing v0.1.44
    Checking num-integer v0.1.47
    Checking allocator-api2 v0.2.21
    Checking const-oid v0.9.6
    Checking unicode-ident v1.0.26
    Checking scopeguard v1.2.0
    Checking rand_core v0.10.1
    Checking nom v8.0.0
    Checking cpufeatures v0.2.17
    Checking zune-core v0.5.3
    Checking num-complex v0.4.6
    Checking fdeflate v0.3.7
    Checking spin v0.9.9
    Checking slab v0.4.12
    Checking zerofrom v0.1.8
    Checking anyhow v1.0.104
    Checking futures-core v0.3.34
    Checking serde v1.0.229
    Checking bitflags v2.13.2
    Checking smallvec v1.16.2
    Checking arrayvec v0.7.8
    Checking hashbrown v0.16.1
    Checking indexmap v2.14.2
    Checking lock_api v0.4.14
    Checking memmap2 v0.9.11
    Checking miniz_oxide v0.9.1
    Checking miniz_oxide v0.8.9
    Checking getrandom v0.3.4
    Checking libloading v0.8.9
    Checking crc32fast v1.5.2
    Checking parking_lot_core v0.9.12
    Checking zune-jpeg v0.5.15
    Checking yoke v0.8.3
    Checking quick-error v2.0.1
    Checking getrandom v0.2.17
    Checking photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-geom)
    Checking typeid v1.0.3
    Checking flate2 v1.1.10
    Checking itertools v0.15.0
    Checking pxfm v0.1.30
    Checking weezl v0.1.12
    Checking profiling v1.0.18
    Checking generic-array v0.14.7
    Checking crypto-common v0.1.7
    Checking rand_core v0.9.5
    Checking dlib v0.5.3
    Checking wayland-sys v0.31.11
    Checking block-buffer v0.10.4
    Checking byteorder-lite v0.1.0
    Checking static_assertions v1.1.0
    Checking color_quant v1.1.0
    Checking parking_lot v0.12.5
    Checking fax v0.2.6
    Checking moxcms v0.8.1
    Checking base64ct v1.8.3
    Checking gif v0.14.2
    Checking siphasher v1.0.4
    Checking image-webp v0.2.4
    Checking qoi v0.4.1
    Checking futures-io v0.3.34
    Checking rand_core v0.6.4
    Checking font-types v0.12.5
    Checking pem-rfc7468 v0.7.0
    Checking der v0.7.10
    Checking png v0.18.1
    Checking scoped-tls v1.0.1
    Checking raw-window-handle v0.6.2
    Checking rawpointer v0.2.1
    Checking crossbeam-epoch v0.9.21
    Checking crossbeam-deque v0.8.8
    Checking rayon-core v1.13.0
    Checking rayon v1.12.0
    Checking getrandom v0.4.3
    Checking byteorder v1.5.0
    Checking dyn-clone v1.0.20
    Checking downcast-rs v1.2.1
    Checking linebender_resource_handle v0.1.1
    Checking polling v3.11.0
    Checking bytes v1.12.1
    Checking zerovec v0.11.8
    Checking string-interner v0.20.0
    Checking jxl-bitstream v1.1.0
    Checking lazy_static v1.5.0
    Checking lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-geom)
    Checking nom-language v0.1.0
    Checking strength_reduce v0.2.4
    Checking parking v2.2.1
    Checking inventory v0.3.25
    Checking wayland-backend v0.3.17
    Checking downcast-rs v2.0.2
    Checking maplit v1.0.2
    Checking rustc-hash v1.1.0
    Checking dyn-hash v1.0.0
    Checking naga-types v30.0.1
    Checking matrixmultiply v0.3.11
    Checking wayland-client v0.31.15
    Checking transpose v0.2.3
    Checking photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-cms)
    Checking photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-color)
    Checking rand_chacha v0.9.0
    Checking rand v0.9.5
    Checking raw-cpuid v11.6.0
    Checking primal-check v0.3.4
    Checking num-bigint v0.4.8
    Checking bit-vec v0.10.1
    Checking digest v0.10.7
    Checking reborrow v0.5.5
    Checking pulp-wasm-simd-flag v0.1.1
    Checking fearless_simd v0.4.1
    Checking unicode-width v0.2.2
    Checking bit-vec v0.9.1
    Checking writeable v0.6.4
    Checking bit-set v0.10.0
    Checking codespan-reporting v0.13.1
    Checking bit-set v0.11.1
    Checking num-rational v0.4.2
    Checking rustfft v6.4.1
    Checking erased-serde v0.4.10
    Checking uuid v1.26.1
    Checking sha2 v0.10.9
    Checking lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-color)
    Checking ndarray v0.17.2
    Checking xattr v1.6.1
    Checking spirv v0.4.0+sdk-1.4.341.0
    Checking jxl-grid v0.6.2
    Checking filetime v0.2.29
    Checking anymap3 v1.1.0
    Checking cpufeatures v0.3.1
    Checking alloc-no-stdlib v2.0.4
    Checking read-fonts v0.41.0
    Checking ash v0.38.0+1.3.281
    Checking wgpu-types v30.0.1
    Checking pulp v0.22.3
    Checking tinystr v0.8.4
    Checking rand_distr v0.5.1
    Checking half v2.7.1
    Checking tiff v0.11.3
    Checking tract-data v0.23.8
    Checking tract-linalg v0.23.8
    Checking jxl-threadpool v1.0.0
    Checking presser v0.3.1
    Checking same-file v1.0.6
    Checking litemap v0.8.3
    Checking winnow v1.0.4
    Checking potential_utf v0.1.6
    Checking gpu-allocator v0.28.0
    Checking equator v0.4.2
    Checking khronos-egl v6.0.0
    Checking hmac v0.12.1
    Checking zerotrie v0.2.5
    Checking proc-macro2 v1.0.107
    Checking ordered-float v5.5.0
    Checking rustc-hash v2.1.3
    Checking fastrand v2.5.0
    Checking glow v0.17.0
    Checking photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raster)
    Checking polycool v0.4.0
    Checking tar v0.4.46
    Checking naga v30.0.1
    Checking walkdir v2.5.0
    Checking kurbo v0.13.1
    Checking renderdoc-sys v1.1.0
    Checking quote v1.0.47
    Checking aligned-vec v0.6.4
    Checking chacha20 v0.10.2
    Checking lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raster)
    Checking tract-core v0.23.8
    Checking wgpu-naga-bridge v30.0.1
    Checking signature v2.2.0
    Checking wayland-protocols v0.32.13
    Checking event-listener v5.4.2
    Checking concurrent-queue v2.5.0
    Checking ff v0.13.1
    Checking inout v0.1.4
    Checking float-cmp v0.9.0
    Checking hex v0.4.3
    Checking icu_locale_core v2.3.0
    Checking icu_provider v2.3.1
    Checking base16ct v0.2.0
    Checking utf8_iter v1.0.4
    Checking icu_collections v2.3.0
    Checking harfrust v0.12.0
    Checking photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-doc)
    Checking skrifa v0.44.0
    Checking strict-num v0.1.1
    Checking cipher v0.4.4
    Checking group v0.13.0
    Checking hkdf v0.12.4
    Checking rand v0.10.3
    Checking syn v3.0.6
    Checking universal-hash v0.5.1
    Checking alloc-stdlib v0.2.4
    Checking crypto-bigint v0.5.5
    Checking rustls-pki-types v1.15.1
    Checking as-slice v0.2.1
    Checking brotli-decompressor v5.0.3
    Checking opaque-debug v0.3.1
    Checking v_frame v0.3.9
    Checking percent-encoding v2.3.2
    Checking arrayref v0.3.9
    Checking zvariant_utils v4.2.0
    Checking aligned v0.4.3
    Checking rand_distr v0.6.0
    Checking enumflags2 v0.7.12
    Checking zcheapstr v1.1.0
    Checking rgb v0.8.53
    Checking no_std_io2 v0.9.4
    Checking jxl-oxide-common v1.0.0
    Checking jxl-coding v1.0.1
    Checking wgpu-hal v30.0.1
    Checking jxl-image v0.13.0
    Checking quick-xml v0.38.4
    Checking y4m v0.8.0
    Checking endi v1.1.1
    Checking photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-vector)
    Checking futures-sink v0.3.34
    Checking base64 v0.22.1
    Checking float-ord v0.3.2
    Checking untrusted v0.9.0
    Checking roxmltree v0.20.0
    Checking av-scenechange v0.14.1
    Checking zvariant v5.15.0
    Checking bitstream-io v4.10.0
    Checking tract-nnef v0.23.8
    Checking prost v0.14.4
    Checking icu_properties_data v2.3.0
    Checking event-listener-strategy v0.5.4
    Checking icu_locale_fallback_data v2.3.0
    Checking av1-grain v0.2.5
    Checking futures-lite v2.6.1
    Checking tract-hir v0.23.8
    Checking rfc6979 v0.4.0
    Checking spki v0.7.3
    Checking maybe-rayon v0.1.1
    Checking phf_shared v0.13.1
    Checking rand_chacha v0.3.1
    Checking lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-tiff)
    Checking itertools v0.14.0
    Checking errno v0.3.14
   Compiling enumn v0.1.14
   Compiling ahash v0.8.12
    Checking imgref v1.12.3
    Checking new_debug_unreachable v1.0.6
    Checking image v0.25.10
    Checking web-time v1.1.0
    Checking jxl-modular v0.11.3
    Checking jxl-color v0.11.0
    Checking async-task v4.7.1
    Checking loop9 v0.1.5
    Checking signal-hook-registry v1.4.8
    Checking pkcs5 v0.7.1
    Checking rand v0.8.8
    Checking async-io v2.6.0
    Checking phf v0.13.1
    Checking icu_properties v2.3.0
    Checking curve25519-dalek v4.1.3
    Checking async-channel v2.5.0
    Checking jxl-vardct v0.11.1
    Checking icu_locale_fallback v2.3.0
    Checking zbus_names v4.3.4
    Checking futures-channel v0.3.34
    Checking icu_normalizer_data v2.3.0
    Checking rustls-webpki v0.103.15
    Checking lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-meta)
    Checking tract-extra v0.23.8
    Checking tract-transformers v0.23.8
    Checking icu_segmenter_data v2.3.0
    Checking polyval v0.6.2
    Checking jxl-frame v0.13.3
    Checking pkcs8 v0.10.2
    Checking tract-onnx-opl v0.23.8
    Checking avif-serialize v0.8.9
    Checking wgpu-core-deps-windows-linux-android v30.0.1
    Checking lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-develop)
    Checking aead v0.5.2
    Checking wgpu-core v30.0.1
    Checking num-iter v0.1.46
    Checking font-types v0.11.3
    Checking sec1 v0.7.3
    Checking rav1e v0.8.1
    Checking color v0.3.3
    Checking aho-corasick v1.1.5
    Checking euclid v0.22.14
    Checking regex-syntax v0.8.11
    Checking elliptic-curve v0.13.8
    Checking parlance v0.1.0
    Checking atomic-waker v1.1.2
    Checking jpeg-encoder v0.7.1
    Checking futures-task v0.3.34
    Checking peniko v0.6.1
    Checking jxl-jbr v0.2.1
    Checking jxl-render v0.12.4
    Checking futures-util v0.3.34
    Checking piper v0.2.5
    Checking tract-onnx v0.23.8
    Checking read-fonts v0.39.2
    Checking fontique v0.11.1
    Checking ecdsa v0.16.9
    Checking primeorder v0.13.6
    Checking parley_data v0.11.1
    Checking guillotiere v0.7.0
    Checking icu_segmenter v2.3.0
    Checking num-bigint-dig v0.8.6
    Checking pkcs1 v0.7.5
    Checking ed25519 v2.2.3
    Checking accesskit v0.24.1
    Checking jxl-oxide v0.12.6
    Checking rustls v0.23.45
    Checking ghash v0.5.1
    Checking regex-automata v0.4.18
    Checking icu_normalizer v2.3.0
    Checking async-signal v0.2.14
    Checking async-lock v3.4.2
    Checking poly1305 v0.8.0
    Checking tiny-skia-path v0.11.4
    Checking wayland-protocols-wlr v0.3.12
    Checking chacha20 v0.9.1
    Checking ctr v0.9.2
    Checking aes v0.8.4
    Checking kurbo v0.11.3
    Checking jpeg-decoder v0.3.2
    Checking moxcms v0.9.1
    Checking ttf-parser v0.25.1
    Checking pollster v1.0.1
    Checking async-process v2.5.0
    Checking svgtypes v0.15.3
    Checking ravif v0.13.0
    Checking li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-seg)
    Checking chacha20poly1305 v0.10.1
    Checking lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-pipeline)
    Checking parley v0.11.1
    Checking wgpu v30.0.1
    Checking ed25519-dalek v2.2.0
    Checking rsa v0.9.10
    Checking vello_common v0.1.0
    Checking p384 v0.13.1
    Checking p256 v0.13.2
    Checking httparse v1.10.1
    Checking blocking v1.7.0
    Checking lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-codecs)
    Checking lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raw)
    Checking x25519-dalek v2.0.1
    Checking zbus_xml v5.2.1
    Checking aes-gcm v0.10.3
    Checking async-executor v1.14.0
    Checking async-broadcast v0.7.2
    Checking rustls-webpki v0.102.8
    Checking regex v1.13.1
    Checking lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-scenes)
    Checking webpki-roots v1.0.9
    Checking http v1.5.0
    Checking ordered-stream v0.2.0
    Checking photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-psd)
    Checking emath v0.36.2
    Checking simplecss v0.2.2
    Checking ab_glyph_rasterizer v0.1.10
    Checking data-url v0.3.2
    Checking pico-args v0.5.0
    Checking xcursor v0.3.11
    Checking xkeysym v0.2.1
    Checking xmlwriter v0.1.0
   Compiling dyn-stack-macros v0.1.3
    Checking linux-raw-sys v0.4.15
    Checking imagesize v0.13.0
    Checking cursor-icon v1.2.0
    Checking ecolor v0.36.2
    Checking wayland-cursor v0.31.14
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking zbus v5.19.0
    Checking rustls-rustcrypto v0.0.2-alpha
    Checking lensfun v0.7.0
    Checking thiserror v1.0.69
    Checking owned_ttf_parser v0.25.1
    Checking photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-text)
    Checking zbus-lockstep v0.5.2
    Checking vello_cpu v0.1.0
    Checking rustix v0.38.44
    Checking skrifa v0.42.1
    Checking dyn-stack v0.13.2
    Checking usvg v0.45.1
    Checking wayland-csd-frame v0.3.0
    Checking ab_glyph v0.2.32
    Checking unicode-general-category v1.1.0
    Checking lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-catalog)
    Checking lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-preview)
    Checking lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-merge)
    Checking photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-compose)
    Checking photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-algo)
    Checking linesweeper v0.4.0
    Checking gethostname v1.1.0
    Checking lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-gpu)
    Checking zune-inflate v0.2.54
    Checking hayro-ccitt v0.3.0
    Checking epaint_default_fonts v0.36.2
    Checking as-raw-xcb-connection v1.0.1
    Checking x11rb-protocol v0.13.2
    Checking lebe v0.5.3
    Checking bit_field v0.10.3
    Checking twox-hash v2.1.4
   Compiling seq-macro v0.3.6
    Checking unicode-segmentation v1.13.3
    Checking base64 v0.23.1
    Checking nohash-hasher v0.2.0
    Checking self_cell v1.3.0
    Checking ruzstd v0.9.0
    Checking hayro-jbig2 v0.3.0
    Checking write-fonts v0.48.1
    Checking photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-pathops)
    Checking ureq-proto v0.6.4
    Checking brotli v8.0.4
    Checking hayro-jpeg2000 v0.3.5
    Checking ron v0.12.2
    Checking calloop v0.13.0
    Checking wasmi_core v2.0.0
    Checking exr v1.74.2
    Checking gemm-common v0.19.0
   Compiling io-lifetimes v3.0.1
    Checking atspi-common v0.13.0
    Checking foldhash v0.1.5
    Checking ryu v1.0.23
    Checking jpeg-encoder v0.6.1
    Checking epaint v0.36.2
   Compiling io-lifetimes v2.0.4
    Checking constant_time_eq v0.4.2
    Checking hayro-postscript v0.1.0
   Compiling ident_case v1.0.1
   Compiling strsim v0.11.1
    Checking option-ext v0.2.0
    Checking utf8-zero v0.8.1
    Checking calloop-wayland-source v0.3.0
    Checking wasmi_ir v2.0.0
    Checking blake3 v1.8.7
   Compiling darling_core v0.24.1
    Checking hashbrown v0.15.5
    Checking dirs-sys v0.5.0
    Checking hayro-syntax v0.7.2
    Checking pdf-writer v0.15.0
    Checking hayro-cmap v0.1.0
    Checking wasmparser v0.228.0
    Checking tiny-skia-path v0.12.0
    Checking sha1 v0.10.7
    Checking photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-codecs)
    Checking egui v0.36.2
    Checking imagesize v0.14.0
    Checking wasmi_collections v2.0.0
    Checking subsetter v0.2.6
    Checking utf-8 v0.7.6
    Checking fixedbitset v0.5.7
    Checking chunked_transfer v1.5.0
    Checking bumpalo v3.20.3
    Checking ascii v1.1.0
    Checking xmp-writer v0.3.3
   Compiling ref-cast v1.0.27
   Compiling io-extras v0.19.0
    Checking httpdate v1.0.3
    Checking data-encoding v2.11.1
    Checking krilla v0.8.2
    Checking lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
    Checking photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-format)
    Checking petgraph v0.8.3
    Checking hayro-interpret v0.7.0
    Checking tungstenite v0.27.0
    Checking dirs v6.0.0
    Checking x11-dl v2.21.0
    Checking smithay-client-toolkit v0.19.2
    Checking atspi-proxies v0.13.0
    Checking xkbcommon-dl v0.4.2
    Checking accesskit_consumer v0.36.0
    Checking wayland-protocols-plasma v0.3.12
    Checking photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-paint)
    Checking photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raw)
    Checking calloop v0.14.4
    Checking mio v1.2.3
    Checking socket2 v0.6.5
    Checking x11rb v0.13.2
   Compiling tokio-macros v2.7.2
   Compiling serde_derive_internals v0.30.0
   Compiling ref-cast-impl v1.0.27
    Checking ureq v3.4.2
   Compiling cap-primitives v4.0.3
    Checking smol_str v0.2.2
    Checking dpi v0.1.2
    Checking bitflags v1.3.2
    Checking tiny_http v0.12.0
    Checking calloop-wayland-source v0.4.1
    Checking png v0.17.16
    Checking atspi v0.29.0
    Checking fs-set-times v0.20.3
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Checking photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-pdf)
    Checking accesskit_atspi_common v0.18.1
    Checking tree_magic_mini v3.2.2
    Checking photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-vecfx)
    Checking photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-svg)
    Checking photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-affinity)
    Checking futures-executor v0.3.34
    Checking pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-trace)
    Checking photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-io)
    Checking photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ops)
    Checking wayland-protocols-misc v0.3.12
    Checking wayland-protocols-experimental v20250721.0.1
    Checking rustix-linux-procfs v0.1.1
    Checking wasmi v2.0.0
    Checking chrono v0.4.45
    Checking os_pipe v1.2.3
    Checking maybe-owned v0.3.4
    Checking winit v0.30.13
   Compiling cap-std v4.0.3
   Compiling rmcp v3.5.0
    Checking ambient-authority v0.0.2
    Checking ipnet v2.12.2
    Checking smithay-client-toolkit v0.20.0
    Checking wl-clipboard-rs v0.9.4
    Checking futures v0.3.34
    Checking tiny-skia v0.11.4
    Checking accesskit_unix v0.21.1
    Checking gemm-f32 v0.19.0
    Checking typed-path v0.12.3
    Checking resvg v0.45.1
    Checking enum-map v2.7.3
    Checking zip v7.2.0
    Checking arboard v3.6.1
    Checking gemm-c64 v0.19.0
    Checking gemm-f64 v0.19.0
    Checking gemm-c32 v0.19.0
    Checking photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-plugins)
    Checking type-map v0.5.1
    Checking li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-attributions)
   Compiling winresource v0.1.31
    Checking egui_extras v0.36.2
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
   Compiling lightcraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-ui-egui)
    Checking float8 v0.6.1
    Checking safetensors v0.7.0
    Checking egui-wgpu v0.36.2
    Checking gemm-f16 v0.19.0
    Checking num_cpus v1.17.0
    Checking is-docker v0.2.0
    Checking accesskit_winit v0.32.2
    Checking gemm v0.19.0
    Checking tokio v1.53.1
    Checking pollster v0.4.0
    Checking is-wsl v0.4.0
    Checking rfd v0.17.2
    Checking photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-gpu)
    Checking toml_parser v1.1.3+spec-1.1.0
   Compiling local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/apps/local-image)
    Checking smithay-clipboard v0.7.3
    Checking toml_datetime v1.1.1+spec-1.1.0
    Checking serde_spanned v1.1.1
    Checking toml_writer v1.1.2+spec-1.1.0
    Checking sys-locale v0.3.2
    Checking open v5.4.4
    Checking photocraft-tablet v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-tablet)
    Checking lightcraft-fetch v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-fetch)
    Checking heic-rs v0.1.1
    Checking egui-winit v0.36.2
    Checking memory-stats v1.2.0
    Checking lightcraft-mcp v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-mcp)
    Checking lightcraft-sysmem v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-sysmem)
    Checking photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-testkit)
    Checking candle-core v0.9.2
    Checking eframe v0.36.2
   Compiling schemars_derive v1.2.2
    Checking photocraft-heif v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-heif)
    Checking tokio-util v0.7.19
   Compiling darling_macro v0.24.1
    Checking toml v1.1.6+spec-1.1.0
   Compiling darling v0.24.1
   Compiling rmcp-macros v3.5.0
    Checking candle-nn v0.9.2
    Checking xtask v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/xtask)
    Checking schemars v1.2.2
    Checking lightcraft-segment v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-segment)
    Checking photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-automation)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Checking local-image-cli v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/apps/local-image-cli)
    Finished `dev` profile [optimized + debuginfo] target(s) in 24m 19s
```

</details>

<details>
<summary>Initial strict Clippy, redundant conversion in new assertion — exit 101</summary>

```bash
cargo clippy --offline -p li-ai -p photocraft-engine -p photocraft-ui-egui -p lightcraft-engine -p lightcraft-develop --all-targets -- -D warnings
```

```text
    Blocking waiting for file lock on build directory
   Compiling syn v2.0.119
    Checking cfg-if v1.0.5
    Checking log v0.4.34
    Checking bitflags v2.13.2
    Checking once_cell v1.21.4
    Checking memchr v2.8.3
    Checking bytemuck v1.25.2
    Checking either v1.18.0
   Compiling rustix v1.1.5
    Checking serde_core v1.0.229
    Checking libc v0.2.189
    Checking libm v0.2.16
    Checking crossbeam-utils v0.8.23
    Checking linux-raw-sys v0.12.1
    Checking foldhash v0.2.0
    Checking simd-adler32 v0.3.10
    Checking thiserror v2.0.21
    Checking adler2 v2.0.1
   Compiling generic-array v0.14.7
    Checking equivalent v1.0.2
    Checking typenum v1.20.1
    Checking itoa v1.0.18
    Checking crossbeam-epoch v0.9.21
    Checking pin-project-lite v0.2.17
    Checking subtle v2.6.1
    Checking rand_core v0.10.1
    Checking crc32fast v1.5.2
   Compiling serde_json v1.0.151
    Checking miniz_oxide v0.9.1
    Checking zmij v1.0.23
    Checking miniz_oxide v0.8.9
    Checking allocator-api2 v0.2.21
    Checking const-oid v0.9.6
    Checking scopeguard v1.2.0
    Checking getrandom v0.4.3
    Checking libloading v0.8.9
    Checking crossbeam-deque v0.8.8
    Checking zune-core v0.5.3
    Checking fdeflate v0.3.7
    Checking tracing-core v0.1.36
    Checking lock_api v0.4.14
    Checking quick-error v2.0.1
    Checking num-traits v0.2.19
    Checking spin v0.9.9
    Checking hashbrown v0.16.1
    Checking anyhow v1.0.104
    Checking dlib v0.5.3
    Checking zune-jpeg v0.5.15
    Checking rayon-core v1.13.0
    Checking hashbrown v0.17.1
    Checking pxfm v0.1.30
    Checking flate2 v1.1.10
    Checking weezl v0.1.12
    Checking cpufeatures v0.2.17
    Checking memmap2 v0.9.11
    Checking itertools v0.15.0
    Checking byteorder-lite v0.1.0
    Checking rayon v1.12.0
    Checking color_quant v1.1.0
    Checking num-integer v0.1.47
    Checking num-complex v0.4.6
    Checking wayland-sys v0.31.11
    Checking image-webp v0.2.4
    Checking lazy_static v1.5.0
    Checking gif v0.14.2
    Checking nom v8.0.0
    Checking png v0.18.1
    Checking unicode-ident v1.0.26
    Checking stable_deref_trait v1.2.1
    Checking serde v1.0.229
    Checking qoi v0.4.1
    Checking rawpointer v0.2.1
    Checking maplit v1.0.2
    Checking downcast-rs v2.0.2
    Checking dyn-clone v1.0.20
    Checking inventory v0.3.25
    Checking strength_reduce v0.2.4
    Checking matrixmultiply v0.3.11
    Checking dyn-hash v1.0.0
    Checking typeid v1.0.3
    Checking primal-check v0.3.4
    Checking num-bigint v0.4.8
    Checking smallvec v1.16.2
    Checking arrayvec v0.7.8
    Checking string-interner v0.20.0
    Checking transpose v0.2.3
    Checking bit-vec v0.10.1
    Checking byteorder v1.5.0
    Checking erased-serde v0.4.10
    Checking photocraft-cms v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-cms)
    Checking ndarray v0.17.2
    Checking parking_lot_core v0.9.12
    Checking rustfft v6.4.1
    Checking anymap3 v1.1.0
    Checking cpufeatures v0.3.1
    Checking photocraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-geom)
    Checking bit-set v0.11.1
    Checking filetime v0.2.29
    Checking static_assertions v1.1.0
    Checking moxcms v0.8.1
    Checking parking_lot v0.12.5
    Checking bytes v1.12.1
    Checking same-file v1.0.6
    Checking indexmap v2.14.2
    Checking siphasher v1.0.4
    Checking getrandom v0.3.4
    Checking chacha20 v0.10.2
    Checking num-rational v0.4.2
    Checking walkdir v2.5.0
    Checking unicode-width v0.2.2
    Checking lightcraft-geom v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-geom)
    Checking raw-window-handle v0.6.2
    Checking rand v0.10.3
    Checking polycool v0.4.0
    Checking photocraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-color)
    Checking uuid v1.26.1
    Checking getrandom v0.2.17
    Checking xattr v1.6.1
    Checking float-ord v0.3.2
    Checking rustc-hash v1.1.0
    Checking base64ct v1.8.3
    Checking bit-vec v0.9.1
    Checking kurbo v0.13.1
    Checking nom-language v0.1.0
    Checking lightcraft-color v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-color)
    Checking rand_core v0.6.4
    Checking codespan-reporting v0.13.1
    Checking tar v0.4.46
    Checking photocraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raster)
    Checking naga-types v30.0.1
    Checking pem-rfc7468 v0.7.0
    Checking bit-set v0.10.0
    Checking spirv v0.4.0+sdk-1.4.341.0
    Checking rand_distr v0.6.0
    Checking ash v0.38.0+1.3.281
    Checking alloc-no-stdlib v2.0.4
    Checking presser v0.3.1
    Checking khronos-egl v6.0.0
    Checking ordered-float v5.5.0
    Checking wgpu-types v30.0.1
    Checking glow v0.17.0
    Checking renderdoc-sys v1.1.0
    Checking zerofrom v0.1.8
    Checking alloc-stdlib v0.2.4
    Checking lightcraft-raster v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raster)
    Checking regex-syntax v0.8.11
    Checking as-slice v0.2.1
    Checking hex v0.4.3
    Checking lightcraft-develop v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-develop)
    Checking brotli-decompressor v5.0.3
    Checking yoke v0.8.3
    Checking no_std_io2 v0.9.4
    Checking rgb v0.8.53
    Checking aligned v0.4.3
    Checking y4m v0.8.0
    Checking roxmltree v0.20.0
    Checking percent-encoding v2.3.2
    Checking web-time v1.1.0
    Checking bitstream-io v4.10.0
    Checking rand_core v0.9.5
    Checking maybe-rayon v0.1.1
    Checking lightcraft-tiff v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-tiff)
    Checking quick-xml v0.38.4
    Checking itertools v0.14.0
    Checking font-types v0.12.5
    Checking linebender_resource_handle v0.1.1
    Checking imgref v1.12.3
    Checking new_debug_unreachable v1.0.6
    Checking ff v0.13.1
    Checking avif-serialize v0.8.9
    Checking aho-corasick v1.1.5
    Checking base16ct v0.2.0
   Compiling semver v1.0.28
    Checking read-fonts v0.41.0
    Checking loop9 v0.1.5
    Checking jpeg-encoder v0.7.1
    Checking pollster v1.0.1
    Checking group v0.13.0
    Checking lightcraft-meta v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-meta)
    Checking zerovec v0.11.8
    Checking moxcms v0.9.1
    Checking jpeg-decoder v0.3.2
    Checking base64 v0.22.1
   Compiling rustc_version v0.4.1
    Checking ttf-parser v0.25.1
    Checking opaque-debug v0.3.1
    Checking lightcraft-scenes v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-scenes)
    Checking writeable v0.6.4
    Checking untrusted v0.9.0
    Checking regex-automata v0.4.18
   Compiling lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
    Checking ab_glyph_rasterizer v0.1.10
    Checking fearless_simd v0.4.1
    Checking lightcraft-preview v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-preview)
    Checking lightcraft-pipeline v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-pipeline)
    Checking photocraft-algo v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-algo)
    Checking photocraft-doc v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-doc)
   Compiling rustls v0.23.45
   Compiling num-bigint-dig v0.8.6
   Compiling curve25519-dalek v4.1.3
    Checking tinystr v0.8.4
    Checking lightcraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-raw)
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking lightcraft-catalog v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-catalog)
    Checking httparse v1.10.1
    Checking http v1.5.0
    Checking num-iter v0.1.46
    Checking rustc-hash v2.1.3
    Checking litemap v0.8.3
    Checking potential_utf v0.1.6
    Checking zerotrie v0.2.5
    Checking option-ext v0.2.0
    Checking icu_locale_core v2.3.0
    Checking base64 v0.23.1
    Checking dirs-sys v0.5.0
    Checking utf8-zero v0.8.1
    Checking data-encoding v2.11.1
    Checking ureq-proto v0.6.4
    Checking regex v1.13.1
    Checking slab v0.4.12
    Checking owned_ttf_parser v0.25.1
    Checking ascii v1.1.0
    Checking icu_provider v2.3.1
    Checking chunked_transfer v1.5.0
    Checking ab_glyph v0.2.32
    Checking utf-8 v0.7.6
    Checking lensfun v0.7.0
    Checking httpdate v1.0.3
    Checking utf8_iter v1.0.4
    Checking dirs v6.0.0
    Checking float-cmp v0.9.0
    Checking fastrand v2.5.0
    Checking polling v3.11.0
    Checking phf_shared v0.13.1
    Checking arrayref v0.3.9
    Checking icu_collections v2.3.0
    Checking icu_locale_fallback_data v2.3.0
    Checking tiny_http v0.12.0
    Checking strict-num v0.1.1
    Checking icu_properties_data v2.3.0
    Checking downcast-rs v1.2.1
    Checking scoped-tls v1.0.1
    Checking parking v2.2.1
    Checking icu_segmenter_data v2.3.0
    Checking icu_locale_fallback v2.3.0
    Checking icu_normalizer_data v2.3.0
    Checking font-types v0.11.3
    Checking futures-core v0.3.34
    Checking parlance v0.1.0
   Compiling photocraft-text v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-text)
    Checking wayland-backend v0.3.17
    Checking icu_properties v2.3.0
    Checking tiny-skia-path v0.11.4
    Checking kurbo v0.11.3
    Checking icu_normalizer v2.3.0
    Checking read-fonts v0.39.2
    Checking icu_segmenter v2.3.0
    Checking photocraft-psd v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-psd)
    Checking raw-cpuid v11.6.0
    Checking reborrow v0.5.5
    Checking pulp-wasm-simd-flag v0.1.1
    Checking wayland-client v0.31.15
    Checking linesweeper v0.4.0
    Checking proc-macro2 v1.0.107
    Checking simplecss v0.2.2
    Checking zune-inflate v0.2.54
    Checking parley_data v0.11.1
    Checking bumpalo v3.20.3
    Checking pico-args v0.5.0
    Checking futures-io v0.3.34
    Checking svgtypes v0.15.3
    Checking data-url v0.3.2
    Checking winnow v1.0.4
    Checking twox-hash v2.1.4
    Checking skrifa v0.44.0
    Checking harfrust v0.12.0
    Checking fontique v0.11.1
    Checking bit_field v0.10.3
    Checking imagesize v0.13.0
    Checking hayro-ccitt v0.3.0
    Checking xmlwriter v0.1.0
    Checking lebe v0.5.3
    Checking ruzstd v0.9.0
    Checking quote v1.0.47
    Checking pulp v0.22.3
    Checking event-listener v5.4.2
    Checking usvg v0.45.1
    Checking hayro-jbig2 v0.3.0
    Checking photocraft-vector v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-vector)
    Checking hayro-jpeg2000 v0.3.5
    Checking brotli v8.0.4
    Checking wasmi_core v2.0.0
    Checking gpu-allocator v0.28.0
    Checking concurrent-queue v2.5.0
    Checking hayro-postscript v0.1.0
    Checking constant_time_eq v0.4.2
    Checking ryu v1.0.23
    Checking jpeg-encoder v0.6.1
    Checking event-listener-strategy v0.5.4
    Checking syn v3.0.6
    Checking blake3 v1.8.7
    Checking hayro-syntax v0.7.2
    Checking pdf-writer v0.15.0
    Checking wasmi_ir v2.0.0
    Checking wasmparser v0.228.0
    Checking futures-lite v2.6.1
    Checking tiny-skia-path v0.12.0
    Checking photocraft-compose v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-compose)
    Checking photocraft-pathops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-pathops)
    Checking wasmi_collections v2.0.0
    Checking imagesize v0.14.0
    Checking parley v0.11.1
    Checking xmp-writer v0.3.3
    Checking wayland-protocols v0.32.13
    Checking photocraft-paint v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-paint)
    Checking photocraft-raw v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-raw)
    Checking zcheapstr v1.1.0
    Checking endi v1.1.1
    Checking pc-trace v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-trace)
    Checking photocraft-ops v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ops)
    Checking accesskit v0.24.1
    Checking hayro-cmap v0.1.0
    Checking errno v0.3.14
    Checking async-task v4.7.1
    Checking async-io v2.6.0
    Checking signal-hook-registry v1.4.8
    Checking async-channel v2.5.0
   Compiling ahash v0.8.12
    Checking atomic-waker v1.1.2
    Checking async-lock v3.4.2
    Checking euclid v0.22.14
    Checking color v0.3.3
    Checking async-executor v1.14.0
    Checking piper v0.2.5
    Checking photocraft-vecfx v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-vecfx)
    Checking async-broadcast v0.7.2
    Checking async-signal v0.2.14
    Checking blocking v1.7.0
    Checking ordered-stream v0.2.0
    Checking linux-raw-sys v0.4.15
    Checking cursor-icon v1.2.0
    Checking xcursor v0.3.11
    Checking xkeysym v0.2.1
    Checking peniko v0.6.1
    Checking emath v0.36.2
    Checking async-process v2.5.0
    Checking wayland-csd-frame v0.3.0
    Checking rustix v0.38.44
    Checking wayland-cursor v0.31.14
    Checking guillotiere v0.7.0
    Checking unicode-general-category v1.1.0
    Checking gethostname v1.1.0
    Checking nohash-hasher v0.2.0
    Checking epaint_default_fonts v0.36.2
    Checking unicode-segmentation v1.13.3
    Checking as-raw-xcb-connection v1.0.1
    Checking ecolor v0.36.2
    Checking vello_common v0.1.0
    Checking self_cell v1.3.0
    Checking x11rb-protocol v0.13.2
    Checking x11-dl v2.21.0
    Checking xkbcommon-dl v0.4.2
    Checking accesskit_consumer v0.36.0
    Checking smol_str v0.2.2
    Checking futures-task v0.3.34
    Checking dpi v0.1.2
    Checking tempfile v3.27.0
    Checking bitflags v1.3.2
    Checking futures-util v0.3.34
    Checking type-map v0.5.1
    Checking skrifa v0.42.1
    Checking write-fonts v0.48.1
    Checking png v0.17.16
   Compiling li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-attributions)
    Checking is-docker v0.2.0
    Checking zvariant_utils v4.2.0
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Checking is-wsl v0.4.0
    Checking accesskit_consumer v0.35.0
    Checking wasmi v2.0.0
    Checking toml_parser v1.1.3+spec-1.1.0
    Checking vello_cpu v0.1.0
    Checking getopts v0.2.24
    Checking colored v2.2.0
    Checking toml_datetime v1.1.1+spec-1.1.0
    Checking serde_spanned v1.1.1
    Checking pollster v0.4.0
    Checking kittest v0.4.0
    Checking tiny-skia v0.11.4
    Checking open v5.4.4
    Checking memory-stats v1.2.0
    Checking sys-locale v0.3.2
    Checking leb128fmt v0.1.0
    Checking toml v1.1.6+spec-1.1.0
    Checking wait-timeout v0.2.1
    Checking bit-vec v0.8.0
    Checking quick-error v1.2.3
    Checking fnv v1.0.7
    Checking wasm-encoder v0.261.0
    Checking rand_xorshift v0.4.0
    Checking unarray v0.1.4
    Checking rusty-fork v0.3.1
    Checking bit-set v0.8.0
    Checking wayland-protocols-wlr v0.3.12
    Checking wayland-protocols-plasma v0.3.12
    Checking wayland-protocols-misc v0.3.12
    Checking wayland-protocols-experimental v20250721.0.1
    Checking rfd v0.17.2
    Checking wast v261.0.0
    Checking resvg v0.45.1
   Compiling zerocopy-derive v0.8.59
   Compiling zeroize_derive v1.5.0
   Compiling tracing-attributes v0.1.31
   Compiling fax_derive v0.2.0
   Compiling derive-new v0.7.0
   Compiling profiling-procmacros v1.0.18
   Compiling prost-derive v0.14.4
   Compiling equator-macro v0.4.2
   Compiling arg_enum_proc_macro v0.3.4
   Compiling num-derive v0.4.2
   Compiling curve25519-dalek-derive v0.1.1
   Compiling phf_macros v0.13.1
   Compiling enumflags2_derive v0.7.12
    Checking subsetter v0.2.6
    Checking krilla v0.8.2
    Checking photocraft-plugins v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-plugins)
    Checking profiling v1.0.18
   Compiling async-recursion v1.1.1
   Compiling thiserror-impl v1.0.69
   Compiling enum-map-derive v0.17.0
    Checking wat v1.261.0
    Checking fax v0.2.6
    Checking equator v0.4.2
    Checking aligned-vec v0.6.4
   Compiling enumflags2 v0.7.12
    Checking zeroize v1.9.1
    Checking v_frame v0.3.9
    Checking der v0.7.10
    Checking rustls-pki-types v1.15.1
    Checking av-scenechange v0.14.1
    Checking av1-grain v0.2.5
   Compiling zvariant v5.15.0
    Checking crypto-common v0.1.7
    Checking block-buffer v0.10.4
    Checking inout v0.1.4
    Checking universal-hash v0.5.1
    Checking digest v0.10.7
    Checking crypto-bigint v0.5.5
    Checking cipher v0.4.4
    Checking polyval v0.6.2
    Checking rav1e v0.8.1
    Checking spki v0.7.3
    Checking sha2 v0.10.9
    Checking hmac v0.12.1
    Checking signature v2.2.0
    Checking pkcs5 v0.7.1
    Checking hkdf v0.12.4
    Checking rfc6979 v0.4.0
    Checking aead v0.5.2
    Checking rustls-webpki v0.103.15
    Checking ctr v0.9.2
    Checking aes v0.8.4
    Checking pkcs8 v0.10.2
    Checking chacha20 v0.9.1
    Checking ghash v0.5.1
    Checking poly1305 v0.8.0
    Checking sec1 v0.7.3
    Checking pkcs1 v0.7.5
    Checking ed25519 v2.2.3
    Checking x25519-dalek v2.0.1
    Checking aes-gcm v0.10.3
    Checking elliptic-curve v0.13.8
    Checking chacha20poly1305 v0.10.1
    Checking sha1 v0.10.7
    Checking ed25519-dalek v2.2.0
    Checking rustls-webpki v0.102.8
    Checking webpki-roots v1.0.9
    Checking phf v0.13.1
    Checking tracing v0.1.44
   Compiling zbus_names v4.3.4
    Checking ecdsa v0.16.9
    Checking primeorder v0.13.6
    Checking hayro-interpret v0.7.0
    Checking prost v0.14.4
    Checking jxl-bitstream v1.1.0
    Checking jxl-grid v0.6.2
    Checking jxl-threadpool v1.0.0
    Checking x11rb v0.13.2
    Checking p256 v0.13.2
    Checking p384 v0.13.1
   Compiling zbus_xml v5.2.1
    Checking jxl-oxide-common v1.0.0
    Checking jxl-coding v1.0.1
    Checking calloop v0.14.4
    Checking jxl-image v0.13.0
    Checking enum-map v2.7.3
    Checking jxl-modular v0.11.3
    Checking jxl-color v0.11.0
    Checking calloop-wayland-source v0.4.1
   Compiling zbus-lockstep v0.5.2
    Checking smithay-client-toolkit v0.20.0
    Checking ravif v0.13.0
   Compiling zbus_macros v5.19.0
    Checking jxl-vardct v0.11.1
    Checking ureq v3.4.2
    Checking jxl-frame v0.13.3
    Checking thiserror v1.0.69
    Checking calloop v0.13.0
    Checking zerocopy v0.8.59
    Checking smithay-clipboard v0.7.3
    Checking calloop-wayland-source v0.3.0
    Checking jxl-jbr v0.2.1
    Checking jxl-render v0.12.4
    Checking smithay-client-toolkit v0.19.2
   Compiling zbus-lockstep-macros v0.5.2
    Checking jxl-oxide v0.12.6
    Checking half v2.7.1
    Checking ppv-lite86 v0.2.21
    Checking epaint v0.36.2
    Checking winit v0.30.13
    Checking rand_chacha v0.9.0
    Checking rand_chacha v0.3.1
    Checking tiff v0.11.3
    Checking tract-data v0.23.8
    Checking naga v30.0.1
    Checking exr v1.74.2
    Checking zbus v5.19.0
    Checking rand v0.9.5
    Checking rand v0.8.8
    Checking image v0.25.10
    Checking tungstenite v0.27.0
    Checking proptest v1.11.0
    Checking egui v0.36.2
   Compiling tract-linalg v0.23.8
    Checking rsa v0.9.10
    Checking rustls-rustcrypto v0.0.2-alpha
    Checking lightcraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-codecs)
    Checking photocraft-codecs v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-codecs)
    Checking photocraft-pdf v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-pdf)
    Checking photocraft-svg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-svg)
    Checking photocraft-affinity v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-affinity)
    Checking arboard v3.6.1
    Checking dify v0.8.0
    Checking photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-testkit)
    Checking atspi-common v0.13.0
    Checking lightcraft-merge v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-merge)
    Checking photocraft-format v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-format)
    Checking egui_extras v0.36.2
    Checking atspi-proxies v0.13.0
    Checking accesskit_atspi_common v0.18.1
    Checking wgpu-naga-bridge v30.0.1
    Checking wgpu-hal v30.0.1
    Checking tract-core v0.23.8
    Checking atspi v0.29.0
    Checking accesskit_unix v0.21.1
    Checking wgpu-core-deps-windows-linux-android v30.0.1
    Checking wgpu-core v30.0.1
    Checking accesskit_winit v0.32.2
    Checking egui-winit v0.36.2
    Checking wgpu v30.0.1
    Checking photocraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-gpu)
    Checking egui-wgpu v0.36.2
    Checking tract-nnef v0.23.8
    Checking tract-hir v0.23.8
    Checking eframe v0.36.2
    Checking egui_kittest v0.36.2
    Checking tract-extra v0.23.8
    Checking tract-transformers v0.23.8
    Checking tract-onnx-opl v0.23.8
    Checking tract-onnx v0.23.8
    Checking li-seg v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-seg)
    Checking lightcraft-gpu v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-gpu)
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
error: useless conversion to the same type: `f64`
   --> crates/li-ai/src/refine/tests.rs:776:60
    |
776 |         assert!((effective["strength"].as_f64().unwrap() - f64::from(values.2)).abs() < 1e-6);
    |                                                            ^^^^^^^^^^^^^^^^^^^ help: consider removing `f64::from()`: `values.2`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.98.0/index.html#useless_conversion
    = note: `-D clippy::useless-conversion` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::useless_conversion)]`

error: could not compile `li-ai` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

</details>

<details>
<summary>Strict Clippy on all touched crates and targets — exit 0</summary>

```bash
cargo clippy --offline -p li-ai -p photocraft-engine -p photocraft-ui-egui -p lightcraft-engine -p lightcraft-develop --all-targets -- -D warnings
```

```text
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking photocraft-io v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-io)
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Checking lightcraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/lc-engine)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Finished `dev` profile [optimized + debuginfo] target(s) in 4m 00s
```

</details>

<details>
<summary>Changed assertion regression after Clippy correction — exit 0</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p li-ai --lib final_review_second_pass_metadata_uses_resolved_strength_and_explicit_options -- --test-threads=1
```

```text
    Blocking waiting for file lock on build directory
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Finished `test` profile [optimized + debuginfo] target(s) in 4m 11s
     Running unittests src/lib.rs (target/debug/deps/li_ai-6d31bc357dfba8f1)

running 1 test
test refine::tests::final_review_second_pass_metadata_uses_resolved_strength_and_explicit_options ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 157 filtered out; finished in 2.91s

```

</details>

<details>
<summary>Initial first-refresh fixture (unknown target) — exit 101</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p li-ai --lib final_review_checker_accepts_unchanged_inventory_refresh_after_submission -- --test-threads=1
```

```text
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Finished `test` profile [optimized + debuginfo] target(s) in 5.89s
     Running unittests src/lib.rs (target/debug/deps/li_ai-6d31bc357dfba8f1)

running 1 test
test refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission ... FAILED

failures:

---- refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission stdout ----

thread 'refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission' (787446) panicked at crates/li-ai/src/refine/tests.rs:795:78:
called `Option::unwrap()` on a `None` value
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 158 filtered out; finished in 0.10s

error: test failed, to rerun pass `-p li-ai --lib`
```

</details>

<details>
<summary>Before-fix first-refresh regression with declared target — exit 101</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p li-ai --lib final_review_checker_accepts_unchanged_inventory_refresh_after_submission -- --test-threads=1
```

```text
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Finished `test` profile [optimized + debuginfo] target(s) in 11.35s
     Running unittests src/lib.rs (target/debug/deps/li_ai-6d31bc357dfba8f1)

running 1 test
test refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission ... FAILED

failures:

---- refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission stdout ----

thread 'refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission' (806572) panicked at crates/li-ai/src/refine/tests.rs:805:50:
called `Result::unwrap()` on an `Err` value: Model inventory changed during refinement testing; retry.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 158 filtered out; finished in 1.32s

error: test failed, to rerun pass `-p li-ai --lib`
```

</details>

<details>
<summary>Refine and assigned graph regressions after inventory normalization — exit 0</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p li-ai --lib -- refine:: catalog::final_review ops::final_review --test-threads=1
```

```text
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Finished `test` profile [optimized + debuginfo] target(s) in 24.24s
     Running unittests src/lib.rs (target/debug/deps/li_ai-6d31bc357dfba8f1)

running 30 tests
test catalog::final_review_realvis_lightning_uses_author_sampling_profile ... ok
test ops::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels ... ok
test refine::tests::autodetect_bypassed_template_model_and_its_output_are_activated ... ok
test refine::tests::autodetect_cached_template_rebinds_only_its_main_model_after_rename ... ok
test refine::tests::autodetect_cached_templates_work_offline_and_prefer_edit_variants ... ok
test refine::tests::autodetect_custom_sampler_template_prunes_other_models_and_splits_sigmas ... ok
test refine::tests::autodetect_fallback_order_is_family_template_universal ... ok
test refine::tests::autodetect_hash_changes_when_file_content_changes_and_cancellation_is_not_cached ... ok
test refine::tests::autodetect_metric_rejects_destroyed_composition ... ok
test refine::tests::autodetect_mock_all_recipes_fail_is_unsupported_and_cancellation_is_transient ... ok
test refine::tests::autodetect_mock_busy_queue_defers_without_a_verdict_or_second_prompt ... ok
test refine::tests::autodetect_mock_family_failure_falls_back_to_universal ... ok
test refine::tests::autodetect_mock_unknown_checkpoint_verifies_once_persists_and_retests ... ok
test refine::tests::autodetect_mock_unknown_split_weights_require_a_template_without_universal_guessing ... ok
test refine::tests::autodetect_template_match_checks_exact_file_role_size_and_hash ... ok
test refine::tests::autodetect_template_preserves_wiring_and_marks_refine_controls ... ok
test refine::tests::autodetect_text_template_becomes_img2img_with_its_decoder_vae ... ok
test refine::tests::autodetect_universal_graph_uses_node_defaults_and_standard_metadata ... ok
test refine::tests::autodetect_verdict_roundtrip_defaults_and_retest_preserves_other_hashes ... ok
test refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission ... ok
test refine::tests::final_review_checker_rejects_inventory_change_after_submission ... ok
test refine::tests::final_review_checker_rejects_model_swap_after_submission ... ok
test refine::tests::final_review_checker_rejects_model_swap_before_cached_publication ... ok
test refine::tests::final_review_effective_metadata_resolves_saved_defaults_and_honours_explicit_values ... ok
test refine::tests::final_review_family_styles_are_preserved_or_rejected_explicitly ... ok
test refine::tests::final_review_hash_detects_same_size_rewrite_with_restored_mtime ... ok
test refine::tests::final_review_recipes_never_silently_drop_styles ... ok
test refine::tests::final_review_refine_keeps_explicit_values_equal_to_nominal_defaults ... ok
test refine::tests::final_review_second_pass_metadata_uses_resolved_strength_and_explicit_options ... ok
test refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes ... ok

test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 129 filtered out; finished in 21.09s

```

</details>

<details>
<summary>UI/i18n after normalization, before equal-number provenance fix — exit 101</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p photocraft-ui-egui --lib -- generate_ui:: i18n:: --test-threads=1
```

```text
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 1m 50s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-219d99becdc1f52f)

running 74 tests
test generate_ui::draft_refine::tests::autodetect_refine_picker_disables_failure_and_click_retest_verifies_on_worker_gpu ... ok
test generate_ui::draft_refine::tests::draft_refine_click_carries_current_composite_selected_layer_and_draft_pixels ... ok
test generate_ui::draft_refine::tests::draft_refine_dock_request_keeps_open_image_and_drops_hidden_create_references ... ok
test generate_ui::draft_refine::tests::draft_refine_fresh_and_changed_refinement_model_use_per_model_denoise ... ok
test generate_ui::draft_refine::tests::draft_refine_gpu_design_system_all_themes_and_resolutions ... ok
test generate_ui::draft_refine::tests::draft_refine_large_previews_and_window_open_at_both_resolutions ... ok
test generate_ui::draft_refine::tests::draft_refine_library_use_and_open_actions_are_one_undo_and_a_new_document ... ok
test generate_ui::draft_refine::tests::draft_refine_lora_picker_filters_family_and_carries_strength_in_request ... ok
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... ok
test generate_ui::draft_refine::tests::draft_refine_properties_sections_collapse_and_sampling_has_no_offline_ceiling ... ok
test generate_ui::draft_refine::tests::draft_refine_sampling_is_editable_and_bounded_only_by_comfyui ... ok
test generate_ui::draft_refine::tests::draft_refine_shared_aspect_presets_lock_and_model_grid ... ok
test generate_ui::draft_refine::tests::draft_refine_zoom_pan_comparison_and_saved_defaults ... ok
test generate_ui::draft_refine::tests::ds_finding_mock_request_guard_clears_during_unwind ... ok
test generate_ui::draft_refine::tests::zimage_turbo_variety_tip_follows_picker_clicks_in_generate_and_draft_gpu ... ok
test generate_ui::ds2_refine_lora_preset_roundtrip_and_invalid_step_limits ... ok
test generate_ui::ds2_restored_upscale_keeps_non_official_model_choice ... ok
test generate_ui::ds_finding_library_snapshots_share_entries_until_removal ... ok
test generate_ui::ds_finding_refine_preset_persists_custom_sampler_and_loads_old_defaults ... ok
test generate_ui::final_review_history_persists_effective_sampling_and_replays_it_explicitly ... ok
test generate_ui::final_review_history_replay_preserves_the_resolved_second_pass ... ok
test generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit ... FAILED
test generate_ui::final_review_unsupported_recipe_picker_explains_and_can_clear_styles ... ok
test generate_ui::tests::aspect_sizes_are_about_a_megapixel_on_the_grid ... ok
test generate_ui::tests::ds_finding_selected_layer_refine_is_disabled_without_a_selected_layer ... ok
test generate_ui::tests::enhance_generate_upscale_clicks_three_methods_and_scale ... ok
test generate_ui::tests::model_tags_do_not_call_models_loras ... ok
test generate_ui::tests::natural_order ... ok
test generate_ui::tests::refine_model_groups_exclude_upscalers_tools_and_video ... ok
test generate_ui::tests::refine_panel_mode_and_second_pass_use_model_defaults_and_grouped_picker ... ok
test generate_ui::tests::refine_with_lora_picker_and_precision_carry_the_second_pass_settings ... ok
test generate_ui::tests::steps_and_guidance_follow_the_backend_limits_only ... ok
test generate_ui::tests::styles_row_opens_the_lora_browser ... ok
test generate_ui::tests::upscale_offers_the_official_upscalers_first ... ok
test generate_ui::zimage_variety_settings_presets_and_requests_keep_old_defaults ... ok
test i18n::live_tests::a_control_request_changes_the_drawing_language_in_the_same_frame ... ok
test i18n::live_tests::an_explicit_language_survives_restart_on_a_different_system_language ... ok
test i18n::live_tests::cjk_font_order_follows_the_selected_language ... ok
test i18n::live_tests::drawing_languages_are_isolated_between_threads ... ok
test i18n::live_tests::dynamic_brush_sections_have_translations_and_draw_in_the_selected_language ... ok
test i18n::live_tests::first_launch_renders_the_system_language_without_overwriting_auto ... ok
test i18n::live_tests::french_and_korean_plural_messages_keep_placeholders ... ok
test i18n::live_tests::keyboard_shortcuts_list_commands_in_the_selected_language ... ok
test i18n::live_tests::korean_glyphs_remain_available_after_repeated_language_switches ... ok
test i18n::live_tests::missing_or_unsupported_system_languages_render_english ... ok
test i18n::live_tests::older_preferences_without_a_language_follow_the_system ... ok
test i18n::live_tests::preferences_preview_does_not_commit_until_applied_and_survives_reload ... ok
test i18n::live_tests::requested_languages_resolve_regional_preferences ... ok
test i18n::live_tests::scoped_preview_restores_language_even_after_a_panic ... ok
test i18n::system::tests::native_locale_results_are_bounded_and_trimmed ... ok
test i18n::system::tests::system_preferences_are_negotiated_in_order ... ok
test i18n::system::tests::test_system_preferences_restore_and_remain_thread_local ... ok
test i18n::tests::blend_mode_names_are_translated ... ok
test i18n::tests::brazilian_portuguese_resolves_and_pluralises ... ok
test i18n::tests::brush_section_names_are_translated ... ok
test i18n::tests::bundled_catalogs_are_consistent ... ok
test i18n::tests::camera_raw_labels_are_translated_in_every_available_language ... ok
test i18n::tests::candidates_walk_from_specific_to_general ... ok
test i18n::tests::catalog_kinds_are_parsed_and_looked_up ... ok
test i18n::tests::complete_languages_translate_every_menu_string ... ok
test i18n::tests::czech_plural_rule ... ok
test i18n::tests::every_tl_literal_is_translated ... ok
test i18n::tests::french_resolves_and_pluralises ... ok
test i18n::tests::indonesian_tags_resolve ... ok
test i18n::tests::korean_uses_photoshop_terminology ... ok
test i18n::tests::lookups_fall_back_to_english ... ok
test i18n::tests::malformed_lines_are_reported_not_fatal ... ok
test i18n::tests::mixer_brush_ui_strings_have_translations_in_every_registered_language ... ok
test i18n::tests::plurals_and_placeholders ... ok
test i18n::tests::preferences_resolve_with_fallback ... ok
test i18n::tests::russian_plural_rules ... ok
test i18n::tests::simplified_chinese_covers_dynamic_shortcuts_and_layer_counts ... ok
test i18n::tests::spanish_resolves_and_pluralises ... ok
test i18n::tests::tags_map_to_languages ... ok

failures:

---- generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit stdout ----

thread 'generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit' (898932) panicked at crates/pc-ui-egui/src/generate_ui.rs:3486:5:
assertion `left == right` failed
  left: (None, None)
 right: (Some(4), Some(1.0))


failures:
    generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit

test result: FAILED. 73 passed; 1 failed; 0 ignored; 0 measured; 1256 filtered out; finished in 29.92s

error: test failed, to rerun pass `-p photocraft-ui-egui --lib`
```

</details>

<details>
<summary>Workspace check after normalization — exit 0</summary>

```bash
cargo check --offline --workspace
```

```text
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
    Checking photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-automation)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Checking local-image-cli v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/apps/local-image-cli)
    Checking local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/apps/local-image)
    Finished `dev` profile [optimized + debuginfo] target(s) in 2m 00s
```

</details>

<details>
<summary>Strict Clippy after normalization — exit 0</summary>

```bash
cargo clippy --offline -p li-ai -p photocraft-engine -p photocraft-ui-egui -p lightcraft-engine -p lightcraft-develop --all-targets -- -D warnings
```

```text
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Finished `dev` profile [optimized + debuginfo] target(s) in 2m 35s
```

</details>

<details>
<summary>Final refine/graph suite, including upload signature — exit 0</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p li-ai --lib -- refine:: catalog::final_review ops::final_review --test-threads=1
```

```text
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Finished `test` profile [optimized + debuginfo] target(s) in 11.78s
     Running unittests src/lib.rs (target/debug/deps/li_ai-6d31bc357dfba8f1)

running 31 tests
test catalog::final_review_realvis_lightning_uses_author_sampling_profile ... ok
test ops::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels ... ok
test refine::tests::autodetect_bypassed_template_model_and_its_output_are_activated ... ok
test refine::tests::autodetect_cached_template_rebinds_only_its_main_model_after_rename ... ok
test refine::tests::autodetect_cached_templates_work_offline_and_prefer_edit_variants ... ok
test refine::tests::autodetect_custom_sampler_template_prunes_other_models_and_splits_sigmas ... ok
test refine::tests::autodetect_fallback_order_is_family_template_universal ... ok
test refine::tests::autodetect_hash_changes_when_file_content_changes_and_cancellation_is_not_cached ... ok
test refine::tests::autodetect_metric_rejects_destroyed_composition ... ok
test refine::tests::autodetect_mock_all_recipes_fail_is_unsupported_and_cancellation_is_transient ... ok
test refine::tests::autodetect_mock_busy_queue_defers_without_a_verdict_or_second_prompt ... ok
test refine::tests::autodetect_mock_family_failure_falls_back_to_universal ... ok
test refine::tests::autodetect_mock_unknown_checkpoint_verifies_once_persists_and_retests ... ok
test refine::tests::autodetect_mock_unknown_split_weights_require_a_template_without_universal_guessing ... ok
test refine::tests::autodetect_template_match_checks_exact_file_role_size_and_hash ... ok
test refine::tests::autodetect_template_preserves_wiring_and_marks_refine_controls ... ok
test refine::tests::autodetect_text_template_becomes_img2img_with_its_decoder_vae ... ok
test refine::tests::autodetect_universal_graph_uses_node_defaults_and_standard_metadata ... ok
test refine::tests::autodetect_verdict_roundtrip_defaults_and_retest_preserves_other_hashes ... ok
test refine::tests::final_review_checker_accepts_unchanged_inventory_refresh_after_submission ... ok
test refine::tests::final_review_checker_rejects_inventory_change_after_submission ... ok
test refine::tests::final_review_checker_rejects_model_swap_after_submission ... ok
test refine::tests::final_review_checker_rejects_model_swap_before_cached_publication ... ok
test refine::tests::final_review_effective_metadata_resolves_saved_defaults_and_honours_explicit_values ... ok
test refine::tests::final_review_family_styles_are_preserved_or_rejected_explicitly ... ok
test refine::tests::final_review_hash_detects_same_size_rewrite_with_restored_mtime ... ok
test refine::tests::final_review_inventory_identity_ignores_uploaded_images_and_retains_weights ... ok
test refine::tests::final_review_recipes_never_silently_drop_styles ... ok
test refine::tests::final_review_refine_keeps_explicit_values_equal_to_nominal_defaults ... ok
test refine::tests::final_review_second_pass_metadata_uses_resolved_strength_and_explicit_options ... ok
test refine::tests::final_review_service_rejects_unsupported_styles_for_both_refine_routes ... ok

test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 129 filtered out; finished in 20.44s

```

</details>

<details>
<summary>Focused UI run compiled before the equal-number provenance helper — exit 101</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p photocraft-ui-egui --lib -- draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened final_review_second_pass_typing_nominal_values_marks_them_explicit --test-threads=1
```

```text
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
   Compiling photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 1m 10s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-219d99becdc1f52f)

running 2 tests
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... ok
test generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit ... FAILED

failures:

---- generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit stdout ----

thread 'generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit' (993053) panicked at crates/pc-ui-egui/src/generate_ui.rs:3486:5:
assertion `left == right` failed
  left: (None, None)
 right: (Some(4), Some(1.0))
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 1328 filtered out; finished in 2.74s

error: test failed, to rerun pass `-p photocraft-ui-egui --lib`
```

</details>

<details>
<summary>Final workspace check — exit 0</summary>

```bash
cargo check --offline --workspace
```

```text
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
    Checking photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-automation)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Checking local-image-cli v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/apps/local-image-cli)
    Checking local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/apps/local-image)
    Finished `dev` profile [optimized + debuginfo] target(s) in 50.72s
```

</details>

<details>
<summary>Final strict Clippy on all touched targets — exit 0</summary>

```bash
cargo clippy --offline -p li-ai -p photocraft-engine -p photocraft-ui-egui -p lightcraft-engine -p lightcraft-develop --all-targets -- -D warnings
```

```text
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking li-ai v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/li-ai)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-engine)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Finished `dev` profile [optimized + debuginfo] target(s) in 1m 58s
```

</details>

<details>
<summary>Final generation UI and i18n suite, headless — exit 0</summary>

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/codex-bugfix-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/codex-bugfix-ai-test-profile/models cargo test --offline -p photocraft-ui-egui --lib -- generate_ui:: i18n:: --test-threads=1
```

```text
    Blocking waiting for file lock on build directory
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-ai/crates/pc-ui-egui)
    Finished `test` profile [optimized + debuginfo] target(s) in 2m 48s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-219d99becdc1f52f)

running 74 tests
test generate_ui::draft_refine::tests::autodetect_refine_picker_disables_failure_and_click_retest_verifies_on_worker_gpu ... ok
test generate_ui::draft_refine::tests::draft_refine_click_carries_current_composite_selected_layer_and_draft_pixels ... ok
test generate_ui::draft_refine::tests::draft_refine_dock_request_keeps_open_image_and_drops_hidden_create_references ... ok
test generate_ui::draft_refine::tests::draft_refine_fresh_and_changed_refinement_model_use_per_model_denoise ... ok
test generate_ui::draft_refine::tests::draft_refine_gpu_design_system_all_themes_and_resolutions ... ok
test generate_ui::draft_refine::tests::draft_refine_large_previews_and_window_open_at_both_resolutions ... ok
test generate_ui::draft_refine::tests::draft_refine_library_use_and_open_actions_are_one_undo_and_a_new_document ... ok
test generate_ui::draft_refine::tests::draft_refine_lora_picker_filters_family_and_carries_strength_in_request ... ok
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... ok
test generate_ui::draft_refine::tests::draft_refine_properties_sections_collapse_and_sampling_has_no_offline_ceiling ... ok
test generate_ui::draft_refine::tests::draft_refine_sampling_is_editable_and_bounded_only_by_comfyui ... ok
test generate_ui::draft_refine::tests::draft_refine_shared_aspect_presets_lock_and_model_grid ... ok
test generate_ui::draft_refine::tests::draft_refine_zoom_pan_comparison_and_saved_defaults ... ok
test generate_ui::draft_refine::tests::ds_finding_mock_request_guard_clears_during_unwind ... ok
test generate_ui::draft_refine::tests::zimage_turbo_variety_tip_follows_picker_clicks_in_generate_and_draft_gpu ... ok
test generate_ui::ds2_refine_lora_preset_roundtrip_and_invalid_step_limits ... ok
test generate_ui::ds2_restored_upscale_keeps_non_official_model_choice ... ok
test generate_ui::ds_finding_library_snapshots_share_entries_until_removal ... ok
test generate_ui::ds_finding_refine_preset_persists_custom_sampler_and_loads_old_defaults ... ok
test generate_ui::final_review_history_persists_effective_sampling_and_replays_it_explicitly ... ok
test generate_ui::final_review_history_replay_preserves_the_resolved_second_pass ... ok
test generate_ui::final_review_second_pass_typing_nominal_values_marks_them_explicit ... ok
test generate_ui::final_review_unsupported_recipe_picker_explains_and_can_clear_styles ... ok
test generate_ui::tests::aspect_sizes_are_about_a_megapixel_on_the_grid ... ok
test generate_ui::tests::ds_finding_selected_layer_refine_is_disabled_without_a_selected_layer ... ok
test generate_ui::tests::enhance_generate_upscale_clicks_three_methods_and_scale ... ok
test generate_ui::tests::model_tags_do_not_call_models_loras ... ok
test generate_ui::tests::natural_order ... ok
test generate_ui::tests::refine_model_groups_exclude_upscalers_tools_and_video ... ok
test generate_ui::tests::refine_panel_mode_and_second_pass_use_model_defaults_and_grouped_picker ... ok
test generate_ui::tests::refine_with_lora_picker_and_precision_carry_the_second_pass_settings ... ok
test generate_ui::tests::steps_and_guidance_follow_the_backend_limits_only ... ok
test generate_ui::tests::styles_row_opens_the_lora_browser ... ok
test generate_ui::tests::upscale_offers_the_official_upscalers_first ... ok
test generate_ui::zimage_variety_settings_presets_and_requests_keep_old_defaults ... ok
test i18n::live_tests::a_control_request_changes_the_drawing_language_in_the_same_frame ... ok
test i18n::live_tests::an_explicit_language_survives_restart_on_a_different_system_language ... ok
test i18n::live_tests::cjk_font_order_follows_the_selected_language ... ok
test i18n::live_tests::drawing_languages_are_isolated_between_threads ... ok
test i18n::live_tests::dynamic_brush_sections_have_translations_and_draw_in_the_selected_language ... ok
test i18n::live_tests::first_launch_renders_the_system_language_without_overwriting_auto ... ok
test i18n::live_tests::french_and_korean_plural_messages_keep_placeholders ... ok
test i18n::live_tests::keyboard_shortcuts_list_commands_in_the_selected_language ... ok
test i18n::live_tests::korean_glyphs_remain_available_after_repeated_language_switches ... ok
test i18n::live_tests::missing_or_unsupported_system_languages_render_english ... ok
test i18n::live_tests::older_preferences_without_a_language_follow_the_system ... ok
test i18n::live_tests::preferences_preview_does_not_commit_until_applied_and_survives_reload ... ok
test i18n::live_tests::requested_languages_resolve_regional_preferences ... ok
test i18n::live_tests::scoped_preview_restores_language_even_after_a_panic ... ok
test i18n::system::tests::native_locale_results_are_bounded_and_trimmed ... ok
test i18n::system::tests::system_preferences_are_negotiated_in_order ... ok
test i18n::system::tests::test_system_preferences_restore_and_remain_thread_local ... ok
test i18n::tests::blend_mode_names_are_translated ... ok
test i18n::tests::brazilian_portuguese_resolves_and_pluralises ... ok
test i18n::tests::brush_section_names_are_translated ... ok
test i18n::tests::bundled_catalogs_are_consistent ... ok
test i18n::tests::camera_raw_labels_are_translated_in_every_available_language ... ok
test i18n::tests::candidates_walk_from_specific_to_general ... ok
test i18n::tests::catalog_kinds_are_parsed_and_looked_up ... ok
test i18n::tests::complete_languages_translate_every_menu_string ... ok
test i18n::tests::czech_plural_rule ... ok
test i18n::tests::every_tl_literal_is_translated ... ok
test i18n::tests::french_resolves_and_pluralises ... ok
test i18n::tests::indonesian_tags_resolve ... ok
test i18n::tests::korean_uses_photoshop_terminology ... ok
test i18n::tests::lookups_fall_back_to_english ... ok
test i18n::tests::malformed_lines_are_reported_not_fatal ... ok
test i18n::tests::mixer_brush_ui_strings_have_translations_in_every_registered_language ... ok
test i18n::tests::plurals_and_placeholders ... ok
test i18n::tests::preferences_resolve_with_fallback ... ok
test i18n::tests::russian_plural_rules ... ok
test i18n::tests::simplified_chinese_covers_dynamic_shortcuts_and_layer_counts ... ok
test i18n::tests::spanish_resolves_and_pluralises ... ok
test i18n::tests::tags_map_to_languages ... ok

test result: ok. 74 passed; 0 failed; 0 ignored; 0 measured; 1256 filtered out; finished in 26.81s

```

</details>

<details>
<summary>Format check — exit 0</summary>

```bash
cargo fmt --all -- --check
```

```text
(no output)
```

</details>

<details>
<summary>Whitespace check — exit 0</summary>

```bash
git diff --check
```

```text
(no output)
```

</details>

