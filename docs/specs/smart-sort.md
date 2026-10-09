# Smart Sort & Export: build spec (Codex task brief)

Status: research done 2026-10-09 against `claude/sleepy-franklin-egimjb` @ `919bfa2`.
Audience: an autonomous coding agent (no GPU, **no network**, cannot ask questions) plus the
coordinator who validates with real model weights afterwards.

---

## 0. What we are building (one paragraph)

A photographer imports a whole event gallery (≈2,000 raw/JPEG photos). In the Library they open
**File ▸ Smart Sort & Export…**, pick an event preset (Conference / Wedding / Sports / Custom), and
the app sorts every photo into categories ("Speakers", "Crowd reactions", "Candids",
"Sponsors"…), using a local image–text AI model and, if the photographer opts in, face
recognition ("one folder per speaker"). They review and correct the result in the same dialog
(drag a photo to another folder, name a face cluster), then export: the app creates one folder
per category under a destination root (default: the Desktop) and exports each photo into its
folder(s) with an existing export preset (JPEG render or copy originals). The categories are also
written as normal catalog keywords so they keep working for search, smart albums and later
exports. Everything runs on the CPU, offline after a one-time model download; no photo, face or
embedding ever leaves the machine.

---

## 1. Hard rules for the implementer

1. **Pure Rust, no new crates.** The workspace denies `unsafe` (`Cargo.toml` `[workspace.lints.rust] unsafe_code = "deny"`).
   Do not add any crate that is not already in `Cargo.lock`. Inference uses **`tract-onnx` 0.23.8,
   already used by `crates/li-seg`**. Do not use `ort`/onnxruntime, `tokenizers`, `candle-transformers`
   or anything with a C build step. Adding an *existing* workspace crate as a dependency of another
   workspace crate is fine (it does not add packages to `Cargo.lock`); verify with
   `git diff Cargo.lock` that **no `[[package]]` entries were added**.
2. **No network in tests.** Tests must never download or require real model weights. Use the
   mock models defined below. Real-weight checks are `#[ignore]` tests the coordinator runs.
3. **Models are downloaded at runtime by the app**, only when the user clicks Download, through the
   existing allow-listed downloader (`li_ai::download::download_file`, hosts `huggingface.co` and
   the `.hf.co` CDNs are already allowed: `crates/li-ai/src/download.rs:16-33`), verified by size +
   SHA-256 (values in §4).
4. **Privacy:** face detection/recognition is **off until the user opts in** (per library). Face
   embeddings live only in the library folder, never in XMP, never in exported files, never in logs.
   A "Clear face data" action deletes them. Person names are written to exported files only if the
   user ticks an option (default off).
5. Toolchain: `cargo +1.98.1 …` (the system cargo is 1.98.1; `~/.cargo/bin/cargo +1.98.1 fmt` /
   `clippy` exist on the coordinator machine). Keep `cargo +1.98.1 fmt --all` clean. Use
   `CARGO_BUILD_JOBS=4`.
6. Follow the code style of the files you touch: module doc comments that explain *why*, small
   functions, `serde(default)` on every new persisted field, no `unwrap()` in non-test code of crates
   that deny it (lc-segment style), errors as `String`/`anyhow` like the neighbouring code.

---

## 2. Research findings: what the repository already has (evidence)

Crate directory → package name: `crates/lc-catalog` = `lightcraft-catalog`, `lc-engine` =
`lightcraft-engine`, `lc-ui-egui` = `lightcraft-ui-egui`, `lc-segment` = `lightcraft-segment`,
`lc-meta` = `lightcraft-meta`; `li-*` and `pc-*` keep their names (`pc-*` = `photocraft-*`).

### 2.1 ML runtimes already in the tree
| Runtime | Where | Notes |
|---|---|---|
| `tract-onnx 0.23.8` (pure Rust ONNX) | `crates/li-seg/Cargo.toml:12`, `crates/li-seg/src/lib.rs` | Runs U²-Net, IS-Net, PP-MobileSeg, **Depth Anything V2 (a ViT)** on the CPU. Loading pattern: `tract_onnx::onnx().model_for_path(p)?.with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1,3,s,s)))?.into_optimized()?.into_runnable()?` (`li-seg/src/lib.rs:237-250`). Output read with `to_plain_array_view::<f32>()`. |
| `candle-core/-nn 0.9.2` (CPU; Metal on macOS) | `crates/lc-segment` (SAM 3) | Hand-ported SAM 3 incl. a **CLIP BPE tokenizer** (`crates/lc-segment/src/tokenizer.rs`, `encode` at :63, `CONTEXT = 32` at :13) and a CLIP text encoder. Feature `sam` of lc-engine. |

`ort`/onnxruntime is **not** present → not allowed.

### 2.2 Feasibility measured (coordinator machine, Ryzen 7 9800X3D, tract 0.23.8, release, 1 thread per model run)
A scratch program loaded the real candidate weights with the repo's exact tract version:

| Model file | Loads in tract? | Load time | Run time per call | Sanity |
|---|---|---|---|---|
| LAION CLIP ViT-B-32 `visual/model.onnx` (Immich export) | yes, input `image` [1,3,224,224] f32 | 0.18 s | **43–52 ms** / image | tetons→"mountain landscape", Earthrise→"earth from space", Migrant Mother→"mother with children" all ranked first |
| LAION CLIP ViT-B-32 `textual/model.onnx` | yes, input `text` [1,77] **i32** | 0.18 s | ≈30 ms / prompt | 512-d output |
| OpenAI CLIP ViT-B/16 (Xenova export) vision / text | yes (text input `input_ids` [1,77] **i64**) | 0.23 s | 160 ms / image | same correct ranking |
| OpenAI CLIP B/16 `vision_model_quantized.onnx` | **no** (tract fails on the dynamic-quantization graph) | – | – | use fp32 files only |
| SigLIP base-patch16-224 vision / text (Xenova) | yes | 0.26 s | 150 ms / image, 50 ms / prompt | outputs `[1,768]` pooled |
| YuNet `face_detection_yunet_2023mar.onnx` | yes | 0.02 s | 82 ms at 640², 330 ms at 1280² | found the face(s) in Migrant Mother with score 0.92; needs `.with_ignore_output_shapes(true).with_ignore_value_info(true)` for any input size other than 640×640 |
| SFace `face_recognition_sface_2021dec.onnx` | yes, input `data` [1,3,112,112] | 0.05 s | ≈50 ms / face | 128-d output |

Throughput estimate for 2,000 photos with 8 worker threads: tagging ≈ 15–40 s (3-crop CLIP, see
§5.2) plus image decode; faces ≈ 1–2 min. Embedded raw previews keep decode cheap (§2.6).

### 2.3 Catalog: keywords, smart albums, people, analysis
- Keywords are plain strings on `Meta.keywords` (`crates/lc-catalog/src/model.rs:184`), hierarchical with `|`
  (`crates/lc-catalog/src/keywords.rs:1-35`: `SEP`, `clean`, `is_under`, keyword tree, rename/merge as one `Op::Batch`).
- Edits are `Op`s in an op log (`crates/lc-catalog/src/lib.rs:71` `enum Op`; `SetMeta` :96, `SetAnalysis` :168, `Batch`),
  committed with `Session::commit(label, op)` (`crates/lc-engine/src/lib.rs:365`) = one undo step.
- Smart albums: `Album.smart: Option<Box<Filter>>` (`model.rs:407`); rules engine `RuleSet { match: all|any|none, rules: [Field{field,op,value} | Group] }`
  (`crates/lc-catalog/src/rules.rs:1-60`), fields table `FIELDS` (`rules.rs:56-83`, includes `keywords`, `rating`, `flag`, `sharpness`, `bestOfGroup`).
  There is a rules editor UI (`crates/lc-ui-egui/src/panels/rules_editor.rs`).
- People: **no face detection or recognition exists.** Faces come only from XMP MWG regions written by other
  apps: `Meta.regions: Vec<lightcraft_meta::Region>` (`model.rs:186-190`; `Region` at `crates/lc-meta/src/lib.rs:57-65`,
  `RegionKind::Face`). `Catalog::people()/people_in()` aggregate named face regions (`crates/lc-catalog/src/query.rs:373-420`),
  `Filter.person` filters by name (`query.rs:42`, `:150-153`), and the Library has a **People view**
  (`crates/lc-ui-egui/src/panels/people.rs`, cards per named person, click → filtered grid).
- Per-photo analysis precedent: Assisted Culling stores `Photo.analysis: Option<Analysis>` (`model.rs:294`) via `Op::SetAnalysis`
  (`crates/lc-engine/src/cmd/cull.rs:22-84`); Find Similar keeps 64-float signatures in a session cache keyed by
  `media::content_key(p)` (`cull.rs:86-117`, `crates/lc-engine/src/lib.rs:186`, `crates/lc-engine/src/media.rs:733`).
  Both analyse **thumbnail-level sources** (`SourceLevel::Thumb` = 512 px, `media.rs:82-96`).

### 2.4 Export pipeline
- Options: `ExportOptions` (`crates/lc-engine/src/export.rs:503-560`): format (JPEG/PNG/TIFF/WebP/AVIF/DNG/**Original** = copy the original + XMP sidecar),
  quality, resize, naming template, `subfolder`, `same_folder`, conflict policy, metadata policy, watermark, colour space.
- Presets: `builtin_presets()` (`export.rs:157-165`: "JPEG (Small)", "JPEG (Large)", "Original + Settings", "DNG") + user presets
  (`crates/lc-engine/src/cmd/export.rs`, `export.presets`/`export.savePreset`, `Session::export_params` expands `{preset}`).
- Batch: `prepare_export`/`prepare_batch` on the UI thread (`export.rs:1121-1135`) → `run_batch(items, opts, dest, write, exists, stop_on_error, progress)`
  on a worker (`export.rs:1322+`). `run_batch` resolves each item's folder (`batch_dir` or the photo's own folder) and keeps a
  `taken` set of **full paths** for name conflicts.
- Background export with progress + Cancel: `crates/lc-ui-egui/src/export_task.rs` (`ExportTask`, `start(app, items, opts, to, after)`).
- Export dialog (Lightroom layout): `crates/lc-ui-egui/src/panels/export_dialog.rs` (`SECTIONS`, `NAMING_TEMPLATES`, `export_dialog_params`).

### 2.5 Model manager, downloads, attributions
- On-device model list: `li_seg::MODELS` (`crates/li-seg/src/lib.rs:109`), `ModelSpec {id,label,file,bytes,sha256,url,size,isnet,licence,task,group,about}` (:18-36),
  `Group::{Subject,Sky,Depth}` with `label/about/recommended/models/in_use` (:39-94), files in `<models_dir>/segmentation/<file>` (`model_path`, :424),
  `installed_bytes` (:459), `remove`/`remove_with`.
- UI: Compositing ▸ Settings ▸ **Local AI ▸ On-device models** (`crates/pc-ui-egui/src/ai_ui.rs:314-393` `seg_rows`/`seg_row`, Download / Cancel / progress /
  "Installed"/"In use" / Remove-to-Trash via `li_ai::trash`), download thread `start_seg_download` (`ai_ui.rs:264-292`) calling
  `li_ai::download::download_file(url, dest, bytes, sha256, &JobControl, on_bytes)`. A test asserts every `li_seg::MODELS` URL is on the allow-list (`ai_ui.rs:1415`).
- The Library app shares that folder: `session.quick_seg_dir = Some(photocraft_engine::seg::models_dir())` (`apps/local-image/src/library_host.rs:787`),
  and currently tells users to set up AI "in Compositing › Local AI" (`crates/lc-ui-egui/src/panels/enhance.rs:78`). lc-ui-egui does **not** depend on li-ai.
- Attributions: `cargo xtask attributions` parses `li_seg::MODELS` from source (`xtask/src/attributions.rs:338-396`, `used_for` text chosen by `task:` at :356-362)
  and writes `assets/attributions.json`; test `every_li_seg_model_is_attributed` (`crates/pc-ui-egui/src/attributions.rs:133-141`) fails until it is regenerated.
  Ported code gets a row in `docs/PORTS.md` (one row per upstream file, full commit hash, SPDX licence, date).

### 2.6 Fast inputs for raws
- `PreviewLoader` = the camera's embedded JPEG, oriented, downsized (`media.rs:114-116`, installed by the app); `embedded_of(p)` returns it for **unedited raws**
  (`media.rs:948-954`). Grid thumbnails are cached on disk (`Library::thumbs_dir`, `crates/lc-engine/src/library.rs:119`).
- Rendering a developed preview: `Session::preview_job(id, max_w, max_h, apply_crop, &settings)` → `job.run().rendered` → `Rgba8` (pattern in
  `crates/lc-engine/src/quickseg.rs:41-50`). Jobs are created on the session thread (cheap) and are `Send` (export runs them on a worker).

### 2.7 UI, menus, tests, i18n
- Menu table `crates/lc-ui-egui/src/menus.rs` (e.g. `("dialog.export", "Export…", None, "File")` :136, `("dialog.cull", "Assisted Culling…", None, "Photo")` :165).
  Top menus are File / Edit / View / Photo / Window / Help (no "Library" menu).
- Dialogs: `enum Dialog` (`crates/lc-ui-egui/src/state.rs:457`), drawn in `panels/dialogs.rs`; large dialogs get their own file (export_dialog.rs).
- Widgets register automation ids with `crate::widgets::register(ctx, id, rect)`; headless tests drive them with
  `h.request("ui.clickWidget", json!({"id": id}), T)` (`crates/lc-ui-egui/src/tests_panels.rs:15-23, 150-160`; harness `crates/lc-ui-egui/src/headless.rs`).
  Demo session: `lightcraft_engine::Session::with_demo()`; engine tests use `fn demo() -> Session` (`crates/lc-engine/src/tests.rs:5`).
- **i18n (correction to the brief):** Library/Develop (`lc-ui-egui`) uses **JSON catalogs** `crates/lc-ui-egui/locales/{zh-hans,zh-hant,ja,pt-br}.json`
  (+ `*-formats.json` for `tr_format!`), looked up with `crate::i18n::tr("English")` (`crates/lc-ui-egui/src/i18n.rs:118-124, 160`). Missing keys fall back to English and
  are only *reported* (test `catalogs_agree_on_placeholders_and_report_gaps`, `i18n.rs:301`). The TSV catalogs with 12 languages
  (`crates/pc-ui-egui/src/i18n/*.tsv`, columns `context<TAB>source<TAB>translation`) belong to **Compositing**; there the test
  `every_tl_literal_is_translated` (`crates/pc-ui-egui/src/i18n/mod.rs:601`) **fails** if a new `tl!("…")` literal lacks a row in a language that claims completeness.

---

## 3. Recommendation

**Zero-shot image–text embeddings (CLIP) + user-editable category prompts + few-shot correction
by example**, with faces as an opt-in second signal. Not a fixed tagger.

Why:
- Event categories are photographer-specific ("Sponsors", "Award handover", "VIP table"). A fixed
  tagger (RAM++, 4,585 fixed tags, ~3 GB Swin-L + BERT) cannot express them, and is heavy. CLIP
  prompts can, and the user edits them in plain English.
- Images are embedded **once** (cached per photo); changing prompts, thresholds or folders
  re-sorts **instantly** (only text is re-encoded: ~30 ms per prompt). This makes the review step
  interactive.
- **Learning from corrections:** every photo the user drags into a folder becomes an exemplar; the
  folder's prototype blends its text embedding with the mean of its exemplars' image embeddings
  (a light Tip-Adapter / prototype classifier). 5–10 exemplars per folder fix most systematic
  zero-shot errors (e.g. a sponsor's branded backdrop being read as "speaker").
- Faces add what CLIP cannot: *who* (one folder per speaker) and counts (crowd vs portrait).

Expected quality (estimates; **must be validated by the coordinator on a real event gallery**, §9):
upstream ImageNet zero-shot top-1 is 66.6 % (LAION ViT-B-32 laion2B-s34B-b79K), 68.3 % (OpenAI B/16),
76.2 % (SigLIP B/16-224). For 5–8 coarse, well-described event categories expect roughly 75–90 %
correct on clear-cut shots, lower on overlapping categories (candid vs crowd reaction); the
"Unsorted" bucket plus review catches the rest; exemplars typically add 5–15 points. The product
must therefore **always show a review step before export**.

### Model choice
| Role | Model | Why |
|---|---|---|
| Tagging (default, Phase 1) | **LAION OpenCLIP ViT-B-32 laion2B-s34B-b79K**, ONNX export by Immich | MIT weights; 3× faster than B/16; uses the CLIP BPE tokenizer the repo already has; proven in tract here. |
| Tagging (Phase 4 option) | **Google SigLIP base-patch16-224** (Xenova ONNX) | Apache-2.0 and no "deployment out of scope" model-card language; better accuracy and calibrated sigmoid scores; but 3× slower and needs a new SentencePiece-Unigram tokenizer. |
| Face detection (Phase 3) | **YuNet 2023mar** (OpenCV Zoo) | MIT, 233 KB, 5 landmarks for alignment, works in tract at any size. |
| Face embedding (Phase 3) | **SFace 2021dec** (OpenCV Zoo) | Apache-2.0, 39 MB, 128-d, LFW ≈ 99.4 % per OpenCV Zoo eval. |

---

## 4. Model table (pinned; the implementer copies these values verbatim)

All URLs are on hosts already allowed by `li-ai` (`huggingface.co`, redirects to `*.hf.co`).
Pin by commit, never `main`. SHA-256 = the LFS `oid` reported by the HF API **and re-computed locally
on 2026-10-09** for every file marked ✔.

### 4.1 Chosen
| id (new) | Task | File (save as) | URL | Bytes | SHA-256 | Licence | Verdict |
|---|---|---|---|---|---|---|---|
| `clip-b32-laion` (visual) | image embedding 512-d | `clip-vit-b32-laion2b-visual.onnx` | `https://huggingface.co/immich-app/ViT-B-32__laion2b-s34b-b79k/resolve/19a057a0fb927cf239541300e74237cd56c7ffe2/visual/model.onnx` | 351613724 | `b9ce24b91a8c62ef8d40ea786051709c93140104cbf2b6b4a2cc270df3838f9c` ✔ | MIT (weights: [laion/CLIP-ViT-B-32-laion2B-s34B-b79K](https://huggingface.co/laion/CLIP-ViT-B-32-laion2B-s34B-b79K), card `license: mit`; ONNX conversion by Immich) | **Use** (default). Note: model card says deployed use is "out of scope" (guidance, not a licence term) → owner question Q1. |
| `clip-b32-laion` (text) | text embedding 512-d | `clip-vit-b32-laion2b-textual.onnx` | `…/19a057a0fb927cf239541300e74237cd56c7ffe2/textual/model.onnx` (same repo) | 254193396 | `48c25be37b352398f2533c9426dab6f9340535e14e46f884cc894236a5060722` ✔ | MIT | **Use** |
| `clip-b32-laion` (tokenizer) | BPE vocab | `clip-vocab.json` | `…/19a057a0fb927cf239541300e74237cd56c7ffe2/textual/vocab.json` | 862328 | `5047b556ce86ccaf6aa22b3ffccfc52d391ea4accdab9c2f2407da5b742d4363` ✔ | MIT (OpenAI CLIP vocabulary) | **Use** |
| `clip-b32-laion` (tokenizer) | BPE merges | `clip-merges.txt` | `…/19a057a0fb927cf239541300e74237cd56c7ffe2/textual/merges.txt` | 524619 | `9fd691f7c8039210e0fced15865466c65820d09b63988b0174bfe25de299051a` ✔ | MIT | **Use** |
| `yunet` | face detection + 5 landmarks | `face_detection_yunet_2023mar.onnx` | `https://huggingface.co/opencv/face_detection_yunet/resolve/3cc26e7f1014a5ee5d74a42acee58bafc9d0a310/face_detection_yunet_2023mar.onnx` | 232589 | `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4` ✔ | MIT ([LICENSE](https://huggingface.co/opencv/face_detection_yunet/blob/main/LICENSE), Shiqi Yu) | **Use** (Phase 3). Trained on WIDER FACE (research dataset): flagged in Q2. |
| `sface` | face embedding 128-d | `face_recognition_sface_2021dec.onnx` | `https://huggingface.co/opencv/face_recognition_sface/resolve/3d7082438a6e4551e840c9b2bb60b71e8da4b524/face_recognition_sface_2021dec.onnx` | 38696353 | `0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79` ✔ | Apache-2.0 ([LICENSE](https://huggingface.co/opencv/face_recognition_sface/blob/main/LICENSE)) | **Use** (Phase 3). Training-data provenance (public face datasets) flagged in Q2. |

Total download: tagging 607 MB; faces 39 MB.

### 4.2 Alternatives evaluated
| Model | Licence | Verdict |
|---|---|---|
| SigLIP base-patch16-224, Xenova ONNX `onnx/vision_model.onnx` (371819850 B, `f89d41bac7f4d4b87e010a467d93f98689d708916ed22f5a07f96fdfa26f475f` ✔), `onnx/text_model.onnx` (441332132 B, `3aa7fdbd20eaa8740cce17bf82913de641fcb632a768fed59f661cdcd0c32553` ✔), `tokenizer.json` (2398744 B, `4a17c975210be5ab4c36b47d8dae4eefb866dbfb1e676e394aad85dc30a3ae08` ✔) at `https://huggingface.co/Xenova/siglip-base-patch16-224/resolve/4649052661e53c7000355844105f8a1792088239/…` | Apache-2.0 (google/siglip-base-patch16-224) | **Phase 4 option** (cleanest licence; needs Unigram tokenizer + logit scale/bias constants). |
| SigLIP 2 base-patch16-224 (onnx-community) | Apache-2.0 | Text model 1.13 GB, 34 MB tokenizer: too big for now. |
| OpenAI CLIP ViT-B/16, Xenova ONNX (`vision_model.onnx` 345060583 B `b5170b47…e6b` ✔, `text_model.onnx` 254058553 B `19f40c79…77aa` ✔) | MIT code repo; HF card has no licence tag; card says deployment out of scope | Fallback only. |
| Apple MobileCLIP / MobileCLIP2 | `apple-amlr` (research only) | **Reject.** |
| RAM++ (recognize-anything-plus) | Apache-2.0 | Fixed tag set, ~3 GB, BERT text side: **not chosen.** |
| InsightFace ArcFace / buffalo_l / antelopev2 / SCRFD (incl. `immich-app/buffalo_*`) | non-commercial research only | **Reject.** |
| Ultralytics YOLO face | AGPL-3.0 | **Avoid** (network clause, licence mixing). |
| FaceNet (davidsandberg) | MIT code, weights trained on VGGFace2 (non-commercial dataset terms) | **Avoid.** |
| RetinaFace mobile0.25 (biubug6) | MIT code, WIDER FACE weights | Viable fallback detector; YuNet is smaller and has the OpenCV alignment pipeline. |
| Quantized ONNX variants (`*_quantized.onnx`, `*_int8*.onnx`) | – | **Do not use**: tract 0.23.8 failed on the dynamic-quantization graph. |

---

## 5. Design

### 5.1 Placement
| Piece | Crate / file | Notes |
|---|---|---|
| Model specs (all four entries) | `crates/li-seg/src/lib.rs` `MODELS` + new `Group::Tagging`, `Group::Faces`, new `Task::{ImageText, FaceDetect, FaceEmbed}` | Single source of truth for the Local AI manager **and** `cargo xtask attributions`. |
| Multi-file models | `ModelSpec.companions: &'static [Companion]` (new; `Companion {file, url, bytes, sha256}`), default `&[]` on existing entries | CLIP = visual (main `file`) + textual + vocab + merges as companions. `installed_bytes` = Some(sum) only if **all** files exist; `remove_with` removes all; downloads fetch all (sequentially, one progress total). |
| CLIP inference + tokenizer | new `crates/li-seg/src/clip.rs`, `crates/li-seg/src/bpe.rs` | `bpe.rs` = copy of `crates/lc-segment/src/tokenizer.rs` (keep its Apache-2.0 notice header) generalised to a `context` parameter (77) and padding id 0. |
| Faces inference | new `crates/li-seg/src/faces.rs` | YuNet decode + NMS, 5-point similarity alignment, SFace embedding. |
| Engine orchestration, storage, classification, clustering, commands | new `crates/lc-engine/src/smart_sort/{mod.rs, store.rs, classify.rs, faces.rs, plan.rs, mock.rs}`, commands in new `crates/lc-engine/src/cmd/smart_sort.rs` | Engine never sees tract types: it uses traits (§5.3). |
| Export of many folders in one batch | `crates/lc-engine/src/export.rs` | Add an optional per-item subfolder to `PreparedExport` (§5.7). |
| Dialog | new `crates/lc-ui-egui/src/panels/smart_sort.rs`; `Dialog::SmartSort` state in `state.rs`; menu row in `menus.rs`; background task in new `crates/lc-ui-egui/src/smart_sort_task.rs` | |
| Model download from the Library | `lc-ui-egui` `Services.download_models: Option<…>` + status, wired in `apps/local-image/src/library_host.rs` to `photocraft_ui_egui::ai_ui::start_seg_download` | lc-ui-egui must not depend on li-ai (keep the host boundary). |

### 5.2 li-seg additions (Phase 1 & 3)

```rust
// lib.rs
pub struct Companion { pub file: &'static str, pub url: &'static str, pub bytes: u64, pub sha256: &'static str }
pub struct ModelSpec { /* existing fields */ pub companions: &'static [Companion] }
pub enum Group { Subject, Sky, Depth, Tagging, Faces }      // ALL gets the two new ones
pub enum Task { Subject, Sky{..}, Depth, ImageText, FaceDetect, FaceEmbed }
```
- `Group::Tagging.label()` = "Smart Sort (scenes)", `about()` = "Recognises what a photo shows (speakers, audience, details…) so Smart Sort can put it in the right folder. Runs on the CPU; photos never leave this computer.", `recommended()` = "clip-b32-laion".
- `Group::Faces.label()` = "Smart Sort (faces)", `about()` = "Finds faces and recognises the same person across photos, for one folder per person. Off until you turn it on in Smart Sort. Face data stays in your library.", `recommended()` = "sface". `in_use()` returns the spec when installed. **Both face specs are needed**: the Faces group is "installed" only when `yunet` and `sface` both are.
- `size` field: 224 for CLIP, 640 for YuNet, 112 for SFace. `isnet: false`.
- MODELS `about` strings: plain language (see style of existing entries).
- `xtask/src/attributions.rs:356-362`: add `"ImageText" => "Smart Sort: sorting photos into folders by what they show."`, `"FaceDetect" | "FaceEmbed" => "Smart Sort: finding faces and recognising people (opt-in)."`; regenerate `assets/attributions.json` with `cargo +1.98.1 xtask attributions` (offline; it reads sources).

**`clip.rs`** (public API):
```rust
pub struct Clip { visual: Plan, textual: Plan, bpe: bpe::Bpe, text_i32: bool }
impl Clip {
    pub fn load(models_dir: &Path, spec: &ModelSpec) -> anyhow::Result<Clip>;
    pub fn dim(&self) -> usize;                                  // 512
    /// L2-normalised embedding of an RGBA8 image (w×h), averaging `crops` square views (§ below).
    pub fn embed_image(&self, rgba: &[u8], w: usize, h: usize, crops: Crops) -> anyhow::Result<Vec<f32>>;
    pub fn embed_text(&self, text: &str) -> anyhow::Result<Vec<f32>>; // L2-normalised
}
pub fn shared_clip(models_dir: &Path) -> Option<Arc<Clip>>;       // process-wide cache like `shared()`
pub enum Crops { Center, Three }                                  // Three = default
pub fn preprocess(rgba: &[u8], w: usize, h: usize, crop: (usize, usize, usize)) -> Vec<f32>; // pub for tests
```
- Preprocess exactly as the Immich `preprocess_cfg.json`: resize so the **shortest** side is 224 (bicubic / Catmull-Rom; `image::imageops::resize` with `FilterType::CatmullRom` is fine), take a 224×224 square, scale to 0..1, normalise with mean `[0.48145466, 0.4578275, 0.40821073]`, std `[0.26862954, 0.26130258, 0.27577711]`, NCHW f32. Alpha is ignored (composite on black is not needed: inputs are opaque).
- `Crops::Three`: three squares along the long edge (start, centre, end), each embedded, then the mean is re-normalised (covers off-centre speakers in 3:2 frames). `Center` = the centre square only. For a square image all three coincide (embed once).
- Text: `bpe.encode(text, 77)` → `[49406, …, 49407, 0, 0…]` (truncate to 76 tokens + EOT). Feed as **i32** if the model's input 0 fact is I32 (Immich export) else i64 (Xenova). Detect from `model.input_fact(0)` before setting the concrete fact; set `[1,77]`.
- Visual input fact `[1,3,224,224]` f32. Output 0 = embedding; normalise.
- Loading: `tract_onnx::onnx().model_for_path(p)?…into_optimized()?.into_runnable()?` (same as `Segmenter::load`). The plan is shared (`Arc`) and may be run concurrently from several threads.

**`bpe.rs`** golden token ids (computed with the reference CLIP BPE on 2026-10-09; use them in unit tests that build a `Bpe` from a *tiny vocabulary/merges fixture* only if needed, otherwise test with the real files in an `#[ignore]` test):
| text | ids |
|---|---|
| `a photo of a speaker on stage` | `[49406, 320, 1125, 539, 320, 4914, 525, 2170, 49407]` |
| `a photo of an audience` | `[49406, 320, 1125, 539, 550, 6863, 49407]` |
| `People applauding at a conference` | `[49406, 1047, 24713, 796, 536, 320, 2230, 49407]` (`applauding` → `applau` + `ding</w>`) |
| `sponsor booth with logo banners` | `[49406, 7574, 3971, 593, 5750, 23145, 49407]` |
| `keynote speaker at a podium` | `[49406, 7556, 4914, 536, 320, 14093, 49407]` |
Non-ignored unit tests: write a fixture vocab/merges (a dozen entries) under `crates/li-seg/tests/fixtures/` and assert merges, lower-casing, BOS/EOT, truncation at 77, padding with 0.

**`faces.rs`** (Phase 3):
```rust
pub struct FaceBox { pub x: f32, pub y: f32, pub w: f32, pub h: f32, pub score: f32, pub landmarks: [[f32; 2]; 5] } // pixels of the input image
pub struct Faces { detect: Plan, embed: Plan }
impl Faces {
    pub fn load(models_dir: &Path) -> anyhow::Result<Faces>;          // needs yunet + sface
    pub fn detect(&self, rgba: &[u8], w: usize, h: usize) -> anyhow::Result<Vec<FaceBox>>;
    pub fn embed(&self, rgba: &[u8], w: usize, h: usize, face: &FaceBox) -> anyhow::Result<Vec<f32>>; // 128-d, L2-normalised
}
pub fn decode_yunet(outputs: &YunetOutputs, pad_w: usize, pad_h: usize, score_min: f32) -> Vec<FaceBox>; // pure, tested
pub fn nms(faces: Vec<FaceBox>, iou: f32, top_k: usize) -> Vec<FaceBox>;                                  // pure, tested
pub fn similarity_transform(src: &[[f32; 2]; 5]) -> [[f32; 3]; 2];                                        // pure, tested (Umeyama, no reflection)
pub fn warp_affine_112(rgba: &[u8], w: usize, h: usize, m: &[[f32; 3]; 2]) -> Vec<u8>;                   // bilinear, RGB 112×112
```
- Detection (port of OpenCV `modules/objdetect/src/face_detect.cpp` @ `52100328d82d0502534323e9524a701baa3a1e2a`, Apache-2.0):
  input = the image scaled so its long edge is **1024** (multiple of 32), padded right/bottom with zeros to `padW = ((w-1)/32+1)*32`, `padH` likewise; channel order **BGR**, values 0..255, no normalisation, NCHW.
  Load with `.with_ignore_output_shapes(true).with_ignore_value_info(true)` and input fact `[1,3,padH,padW]` (the model file has 640×640 baked in; re-optimise per distinct padded size and cache plans by size — in practice one size per orientation).
  Outputs **by name** `cls_8, cls_16, cls_32, obj_8, obj_16, obj_32, bbox_8, bbox_16, bbox_32, kps_8, kps_16, kps_32`. For stride `s ∈ {8,16,32}`, `cols = padW/s`, `rows = padH/s`, cell `idx = r*cols + c`:
  `score = sqrt(clamp(cls[idx],0,1) * clamp(obj[idx],0,1))`; `cx = (c + bbox[4idx]) * s`, `cy = (r + bbox[4idx+1]) * s`, `w = exp(bbox[4idx+2]) * s`, `h = exp(bbox[4idx+3]) * s`, box `x = cx - w/2`, `y = cy - h/2`; landmark `n`: `((kps[10idx+2n] + c) * s, (kps[10idx+2n+1] + r) * s)`.
  Keep `score ≥ 0.9`, NMS IoU 0.3, top-k 5000 (OpenCV defaults). Map back to the original image by dividing by the scale.
- Alignment (port of OpenCV `modules/objdetect/src/face_recognize.cpp` @ `13c571a801ad5c67a752e5cd58a8a7e7725f99d2`, `alignCrop`/`getSimilarityTransformMatrix`): destination points
  `[[38.2946,51.6963],[73.5318,51.5014],[56.0252,71.7366],[41.5493,92.3655],[70.7299,92.2041]]`, similarity (rotation+uniform scale+translation) least squares, warp bilinear to 112×112.
- Embedding: input **RGB**, 0..255, NCHW `[1,3,112,112]`, output `[1,128]`, L2-normalise. Same person ⇔ cosine ≥ 0.363 (OpenCV's threshold).
- Add both rows to `docs/PORTS.md` (licence `Apache-2.0`, date of the port), and mention the sources in the module doc.

### 5.3 Engine abstraction (lc-engine `smart_sort`)

```rust
pub trait Tagger: Send + Sync {
    fn model_id(&self) -> &str;                       // part of the cache key: "clip-b32-laion"
    fn dim(&self) -> usize;
    fn embed_image(&self, img: &Rgba8) -> Result<Vec<f32>, String>;
    fn embed_text(&self, text: &str) -> Result<Vec<f32>, String>;
}
pub trait FaceModel: Send + Sync {
    fn model_id(&self) -> &str;                       // "yunet+sface"
    fn detect(&self, img: &Rgba8) -> Result<Vec<DetectedFace>, String>;              // rect normalised 0..1 in the input frame + score
    fn embed(&self, img: &Rgba8, face: &DetectedFace) -> Result<Vec<f32>, String>;
}
pub struct SmartSort {                                // `Session.smart`
    pub tagger: Option<Arc<dyn Tagger>>,              // set lazily from quick_seg_dir via li_seg::shared_clip, or a mock in tests
    pub faces: Option<Arc<dyn FaceModel>>,
    pub store: Store,                                 // §5.4
}
```
- Real implementations wrap `li_seg::Clip` / `li_seg::Faces` (in `smart_sort/mod.rs`, behind no feature flag: li-seg is already a dependency of lc-engine).
- `smart_sort/mock.rs` (always compiled, `#[doc(hidden)] pub`): `MockTagger` and `MockFaces`, deterministic, no files:
  - `MockTagger::embed_image`: an 8-d vector from the image's mean R, G, B, luminance, R−G, B−G, saturation, and 1.0, then normalised. `embed_text`: keywords in the text map to the same axes (`"red"`→R, `"green"`→G, `"blue"`→B, `"bright"`→luminance, `"dark"`→ −luminance, `"colorful"`→saturation; others → a hashed axis), normalised. This makes the demo library's procedural scenes sort deterministically in tests.
  - `MockFaces::detect`: returns one face per image whose top-left 8×8 pixel block is pure white (so tests can paint "faces"), embedding = the colour of the pixel block at (8..16, 0..8) one-hot-ish → same colour = same person.

### 5.4 Storage (no catalog bloat)
Embeddings are **not** stored in the catalog snapshot/op log (2 KB × 2,000 photos per model would bloat every save).

- Directory: `<library dir>/AI/` (create on first write). `Library.dir` is `crates/lc-engine/src/library.rs:47`. Without a library (ephemeral/tests) the store is memory-only.
- `AI/embeddings-<model_id>.bin`: header line `LIEMB1 {"model":"clip-b32-laion","dim":512}\n`, then records `[u16 key_len][key utf8][dim × f32 LE]`, key = `media::content_key(photo)` (virtual copies share a key; fine). Loaded lazily; written with `lightcraft_catalog::safe_file::write_atomic_with` after an analysis batch.
- `AI/faces-<model_id>.bin` (sensitive): header `LIFACE1 {...}\n`, records `[key][u8 n_faces]` then per face `[x,y,w,h,score: f32][128 × f32]` (rect normalised to the **full, oriented, uncropped** frame = the MWG convention).
- `AI/people.json`: `{"version":1,"people":[{"id":1,"name":"Jane Doe","faces":[["<key>",0],…],"rejected":[["<key>",2]]}],"nextId":2}` — confirmed assignments and "not this person" marks.
- `SmartSort::clear_faces()` deletes `faces-*.bin` and `people.json` (moves nothing to Trash: these are caches; the UI confirms first).
- Settings (prefs, persisted with the library like export presets, `PrefsFile` in `library.rs`): `smart_sort: SmartSortPrefs { presets: Vec<SortPreset>, last: Option<SortPreset>, faces_enabled: bool }` all `#[serde(default)]`.

### 5.5 Data model of a sort
```rust
#[serde(default, rename_all = "camelCase")]
pub struct SortPreset {
    pub name: String,                         // "Conference"
    pub keyword_parent: String,               // "Smart Sort" → keywords "Smart Sort|Speakers"
    pub categories: Vec<Category>,
    pub sensitivity: Sensitivity,             // Strict | Balanced (default) | Loose
    pub multi: bool,                          // a photo may get several categories (default false)
}
pub struct Category {
    pub name: String,                         // folder + keyword leaf: "Speakers"
    pub prompts: Vec<String>,                 // plain English, 1..n
    pub exemplars: Vec<String>,               // content keys of photos the user put here (few-shot)
    pub min_faces: Option<u32>, pub max_faces: Option<u32>,   // optional face-count gate (Phase 3)
}
pub struct FolderDef {                         // export step
    pub name: String,                          // may contain "/" for nesting, sanitised
    pub rules: lightcraft_catalog::RuleSet,    // which photos go in (reuses smart-album rules)
    pub enabled: bool,
}
```
Built-in presets (English source strings; folder name = category name):
- **Conference**: Speakers ("a person speaking at a podium on stage", "a presenter giving a talk with a microphone", "a panel discussion on a stage"); Audience reactions ("an audience applauding", "audience members laughing in their seats", "a crowd of people at a conference"); Candids & networking ("people talking and networking at an event", "a candid photo of people having a conversation"); Sponsors & exhibitors ("a sponsor booth with logo banners", "an exhibition stand at a trade show", "a branded backdrop with company logos"); Group photos ("a posed group photo of people smiling at the camera"); Venue & details ("an empty conference room with rows of chairs", "name badges, signs and event decorations", "food and drinks on a catering table").
- **Wedding**: Getting ready; Ceremony; Couple portraits; Family & group formals; Speeches & toasts; Reception & dancing; Details (rings, flowers, cake, table decor).
- **Sports**: Action; Celebrations; Fans & crowd; Team & portraits; Venue.
- **Custom**: two empty categories.

### 5.6 Classification algorithm (`smart_sort/classify.rs`, pure, deterministic)
Inputs: image embeddings `x_i` (normalised), categories `c` with prompts and exemplars, background prompts
`B = ["a photo", "a blurry photo", "a photo of an empty room", "a screenshot"]`.
1. Text prototype `t_c = normalise(mean_p normalise(embed_text(p)))` over the category's prompts. Prompts are used verbatim; if a prompt does not start with "a ", "an ", "the " or "photo", prefix "a photo of ". Background prototype `t_bg` likewise from `B`.
2. Few-shot: if the category has `n ≥ 1` exemplars with embeddings, `t_c ← normalise(t_c + γ_n · normalise(mean(x_e)))`, `γ_n = min(n, 10) / 5`.
3. Logits `l_c = 100 · cos(x_i, t_c)` for every category and the background; `p = softmax(l)`.
4. Face gate (Phase 3, only if face data exists for the photo): a category with `min_faces`/`max_faces` violated gets `p_c = 0` before step 5.
5. Assignment with thresholds `τ = {Strict: 0.60, Balanced: 0.45, Loose: 0.30}` (constants; coordinator tunes them, §9):
   - single (default): the top category if `p_top ≥ τ` and it is not the background, else **Unsorted**.
   - multi: every non-background category with `p_c ≥ τ · 0.75`; none → Unsorted.
6. Manual overrides win: a photo the user moved in review is assigned exactly as the user said (and becomes an exemplar of that category; moving it out removes it from that category's exemplars).
Output per photo: `{key, scores: [(category, p)], assigned: Vec<category>, manual: bool}`. Ties broken by category order.

### 5.7 Export of folders
- Folder membership is computed from **catalog keywords** (after "Apply keywords") using `RuleSet` evaluation, so it equals what smart albums would show. The dialog builds a default `FolderDef` per category: `rules = {match: all, rules: [{field: "keywords", op: "is", value: "<parent>|<category>"}]}` (check the Keywords ops in `rules.rs` — use the op that matches one whole keyword, case-insensitive); per named person (Phase 3): `{field: "person", op: "is", value: name}` (add a `person` field to `rules::FIELDS`, Kind::Text over named Face regions, reusing `query.rs:150`'s matcher). Users may edit any folder's rules with the existing rules editor (AND/OR/NONE and nested groups for free).
- Optional "Unsorted" folder (photos matching no enabled folder), default on.
- **Overlap policy** (`firstMatch: bool`, default `false`): `false` → a photo goes into every folder it matches (one exported file per folder); `true` → only the first matching folder in list order.
- Export: build one batch. For each `(folder, photo)` pair call `prepare_guarded` (seq counted per folder) and set the new **`PreparedExport.subfolder: Option<String>`** (sanitised folder name; `/` allowed for nesting, `..`/absolute paths rejected). In `run_batch`, the item's folder is `join(batch_dir, subfolder)`; the existing `taken` set already works on full paths. Make sure the writer creates missing directories (check `Services.write_shared` / `write` and the CLI writer; add `create_dir_all` where needed).
- Destination: user-chosen root; default `<Desktop>/<source name> – Sorted` (`dirs::desktop_dir()`, else home). Refuse a root inside the library folder or equal to an originals folder (`Session::check_write_target`).
- Export settings: any export preset (built-in or user). "Copy originals" = the built-in **Original + Settings** preset. The dialog shows the preset dropdown plus **Edit Settings…** which opens the normal Export dialog to tweak and returns.
- Metadata: categories are keywords → exported files carry them if the export's metadata policy includes keywords. Person names: only written as keywords `People|<name>` when the user ticks **Include people's names in exported files** (default off).

### 5.8 Faces → people (Phase 3)
1. Analyse: for each photo, detect on the 1024-px input; drop faces with `score < 0.9` or shorter side < 40 px (in the 1024 input); embed the rest.
2. Seed names: catalog Face regions that already have a name (XMP from Lightroom etc.) and overlap a detected face with IoU ≥ 0.5 are confirmed faces of that person.
3. Cluster unassigned faces deterministically: graph with an edge where `cos ≥ 0.50`; Chinese Whispers (nodes ordered by `(photo date, key, face index)`, label = node index initially, 30 passes or until stable; each node takes the neighbour label with the largest summed weight, ties → smallest label). Clusters of ≥ 2 faces become suggestions "Unnamed person 1…" ordered by size; singletons stay unclustered.
4. Assign to named people: a face (or a whole cluster by its centroid) joins person P if `cos(face, centroid_P) ≥ 0.45` and beats the runner-up by ≥ 0.05, unless the user rejected that pair.
5. Naming a cluster/person writes, as **one undo step**, a `Region { kind: Face, name: Some(name), rect, description: None, auto: true }` on each of its photos (new field `auto: bool`, `#[serde(default, skip_serializing_if = "std::ops::Not::not")]` on `lightcraft_meta::Region`; fix every struct literal). The existing People view, `Filter.person` and the new `person` rule field then work unchanged.
6. "Clear face data" removes the AI files and, if the user ticks "Also remove names Smart Sort added", every region with `auto == true` (one undo step).

### 5.9 Engine commands (`crates/lc-engine/src/cmd/smart_sort.rs`, registered like `cull.rs` via `specs()`)
All JSON in/out so the UI, MCP and `lightcraft-cli run` share them. Long work runs synchronously here (CLI/tests); the UI uses the background split in §5.10.
| id | params → result |
|---|---|
| `smartSort.status` | `{}` → `{tagger:{id, installed, bytes, missing:[files]}, faces:{…, enabled}, analysed:{tags:n, faces:n}, total:n}` |
| `smartSort.presets` / `smartSort.savePreset` / `smartSort.deletePreset` | built-in + user presets, like `export.presets` |
| `smartSort.analyze` | `{ids?, faces?: bool}` (default ids: selection if >1 else visible photos) → `{analysed, skipped, failed:[[id, error]]}`; skips photos already in the store for the current model |
| `smartSort.classify` | `{preset: SortPreset, ids?, overrides?: [{id, categories}]}` → `{photos:[{id, scores:{name:p}, assigned:[…], manual}], counts:{name:n, "Unsorted":n}}` |
| `smartSort.applyKeywords` | `{preset, assignments:[{id, categories}], replace?: true}` → one `Op::Batch` of `SetMeta` labelled "Smart Sort": adds `<parent>\|<category>`; with `replace` removes other keywords under `<parent>` first → `{changed}` |
| `smartSort.people` | `{}` → `{people:[{id, name, count, cover:{photo, rect}}], suggestions:[{cluster, count, cover}]}` |
| `smartSort.namePerson` | `{cluster? \| person?, name}` → writes regions (§5.8.5) |
| `smartSort.mergePeople` / `smartSort.rejectFace` | `{ids}` / `{person, photo, face}` |
| `smartSort.clearFaceData` | `{removeNames?: false}` |
| `smartSort.plan` | `{folders:[FolderDef], ids?, firstMatch?, unsorted?: "Unsorted"\|null}` → `{folders:[{name, ids}]}` |
| `smartSort.export` | `{dir, folders…, firstMatch?, unsorted?, preset? \| export params}` → same result shape as `app.export` (`[{path, photo}…]`) |

### 5.10 Background analysis (UI)
Mirror `export_task.rs`: on the UI thread `smart_sort::prepare_inputs(session, ids) -> Vec<InputJob>` creates, per photo, either the embedded-preview loader call (unedited raw: `media.preview_loader(path, 1024)`) or `preview_job(id, 1024, 1024, false, &settings)` (uncropped, as developed — face rects must be in the full frame). A worker thread runs a dedicated `rayon::ThreadPool` with `max(1, available_parallelism/2)` threads: produce the 1024 RGBA input, embed (tagger), detect+embed faces (if enabled), send results over a channel; progress `(done, total, current file)` in an `Arc<Mutex<…>>`, `AtomicBool` cancel. Results are applied to `session.smart.store` on the UI thread; the store is saved when the task finishes or is cancelled. Video files are skipped; unreadable photos are listed as "Couldn't read" in the dialog. Models load lazily on the worker the first time (first run shows "Loading model…"). Drop the loaded models when the dialog closes (memory ~1 GB).

### 5.11 UX (Lightroom/Capture One style; simple and professional)
Entry points: **File ▸ Smart Sort & Export…** (`("dialog.smartSort", "Smart Sort & Export…", None, "File")`, right after Export…) and the grid context menu **Smart Sort Selected…**. Source = selected photos if more than one is selected, else everything in the current view (folder, album, collection, date…), shown as "1,984 photos from ‘Folder: 2026-10-07 SNHU Summit’".

A single modal window (≈ 980×680, resizable) with three steps in a segmented header: **1 Sort · 2 Review · 3 Export**. Footer: Cancel (left), Back / Next or primary action (right).

**Step 1 — Sort**
- Preset: dropdown (Conference ▾ / Wedding / Sports / Custom / user presets) + "Save Preset…".
- Folders list (left, editable): each row = folder name, a one-line "what's in it" description (the prompts, editable in a multi-line field; tooltip: "Describe the photos in plain English, one idea per line"), delete (trash), "+ Add Folder".
- Options: "Sorting: ○ Strict ● Balanced ○ Loose"; "☐ A photo can go in more than one folder"; keyword parent field "Add keywords under: [Smart Sort]".
- Faces box: "☐ Also sort by person (face recognition)" with the explanation "Finds faces and groups photos of the same person. Face data is stored only in this library and can be cleared at any time." — disabled with "Download face models (39 MB)" if not installed.
- Models: if the tagging model is missing, a notice row "Smart Sort needs a one-time download of 607 MB. It runs on this computer; your photos are never uploaded." [Download] with progress/Cancel (via the host service). Licence links shown under it (MIT / Apache-2.0).
- Primary button: **Analyse 1,984 Photos** → progress bar "Analysing 312 of 1,984 — DSC04512.ARW" + Cancel; already-analysed photos are skipped ("1,200 already analysed").

**Step 2 — Review**
- Left: folder list with live counts (Speakers 214, Audience reactions 388, …, **Unsorted 97**, and with faces on: People ▸ Jane Doe 46, Unnamed person 1 31…).
- Centre: thumbnail grid of the selected folder (uses the normal thumbnail renderer), each with a small confidence bar; sort by "Least sure first" (default) or capture time.
- Correct: drag thumbnails onto a folder in the left list, or right-click ▸ Move to ▸ / Also add to ▸ / Remove from folder; multi-select with Shift/Cmd. Moved photos get a small "✓" badge (manual) and teach the folder (exemplars) — the other photos re-sort live; a toast says "Learned from 6 corrections".
- "Sorting" slider (Strict ↔ Loose) re-sorts live.
- People (faces on): cards for suggested clusters with a name field and buttons Merge / Not this person; naming offers "Create a folder for this person" (default on for named people).
- Button "Apply Keywords" is implicit: moving to Step 3 applies keywords as one undo step ("Smart Sort: added keywords to 1,887 photos").

**Step 3 — Export**
- Destination root: path field + "Choose…" (host folder picker); default Desktop.
- Folder table: ☑ enabled, folder name (editable), rule summary ("Keyword is Smart Sort|Speakers" / "Person is Jane Doe" / custom), count, "Edit Rules…" (rules editor). Rows for Unsorted and per person.
- "If a photo matches several folders: ● Put a copy in each ○ Only the first folder".
- Export as: preset dropdown (JPEG (Large) default / JPEG (Small) / Original + Settings ("copy originals") / user presets) + "Edit Settings…".
- "☐ Include people's names in exported files" (faces on only). "☐ Also create a smart album for each folder".
- Summary: "2,431 files into 8 folders (1,984 photos)". Primary: **Export** → closes the dialog and runs the normal background export panel (progress, Cancel), then "Show in Finder/Files".

Automation ids (register them; tests use them): `smartSort:step:{sort,review,export}`, `smartSort:preset`, `smartSort:addFolder`, `smartSort:folder:<index>`, `smartSort:prompts:<index>`, `smartSort:faces`, `smartSort:download`, `smartSort:analyze`, `smartSort:cancel`, `smartSort:reviewFolder:<name>`, `smartSort:thumb:<photoId>`, `smartSort:moveTo:<name>`, `smartSort:sensitivity`, `smartSort:dest`, `smartSort:exportFolder:<index>`, `smartSort:firstMatch`, `smartSort:exportPreset`, `smartSort:export`. Dialog state is inspectable through `ui.inspect` like the export dialog.

### 5.12 i18n
- All new Library strings go through `crate::i18n::tr("…")` / `tr_format!`. Add the English keys with translations to `crates/lc-ui-egui/locales/{ja,pt-br,zh-hans,zh-hant}.json` (and `*-formats.json` for formatted ones, keeping placeholders identical) — translations may be machine-quality; leave a key out rather than invent nonsense (gaps are allowed and reported).
- Group labels/abouts for the Compositing Local AI window are shown via `tl!(group.label())`; add rows (`<TAB>source<TAB>translation`, empty context) for the two new labels and abouts to all 12 `crates/pc-ui-egui/src/i18n/*.tsv`. If you add any new `tl!("literal")` in pc-ui-egui, it **must** get a row in every complete-language TSV or `every_tl_literal_is_translated` fails.
- Preset category names and default prompts stay English in the data (they are user-editable content and the model reads English); the preset *names* in the dropdown are translated for display only.

---

## 6. Phases (each sized as one Codex job; each ends green)

Common commands (run all that touch your crates):
```
CARGO_BUILD_JOBS=4 cargo +1.98.1 test --offline -p li-seg
CARGO_BUILD_JOBS=4 cargo +1.98.1 test --offline -p lightcraft-catalog
CARGO_BUILD_JOBS=4 cargo +1.98.1 test --offline -p lightcraft-meta
CARGO_BUILD_JOBS=4 cargo +1.98.1 test --offline -p lightcraft-engine
CARGO_BUILD_JOBS=4 cargo +1.98.1 test --offline -p lightcraft-ui-egui
CARGO_BUILD_JOBS=4 cargo +1.98.1 test --offline -p photocraft-ui-egui
CARGO_BUILD_JOBS=4 cargo +1.98.1 build --offline -p local-image
cargo +1.98.1 xtask attributions   # when MODELS changes; commit assets/attributions.json
cargo +1.98.1 fmt --all -- --check
git diff --stat Cargo.lock          # must show no new [[package]]
```
(If the binary package is named differently, find it with `cargo metadata --offline --no-deps --format-version 1`.)

### Phase 1 — Tagging engine + storage + commands (no UI)
Tasks:
1. li-seg: `Companion`, `companions` on every `ModelSpec` (existing ones `&[]`), `Group::{Tagging, Faces}` (Faces entries are added in Phase 3 — in Phase 1 add only the `Tagging` group and the CLIP spec), `Task::ImageText`, `installed_bytes`/`remove_with`/`model_path` handling companions, `spec()`.
2. li-seg: `bpe.rs` (from lc-segment tokenizer, context 77, pad 0), `clip.rs` (§5.2) with `shared_clip`.
3. Compositing manager: `start_seg_download` downloads main file + companions in sequence with one cumulative progress; Remove removes all files; allow-list test still passes (add companion URLs to the check at `ai_ui.rs:1415`).
4. xtask `used_for` mapping; regenerate `assets/attributions.json`.
5. lc-engine: `smart_sort` module: traits, real wrapper over `li_seg::Clip`, mocks, `Store` (embeddings file I/O), presets (§5.5) with serde defaults in library prefs, `classify.rs` (§5.6), commands `status`, `presets/savePreset/deletePreset`, `analyze` (tags only), `classify`, `applyKeywords`, `plan` (keywords-based folders).
Acceptance:
- `li_seg` unit tests: BPE fixture tests (merges, lower-case, BOS/EOT, truncation, padding); `preprocess` produces mean/std-normalised values for a synthetic image (e.g. a uniform (128,128,128) image → every value `(128/255 − mean_c)/std_c` within 1e-5) and for a 300×200 input (resized to 336×224) the three crops start at x = 0, 56, 112; companions make `installed_bytes` None until all files exist (temp dir).
- `#[ignore]` real-weight tests in li-seg, run by the coordinator with `LOCAL_IMAGE_MODELS_DIR=<dir> cargo +1.98.1 test --offline -p li-seg -- --ignored`: the golden token ids of §5.2 with the real vocab/merges; zero-shot ranking on `docs/upstream/lightcraft/images/ba-tetons.jpg` ("a photo of a mountain landscape" first), `ba-earthrise.jpg` ("…earth from space" first), `ba-migrant-mother.jpg` ("…mother with her children" or "…black and white photo of people" first) against the prompt list in §9.
- Engine tests with `MockTagger` on `Session::with_demo()`: `smartSort.analyze` stores one embedding per photo and is idempotent (second run analyses 0); `smartSort.classify` puts a red-dominant demo scene into a category whose prompt is "red" and an unrelated one into Unsorted under Strict; exemplars move a borderline photo; `multi` assigns 2 categories when both pass; manual overrides win; `applyKeywords` is one undo step (undo restores keywords exactly) and `replace` removes stale `Smart Sort|*` keywords only; the store round-trips through a temp library dir (`open_library`), and a corrupt/truncated file is ignored with a log warning, not a crash; presets persist with the library and old prefs files without `smartSort` still load.

### Phase 2 — Dialog, review, export
Tasks:
1. `PreparedExport.subfolder` + `run_batch` support + directory creation; `smartSort.export` command; `smartSort.plan` with `firstMatch` and Unsorted.
2. `panels/smart_sort.rs`, `Dialog::SmartSort { step, preset, … }`, menu row, grid context-menu item, background analysis task (`smart_sort_task.rs`, §5.10), live re-classify on edits, drag-and-drop/“Move to” corrections, keyword apply on Next, export via `export_task::start` with the batch from step 3.
3. Host service for model download from the Library (`Services.download_models` + status) wired in `apps/local-image/src/library_host.rs` to the Compositing downloader; without the service (tests, web) the Download button is hidden and the notice says "Download the Smart Sort model in Compositing ▸ Local AI".
4. Rules: Phase 2 uses only `keywords` rules; the "Edit Rules…" button reuses `rules_editor.rs`.
5. i18n rows (§5.12).
Acceptance:
- Engine: exporting a 3-folder plan of demo photos (PNG, long edge 64, to a temp dir) creates `<root>/<folder>/…` files; with overlap and `firstMatch:false` a photo appears in both folders, with `true` only in the first; names never collide across folders; a destination inside the library is refused; `..` in a folder name is rejected.
- Headless UI tests (`crates/lc-ui-egui/src/tests_smart_sort.rs`, mock tagger injected into `app.session.smart`): open via menu id `dialog.smartSort`; step 1 shows the preset's folders; clicking `smartSort:analyze` runs to completion (`h.step_until`) and step 2 lists counts that sum to the photo count (+ duplicates when multi); clicking a thumbnail then `smartSort:moveTo:<name>` changes the counts and marks it manual; Next applies keywords (catalog shows `Smart Sort|<name>`; Edit ▸ Undo removes them in one step); step 3 with a temp destination and `smartSort:export` produces the files and the export progress panel finishes; Cancel during analysis stops it and keeps the partial store; the dialog renders without overflow at 1280×800 and 1920×1080 (`ui.screenshot` non-empty, widgets inside the window rect).
- `photocraft-ui-egui` tests still pass (allow-list, attributions, i18n).

### Phase 3 — Faces and people
Tasks: li-seg `faces.rs` + YuNet/SFace specs in `Group::Faces`; `Task::FaceDetect/FaceEmbed`; engine faces store, clustering (§5.8), people commands, `Region.auto`, `person` rule field, face gate in classify, "Clear face data"; dialog: opt-in checkbox, People section in Review, per-person folders in Export, "Include people's names" option; PORTS rows; attributions regen.
Acceptance:
- Pure tests: `decode_yunet` on hand-made output tensors (one cell above threshold at stride 8, c=3, r=2, bbox [0.5,0.5,ln 2,ln 2] → box centre ((3.5)·8,(2.5)·8) = (28,20), size 16×16; landmarks decoded likewise); NMS keeps the higher score of two IoU 0.8 boxes and both of two disjoint ones; `similarity_transform` maps the reference points to themselves (identity) and a scaled/rotated/translated copy back within 1e-3; `warp_affine_112` of a constant image is constant.
- Engine with `MockFaces`: faces are stored only when `faces_enabled`; clustering groups same-colour faces and separates different ones deterministically; naming a cluster writes `Region{auto:true}` regions in one undo step and the People view lists the person; `person` rule selects them; XMP-named regions seed people; `clearFaceData` deletes the files and (with `removeNames`) only `auto` regions; `people.json` round-trips.
- Headless: faces checkbox off by default; turning it on and analysing shows suggestions; typing a name and confirming creates a person folder row in step 3.
- `#[ignore]` real-weight test: YuNet finds ≥ 1 face with score ≥ 0.9 in `ba-migrant-mother.jpg` (two copies side by side → expect 2 faces after NMS, centres near x≈620 and x≈1868 of 3200, y≈572) and the two SFace embeddings have cosine ≥ 0.6 (same woman); none in `ba-tetons.jpg`.

### Phase 4 (optional, owner decision Q1) — SigLIP model family
Add `Task::ImageText` variant data `{family: Clip|Siglip}`; SentencePiece-Unigram tokenizer from `tokenizer.json` (lower-case, strip punctuation, collapse spaces, prefix `▁`, spaces→`▁`, Viterbi over piece scores, unknown → id 2, append `</s>` id 1, pad to 64 with id 1); preprocessing resize to 224×224 (no crop), mean/std 0.5; sigmoid scoring `σ(scale·cos + bias)` with scale/bias read by the coordinator from the full `onnx/model.onnx` initializers (`logit_scale`, `logit_bias`) and pinned as constants. Keep CLIP as the "Fast" option.

---

## 7. Edge cases the implementation must handle
- Offline originals: use smart previews / cached thumbnails if the original is missing; otherwise list under "Couldn't read".
- Virtual copies share an embedding (same content key) but get their own keywords/folders.
- Rejected photos (flag = reject) are excluded from the source by default (checkbox "Include rejected photos").
- Videos are skipped and counted ("12 videos skipped").
- Very large galleries: the store is append-only per batch and written atomically; analysis can be cancelled and resumed.
- Model files partially downloaded or wrong hash: treated as not installed (the downloader already verifies).
- Folder names: sanitise `\ : * ? " < > |` and control characters, trim dots/spaces, empty → "Untitled", case-insensitive duplicates get " 2".

## 8. Risks
| Risk | Mitigation |
|---|---|
| Zero-shot accuracy lower than users expect on event-specific categories | Mandatory review step, Unsorted bucket, few-shot exemplars, editable prompts, sensitivity slider; coordinator tunes defaults on a real gallery. |
| CLIP model-card "deployment out of scope" language | Licence is MIT; offer SigLIP (Apache-2.0) in Phase 4; owner decides (Q1). |
| Face recognition legal exposure (GDPR Art. 9, BIPA, etc.) and dataset provenance | Opt-in per library, local-only storage, clear-data action, names not exported by default; explain in UI and docs; owner decides (Q2). |
| RAM (~1 GB with CLIP text+visual loaded) | Load lazily, drop when the dialog closes; text model only needed while prompts change. |
| tract performance on low-end CPUs (≈4–8× slower than the test machine) | Dedicated half-core pool, resumable, progress + ETA; `Crops::Center` fallback setting if too slow. |
| tract fails on quantized ONNX | Only fp32 files are pinned. |
| Raw decode cost when no embedded preview | Embedded previews for unedited raws; cached renders otherwise. |

## 9. Coordinator steps (need network / real weights; not for Codex)
1. Download the four CLIP files and two face files from §4 into a models dir (`<models_dir>/segmentation/…` with the "save as" names), verify SHA-256.
2. Run the `#[ignore]` tests: `LOCAL_IMAGE_MODELS_DIR=<dir> CARGO_BUILD_JOBS=8 cargo +1.98.1 test --offline -p li-seg -- --ignored`. Prompt list for the zero-shot check: "a photo of a mountain landscape", "a photo of a mother with her children", "a photo of the earth from space", "a photo of a speaker on stage", "a photo of a crowd of people", "a black and white photo of people", "a photo of an audience", "a portrait of a woman". Reference cosine values measured 2026-10-09 with tract 0.23.8 (centre crop, LAION B-32): tetons 0.216 mountain (top), earthrise 0.222 earth (top), migrant mother 0.226 b/w people, 0.221 mother with children (top two).
3. Timing on ≥ 500 real photos (with the owner's permission; client photos never leave the machine, never viewed by agents): seconds per 100 photos for tags and faces; RSS peak.
4. Accuracy tuning: with a real event gallery the owner/friend has hand-sorted (or sorts a 200-photo sample), run `smartSort.classify` via `lightcraft-cli run`, compute the confusion matrix, adjust `τ` and default prompts.
5. Review the Codex diffs: `git diff Cargo.lock` (no new packages), fmt, clippy, all tests, then install/smoke-test the app.

## 10. Open product questions for the owner
1. **Which tagging model by default?** LAION CLIP B-32 (MIT licence, fast, built first; its model card calls deployed use "out of scope" as non-binding guidance) or wait for SigLIP (Apache-2.0, more accurate, 3× slower, ~1 extra Codex job)?
2. **Ship face recognition at all, and opt-in per library?** It is biometric processing of client photos; everything stays local, but the photographer is responsible for consent. The face models' licences are permissive (MIT / Apache-2.0); their training datasets are research face datasets (industry-normal, but not "clean").
3. **Overlap default:** a photo in several folders gets a copy in each (proposed) or only the first match?
4. **Export default:** rendered JPEG (Large) with edits (proposed), or copy the original files?
5. **People's names in exported files:** keep off by default (proposed)?

## 11. Owner decisions (2026-10-09)

1. Default tagging model: **CLIP** (OpenCLIP ViT-B-32, section 4.1). SigLIP not planned for now; keep the family pluggable.
2. Face recognition: **yes, opt-in per library** (Phase 3), with "Clear face data"; face data never leaves the machine.
3. Photo matching several folders: **a per-sort choice in the dialog** ("Copy into every matching folder" / "First
   matching folder only"), defaulting to copying into every matching folder; saved with the sort preset.
4. Export default: **rendered JPEGs** through the normal export settings; "Copy originals" is the alternative. People's
   names are not written into exported files unless the user turns that on (off by default).
5. **Folders are defined by a list of tags** (owner, 2026-10-09). In Step 1 each folder row shows its tags as editable
   chips with a type-ahead field ("speaker, podium, microphone" — comma or Enter adds a tag; × removes; drag to reorder;
   paste a comma-separated list). A tag may be a word or a short phrase ("panel discussion", "sponsor logo banner").
   Tags are the folder's `prompts` (no schema change; old presets' sentences simply show as long chips). Each tag is
   turned into a CLIP prompt by the existing §5.6 prefix rule ("a photo of a speaker").
   - **Matching**: a folder matches when the photo strongly matches **any** of its tags (default) — score per category
     is the max over its per-tag prototypes (each tag prototype gets the same exemplar boost), instead of the mean of all
     prompts. A per-folder toggle "Match: Any tag / All tags" (All = min over tags) is saved in the category
     (`match_all: bool`, serde default false). Update classify.rs, its tests and the built-in presets (convert their
     sentences to short tag lists, e.g. Speakers: "speaker on stage", "presenter with microphone", "podium",
     "panel discussion").
   - **Tag sets**: a "Tag sets" menu on each folder row: "Save Tags as Tag Set…" (name it, e.g. "Speakers") and a list
     of saved tag sets to apply (replace or add). Tag sets are user data saved next to the user sort presets
     (`smart-sort-tagsets.json`, same atomic save), with Rename/Delete in a small manager. Built-in tag sets are derived
     from the built-in presets' folders. Whole-dialog presets ("Save Preset…") remain and store each folder's name, tags,
     match mode and rules.
   - Engine: commands `smartSort.tagSets`, `smartSort.saveTagSet`, `smartSort.deleteTagSet`, `smartSort.renameTagSet`;
     tests for persistence, any/all matching, and that a folder with tags ["podium"] beats one with ["crowd"] on a mock
     embedding near "podium". Headless UI tests type tags into a folder, save a tag set, apply it to another folder.
   - Automation ids: `smartSort:tags:<index>`, `smartSort:tagInput:<index>`, `smartSort:tagSetMenu:<index>`,
     `smartSort:matchAll:<index>`.
6. **Person folders are opt-in, per person** (owner, 2026-10-09): there is no folder per person by default. The user
   picks the few people who should get a folder (e.g. the keynote speaker) and gets a folder of just their photos.
   This overrides §5.11's "Create a folder for this person (default on for named people)" — it is now **off** by default,
   and unnamed clusters never get folders.
   - **Pick a person from a photo** (main entry point): in Step 2 (and from the Library grid/loupe context menu
     "Find This Person…"), the user picks a photo of the person, the dialog shows that photo with its detected faces
     outlined, they click the face, type a name ("Keynote — Dr. Jane Doe") and press **Find Photos**. The engine
     ranks every analysed photo by that face's similarity (assignment rules of §5.8 step 4, using the picked face(s) as
     the person's centroid), shows the matches in a review grid with "Not this person" to reject, and clicking more
     faces of the same person refines the centroid. **Create Folder** adds an export folder row whose rule is
     "Person is <name>" (default folder name = the person's name, editable).
   - **From suggestions**: the People section's cluster cards still exist (to name people quickly) but each card has an
     unticked "Folder" checkbox; only ticked people get folder rows in Step 3.
   - People folders combine with tag folders: a photo can be in "Speakers" and "Keynote – Jane Doe" (per the
     multi-match choice). A person folder may optionally be narrowed by tags ("Jane Doe on stage only" = person rule AND
     tag folder rule) via the existing "Edit Rules…".
   - Face analysis can be limited to the people the user is looking for: if no suggestions are wanted, Smart Sort only
     needs embeddings; the People suggestions panel is collapsed by default and computed lazily.
   - Engine commands: `smartSort.facesInPhoto {photo}` → faces with rects; `smartSort.findPerson {name, faces:[…]}` →
     ranked matches; `smartSort.rejectFace`, `smartSort.personFolder {person, enabled}`. Tests: picking one face of a
     mock person finds that person's other photos and not others; rejecting removes a match; folders appear only for
     ticked/created people. Headless: open a photo, click a face (automation id `smartSort:face:<photoId>:<i>`), name it,
     Find Photos, Create Folder → one folder row in Step 3.
7. **Export folder setup section** (owner, 2026-10-09). Step 3 has a "Folders" section that decides exactly which
   folders are written:
   - **Default: one folder per tag category** — every Step 1 category (e.g. "Speakers" with its tag list) gets a folder
     row named after it, plus an "Unsorted" row (unticked by default). Rows can be unticked, renamed, reordered.
   - **"+ Custom Folder"**: a folder built from any combination of categories/tags and (if enabled) people. The editor is
     simple pickers, not the full rules editor: "Include photos tagged: [Speakers ×] [Panel ×] [+]" (any of),
     "and/or showing: [Jane Doe ×] [John Roe ×] [+]" (any of the people), with a combine switch "Tagged AND showing"
     / "Tagged OR showing". Example: a "Keynote" folder = people {Jane Doe} only; "VIPs" = people {Jane, John, Ana};
     "Jane on stage" = tag Speakers AND person Jane. "Advanced Rules…" still opens the full rules editor.
   - **"☐ People folders"** checkbox in that section (only shown when face recognition is on): unticked = no person
     pickers and no person rows at all. Ticking it shows the people pickers in custom folders and the "Find This
     Person…" button (item 6) to add people; it does NOT create a folder per person automatically.
   - A folder may hold several people (any of them). Default folder rows stay alongside custom ones; the
     multi-match choice ("a copy in each" / "first matching folder only", item 3) applies across all rows in order.
   - The folder layout (default rows' enabled/renamed state + custom folders) is saved in the sort preset; people are
     stored by person id so presets reused in another library skip unknown people with a notice.
   - Phase split: tag-based default rows + custom folders with tag pickers in Phase 2; the People folders checkbox,
     person pickers and person rows in Phase 3.
   - Automation ids: `smartSort:folders:default:<index>`, `smartSort:folders:addCustom`, `smartSort:folders:custom:<index>`,
     `smartSort:folders:tagPicker:<index>`, `smartSort:folders:peoplePicker:<index>`, `smartSort:folders:combine:<index>`,
     `smartSort:peopleFolders`.
   - Tests: default rows = categories; unticking a row skips it in the plan; a custom folder with tags {A,B} contains
     photos assigned A or B; AND/OR with people (Phase 3, mock faces); preset round trip of the layout.
8. **People finder = face bubbles + per-folder people toggle** (owner, 2026-10-09; refines items 6–7):
   - The People panel shows every detected person as a **round face bubble** (best-quality crop of their clearest face),
     **sorted by how many photos they appear in, most first**, with the photo count as a badge. Each bubble has an
     inline name field ("Add name") and merges/splits via drag-onto-bubble and "Not the same person".
   - Each folder row in the Folders section has a **"People" toggle**. When a folder's People toggle is on, the face
     bubbles show a checkbox for that folder: ticking (or clicking) a bubble puts that person in the folder. Selecting a
     folder highlights which bubbles are in it. Examples: folder "Jane Doe" → People on → click Jane's bubble;
     folder "Afternoon speakers" → People on → click three speakers' bubbles (the folder holds photos of any of them).
   - Tag folders keep working independently: the automatic "Speakers" tag folder still collects everyone photographed
     speaking, alongside the people folders.
   - A bubble can be in several folders. Unnamed bubbles can be selected too (folder rule stores the person id; the
     export uses the folder's name, not the person's name, so naming is optional).
   - Bubbles load lazily (top 60 first, "Show all") and update live as analysis progresses.
   - This replaces the separate cluster-card UI of §5.11/item 6; "Find This Person…" from a photo remains as a way to
     create/locate a bubble.
   - Automation ids: `smartSort:bubble:<personId>`, `smartSort:bubbleName:<personId>`, `smartSort:folderPeople:<index>`.
