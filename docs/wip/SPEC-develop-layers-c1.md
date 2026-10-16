# SPEC: Capture One-style layers in Develop (C1)

Status: research + design, read-only on code. Author: research agent, 2026-10-13. Written incrementally.

KEY FINDING UP FRONT: the **data model, pipeline and commands for Capture One-style layers already exist** (`Mask` = layer, `LayerTools` = sparse partial settings, `lc-pipeline/src/layers.rs`; DEVELOP-DESIGN.md section 3.2 "Built (October 2026)"). What the owner is asking for is mostly a **UI-integration change** (layers selected from the normal Develop tabs instead of a separate Masking panel) plus a few missing capabilities (layer WB picker, GPU path for layer tools, curve presets/targeted adjust on layers). Part 3 is therefore an incremental plan, not a rewrite.

## Part 1 - Capture One reference

Caveat: support.captureone.com returns HTTP 403 to our fetcher; the facts below come from search-result snippets of C1 pages (cited by URL) and third-party tutorials. Items marked (verify) could not be confirmed for C1 16.8 and should be checked by the owner in his own copy (he uses C1, so he can confirm in two minutes).

### 1.1 Layers tool (structure)
- A **Background** layer is the image itself; adjustment layers sit on top; every adjustment on Background and each layer adds up to the final result. Up to **16 layers** on top of Background (older versions: 10). Each layer has its own mask. https://imagealchemist.net/capture-one-layers-explained/ , https://www.tutkit.com/en/text-tutorials/22526-effectively-using-layers-in-capture-one-pro
- Add layer with "+": **New Empty Layer** (no mask: nothing happens until a mask is drawn), **New Filled Layer** (mask covers the whole image; used for "affect everything, then erase"), plus **Clone** and **Heal** layer types. Brush stroke with no layer selected auto-creates "Layer 1". Double-click/click renames a layer. https://digital-photography-school.com/getting-started-with-layers-in-capture-one-pro/ , https://www.tutkit.com/en/text-tutorials/22526-effectively-using-layers-in-capture-one-pro
- **Per-layer opacity** slider 0-100 (default 100), at the top of the Layers panel. Global edits made on the Background are "proportionally" visible through layers; select the Background to make global edits. https://www.tutkit.com/en/text-tutorials/22526-effectively-using-layers-in-capture-one-pro
- Mask creation (16.x): Brush (B), Erase (E), Linear Gradient, Radial Gradient (parametric since C1 12; the old Gradient Brush was removed), Magic Brush, and AI masks: Subject, Background (16.3), AI Select (click-to-add, additive, 16.3), People parts incl. Pupil/Iris/Sclera (16.5), Clothes (16.7). https://www.capturemag.com.au/news/capture-one-pro-16-3-released-with-new-features-and-performance-improvements , https://support.captureone.com/hc/en-us/articles/14360621972509
- **Luma Range** (16.7.0+): reached from right-click on the layer, or "Combine Mask" in the **Layers & Masks** tool: Add / Subtract / **Intersect** with Luma Range (left handles = shadows, right = highlights, with feather). https://support.captureone.com/hc/en-us/articles/360002622777 (Luma Range Overview)
- **Combine Masks** (16.7.0, Oct 2025): add/subtract/intersect Subject, Background, People, Brush, Magic Brush, Linear/Radial Gradient, AI Select, Luma Range into one layer's mask group; gradient and Luma Range submasks stay parametric (dynamic) until rasterized. 16.7.2: mask thumbnails in the Layers tool, rename submasks, export masks as PSD alpha. 16.7.3: copy-layer options (Skip/Add as duplicates/Replace). Source: 16.7 release notes https://support.captureone.com/hc/en-us/articles/31141690629917-Capture-One-16-7-release-notes ; 16.7.2 https://support.captureone.com/hc/en-us/articles/32384987653277-Capture-One-16-7-2-release-notes ; coverage https://petapixel.com/2025/11/05/capture-one-update-adds-seven-new-features-broadens-camera-support
- Invert Mask, Fill Mask, Copy Mask are in the layer's action menu / right-click; Refine Mask (feather/edge) for AI masks (verify exact label); Rasterize converts a parametric mask to a paintable one. https://www.captureone.com/blog/ramp-up-your-efficiency-with-local-adjustments , https://www.digitalcameraworld.com/tutorials/capture-one-luma-ranges-how-to-use-them-for-complex-adjustments-and-masks

### 1.2 How selecting a layer redirects tools
- The Layers tool lives under the histogram on the Exposure tab (older: its own Local Adjustments tab; C1 11: on Color, Exposure and Details tabs). https://digital-photography-school.com/getting-started-with-layers-in-capture-one-pro/ , https://imagealchemist.net/capture-one-layers-explained/
- Selecting a layer makes layer-enabled tools act on that layer's masked area; **a small brush icon next to the tool name in its header** shows the tool is currently editing the selected layer. Per-tool option "Adjust Selected Layer" (small triangle in the tool header) switches a tool between layer and Background; tools on the Quick tab are NOT pre-activated for layers and default to Background. https://support.captureone.com/hc/fr/community/posts/360012310218-Feature-Adjust-Selected-Layer , https://support.captureone.com/hc/es/community/posts/360012197217-Local-Adjustments-in-Quick-Tab
- A **layer drop-down sits at the left end of the tool-tab bar** (shown only when adjustment layers exist) so you can change layer without going back to the Layers tool. https://support.captureone.com/hc/de/community/posts/360012113377-Layer-drop-down-box-next-to-tool-tabs
- Non-layer tools always act globally regardless of the selected layer. https://support.captureone.com/hc/en-us/articles/360002615097 (search snippet)
- Tool history of layer support: 6.4.5 Exposure, Color Editor (Advanced + Skin Tone only), Sharpening, Moire, Clarity; C8 added HDR, **White Balance**, Noise Reduction, Purple Fringing; C9 Curve incl. Luma Curve; C11 Color Balance and Levels (so "almost all adjustments"). Color Editor Basic is not layer-capable (older info, verify). Grain (and by extension Vignetting) are Background-only. https://support.captureone.com/hc/en-us/community/posts/360009396238-tool-functionality-in-layers (snippet), https://www.tutkit.com/en/text-tutorials/22526-effectively-using-layers-in-capture-one-pro . Lens/crop/rotate/base characteristics/ICC/keystone are image-level (verify list).
- Applying a Style/Preset: "Apply to Background" or to a layer. https://support.captureone.com/hc/en-us/articles/360002619458-Applying-Styles-and-Presets-to-Layers

### 1.3 Mask display and shortcuts
- Mask is a **red overlay** (colour changes in Preferences > Appearance); hidden by default except while painting; **M** toggles Always/Never display; modes in the brush/cursor panel: Always, Never, "Only when drawing". Mask and the adjustment cannot be shown at once. https://imagealchemist.net/capture-one-layers-explained/ , https://www.captureone.com/blog/discover-the-shortcuts-of-the-adjustments-layers-tool
- Shortcuts: **B** draw mask, **E** erase mask, **G** gradient, **M** toggle mask display, **[ ]** brush size, Shift-click straight line, Shift while dragging gradient = 45-degree snap. Layer add/next have no default key (assignable). https://www.captureone.com/blog/discover-the-shortcuts-of-the-adjustments-layers-tool , https://digital-photography-school.com/getting-started-with-layers-in-capture-one-pro/
- Checkbox beside each layer toggles its effect; 16.7 shows mask thumbnails per layer.

### 1.4 Skin Tone tool (Color Editor > Skin Tone tab)
- Tab next to Basic / Advanced. The picker (an eyedropper with its **own icon, different from the other Color Editor pickers**, next to the colour wheel; also in the cursor-tool toolbar) samples skin. After a pick, the **colour wheel shows the selected range** (a wedge/wireframe selection on the wheel). **Smoothness** sets how far the selection extends. **Amount** sliders (Hue / Saturation / Lightness) change the selected colour; **Uniformity** sliders (Hue / Saturation / Lightness) pull the range toward the reference so skin evens out. **"View selected color range"** checkbox shows the selected pixels in colour and everything else in black and white. https://mastinlabs.com/blogs/photoism/how-to-edit-skin-tone-in-capture-one , https://support.captureone.com/hc/en-us/articles/360002596077 , https://site.creativelive.com/class/advanced-color-correction-in-capture-one-david-grover/lessons/dive-into-the-color-editors-skin-tone-tab
- Standard workflow is: Filled layer, Skin Tone tab, pick, uniformity, M to refine mask (erase eyes/lips), lower layer opacity. i.e. Skin Tone is a layer-capable tool. (Mastin Labs link above.)
- White Balance in C1 has Normal and Skin-tone pick modes with a picker in the WB tool and in the cursor-tool toolbar (could not confirm iconography from text sources; owner can verify; we use a "WB dropper" and a "face/skin dropper" - see Part 4).

(Part 2 onward follows.)

## Part 2 - Current state (evidence)

Paths are relative to the repo root. Line numbers are from branch `wip/up-raw` at 6024e4c7.

### 2.1 The layer model already exists (Capture One style, built October 2026)
- `DevelopSettings.masks: Vec<Mask>` - `crates/lc-develop/src/settings.rs:40`. A `Mask` **is** the layer: `id, name, visible, invert, components: Vec<MaskComponent>, adjust: LocalAdjustments, refine, opacity (0..100), tools: LayerTools` (`settings.rs:1136-1160`). Doc comment at `settings.rs:1131`: "A mask is a develop layer (Capture One-style)".
- A layer carries TWO sets of adjustments:
  1. `adjust: LocalAdjustments` (`settings.rs:944`) - the 20 Lightroom-style local sliders (temp, tint, exposure, contrast, highlights, shadows, whites, blacks, texture, clarity, dehaze, hue, saturation, sharpness, noise, moire, defringe, colour overlay hue/sat, amount). Summed across masks in one log-domain term; this is also what Lightroom XMP import fills.
  2. `tools: LayerTools` (`settings.rs:1190`, keys `LAYER_KEYS` at `settings.rs:1223`): sparse partial develop settings: `wb, light, curve, color, mixer, point_colors, treatment, bw_mix, grading, effects, vignette, grain, detail, skin_tone` (14 sections; each `Option`, omitted from JSON when unset, so old settings serialize/hash identically). Values are relative to the photo's own (WB is the layer's own white). So **a layer already holds the full develop toolset except image-level tools** (profile/look, tone equalizer, raw/demosaic, optics/lens, calibration, colour-cal, negative, crop/geometry, spots, red eye, AI enhance).
- Mask components: Brush, Linear, Radial, ColorRange, LuminanceRange, DepthRange, Subject, Sky, Background, Object (click-SAM), Prompt (describe), People, Landscape; ops Add/Subtract/Intersect, per-component invert, per-layer invert and Refine Edges (`settings.rs:1028-1117`). There is **no "whole image / filled" shape** and an empty-component layer is skipped by the pipeline (`lc-pipeline/src/layers.rs:36-38` `evaluated()` filters `!m.components.is_empty()`), so "full layer" cannot be expressed today.
- Commands (`crates/lc-engine/src/cmd/masks.rs`): `mask.add` (168), `mask.addComponent`, `mask.brushStroke`, `mask.update`, `mask.adjust` (513), `mask.setTools` (533; merges partial settings or `{controlId: value}`, clamps), `mask.setOpacity` (576), `mask.resetTools` (590), `mask.select` (620), `mask.delete/rename/invert/duplicate/move/visible`, `pointColor.pick/delete {mask}`, `skinTone.pick {mask}` (`cmd/color.rs:173`). Selection state is `Session.active_mask: Option<u32>` (`lc-engine/src/lib.rs:167`), UI-only, not in undo.
- Not layer-aware yet: `develop.wbPick` has no `mask` param (`cmd/develop.rs:333`), so the layer WB section has no picker (`ui edit.rs:191` shows only a sub-title); layer curve has no presets and no targeted adjustment (`edit.rs:1054-1058` hides TAT for layers; `curve_footer` returns early with Reset only for layers, `edit.rs:1251-1262`).

### 2.2 Pipeline (CPU and GPU)
- Design: "darktable way" - each tool runs once at its own stage and every layer that sets it blends `out = mix(in, tool_L(in), alpha_L)`, alpha = mask x amount x opacity (`crates/lc-pipeline/src/layers.rs:1-26`, `masks.rs:103 mask_scale`). N layers cost one render plus per-layer work only for the tools they set; NR is a second denoise of the linear image per NR-layer, cached (`layers.rs:12`).
- CPU: `layers::resolve` (`layers.rs:93`) builds `LayerK` per layer; mask alpha planes via `masks::evaluate*` (`masks.rs:27-100`).
- GPU: layer tools are now native. `lc-pipeline/src/lib.rs:495` `layers_need_cpu` is hard-coded `false` (DEVELOP-DESIGN.md "Not yet: a WGSL port of the layer tools" is **stale**; commit cad2cc48/62f0e158 "every Develop tool on the GPU ... layer tools"). GPU loop `lc-gpu/src/primary.rs:~811-830` (`for (mi, m) in s.masks...`, cache scope `layer/{mi}` so editing one layer does not recompute others); alpha buffers `lc-gpu/src/render.rs:1211-1380` (one `n`-float plane per layer, shapes with a kernel, else CPU fallback `record_cpu_stage("mask shape")` at 1356); parity test `lc-gpu/src/remaining_tests.rs:364`. A new mask shape without a kernel therefore still works via the CPU fallback.
- No cap on layer count (grep for MAX_MASKS found none). C1 caps at 16.

### 2.3 Masks UI today (the "separate panel")
- Lives in `RightPanel::Masking` (`lc-ui-egui/src/state.rs:44`, dispatch `panels/right.rs:33`, strip icon `panels/strip.rs:49`, shortcut Shift+W `menus.rs:162`). File `panels/masking.rs` (1008 lines): "Create New Mask" tile grid of 11 kinds (`masking.rs:70-120`), mask list with eye/rename/context menu (`masking.rs:171-262`), component list with + / - and Add/Subtract/Intersect/Invert/Delete (`masking.rs:270-354`), brush settings, Opacity / Amount / Refine Edges sliders (`masking.rs:360-410`), "Show only tools this layer uses" + Reset Layer (`411-432`), the quick "Local Adjustments" section (`local_section`, 449), then **the full tool sections re-used** via `edit::tool_sections(..., Target::Layer(m.id), used_only)` (`masking.rs:429-440`). So the engine and the widgets are shared; only the *placement* is separate.
- `edit.rs:49-90` `enum Target { Global, Layer(u32) }` routes every control: Global -> `develop.set`, Layer -> `mask.setTools` (`edit.rs:56-69`). `tool_sections` (`edit.rs:110`) draws Light(+Curve), Color(WB, vibrance/sat, mixer, Point Color, Skin Tone, grading), Effects(+vignette, grain), Detail(+NR) for either target. The Edit panel itself always passes `Target::Global` (`edit.rs:529`).
- Mask overlay and on-canvas handles are hard-wired to the Masking panel: overlay only when `app.ui.right == RightPanel::Masking` (`panels/detail.rs:726`), handles `detail.rs:380`, tool is cleared when leaving the panel (`menus.rs:266`), Delete key acts on the active mask only in that panel (`shortcuts.rs:258`). Overlay toggle is LR's `O` (`menus.rs:140`); modes/colour/opacity already exist (`masking.rs overlay_options`, `state.rs:302-305`).

### 2.4 Undo / history / persistence
- Every mask edit is `masks_edit` -> `Session::set_develop(id, settings, label)` -> catalog `Op::SetDevelop` (`cmd/masks.rs:140`, `lc-engine/src/lib.rs:592`), one undo step, label shown in History; slider drags/brush strokes group via begin/endInteraction (`lib.rs:107`). Layer selection is not undoable (same as C1).
- Persistence: complete settings, masks and layer tools included, are JSON in the catalog and in the XMP sidecar as `lc:settings` (`lc-engine/src/sidecar.rs:6`, struct field at 113). Foreign `crs:` blocks in an existing sidecar are preserved byte for byte (`sidecar.rs:9-13`).
- Presets/copy-paste: `SettingsGroup::Masks` ("Masking") carries `masks` (incl. layer tools); LR-style copy excludes it by default (`lc-develop/src/presets.rs:31,64-68,116`). AI mask caches are stripped when copying to other photos (`presets.rs:144-150`).

### 2.5 Lightroom XMP compatibility
- **Import only.** `lc-engine/src/crs_masks.rs` maps `crs:MaskGroupBasedCorrections`, `GradientBased...`, `CircularGradientBased...`, `PaintBasedCorrections` into `Mask { components, adjust }` (`crs_masks.rs:184-224`): Local* fields (Exposure2012 x4 EV, Contrast, Highlights, Shadows, Whites, Blacks, Clarity, Texture, Dehaze, Temperature, Tint, Saturation, Hue, Sharpness, LuminanceNoise, Moire, Defringe, Toning hue/sat) -> `adjust`; components Gradient, CircularGradient, Paint, RangeMask (lum/colour/depth), Image (subject/sky/background...). Unsupported components are reported, not guessed (`crs.rs:367-393`). **No LR-imported mask ever has `tools`**; they live entirely in `adjust`.
- **No export.** The app never writes `crs:` develop fields or mask groups (searched: no writer of `crs:Exposure2012` or `CorrectionMasks`); round trip is `lc:settings` JSON. Consequence: moving layer data to `tools` (or adding new layer kinds) cannot break an LR export because none exists; it only has to keep LR **import** and the `adjust` render byte-identical (golden hashes `lc-pipeline/src/tests_layers.rs`).
- LR can only represent what `Local*` fields cover; layer curves, Point Color, skin tone, grading, vignette, grain, B&W have no `crs:` equivalent per mask in the interchange we implement (LR 15 added some per-mask tools, not implemented here).

### 2.6 Curves: how drawn and why hard to see
`panels/edit.rs:1014 curve_editor` (egui painter, single dark theme: `theme.rs:70` canvas #1c1c1c, panel chrome #2d2d2d):
- Plot background `t.canvas` (#1c1c1c) on chrome (#2d2d2d), **no border**, corner radius 2 (`edit.rs:1071`) - the plot barely separates from the panel.
- Grid `Color32::from_gray(52)` 1 px (1076/1080) and the identity diagonal `from_gray(70)` 1 px (1083): hard-coded greys, not theme tokens, both nearly the background.
- Curve line 2 px in `from_gray(225)` for master / #dd3333 / #33bb55 / #3377ee for channels (`1134-1141`); parametric 2 px `from_gray(220)` (1107). A fresh/identity curve lies **exactly on** the diagonal so nothing visible changes when you start; no under-stroke/halo, no dashed reference.
- Point handles: 4 px filled circles in the same colour as the line, 5.5 px on hover/drag (`1155-1161`), hit radius 10 px; no outline on resting handles, end points not distinguished.
- No histogram behind the curve (Lightroom/C1 both have one), no other channels shown while editing one channel, no grid labels. Plot side is `panel width - 48` (`1064`): ~222 px at the default 270 px panel (`state.rs:240`), so the 2 px line is a hairline at that size.
- Parametric: the drawn line is a hand-rolled approximation (`1090-1106`), not the pipeline's `CurveTables`; there are no draggable split markers although `split_shadows/mid/highlights` exist.
- Layer curves reuse the same widget; the tone-curve flyout for a layer is hidden inside the Masking panel.
(I could not screenshot the running app; the above is from the code. The headless harness `lc-ui-egui/src/headless.rs` can raster the panel for a before/after check.)

### 2.7 WB / skin-tone / point-colour pickers
- WB: row in the Color section (`edit.rs:166-190`): `tool_button("wbPicker", Icon::Picker, 28x28)` right-aligned next to the WB dropdown; named icon map `wbPicker -> "pipette"` (`crates/pc-ui-egui/src/color_icon_data.rs:133`; the SVG set is shared into lc-ui-egui by `#[path]` includes at `lc-ui-egui/src/lib.rs:9-26`), fallback `Icon::Picker` (`icons.rs:424`, a plain line + diamond). Same "pipette" is also used for Film Base (`negDmin`). **In a layer the WB section has no picker at all** (`edit.rs:191`).
- Skin Tone: a `flyout_row(..., Icon::Picker)` (`edit.rs:243`) - the *same* icon as Point Color (`edit.rs:233`) and Defringe (`edit.rs:568`) - and, inside, a **text-only** button "Pick Skin Reference" (`edit.rs:248`, `text_button`, no icon). No entry for skin in `DEVELOP_TOOL_ICONS` (`color_icon_data.rs:102-138`). Cursor on canvas is the generic Crosshair (`panels/detail.rs:816-832`).
- The skin tool shows **no sampled tone**: `SkinTone { reference: Option<[J,C,h]> (UCS 22 JCH), uniformity, lightness, hue_range, chroma_range, lightness_range, protect_lips }` (`settings.rs:682`), but the UI is five sliders (disabled until a reference exists, silently - `edit.rs:254-257`) and a "Protect Lips" checkbox. No swatch, no range display, no "view selected range" overlay. Contrast: Point Color shows a swatch strip per sample (`edit.rs:1466-1480`, `oklch_color`) and a "Visualize range" overlay (`edit.rs:1500`, `detail.rs:754`, `Overlay::PointColorRange`).

## Part 3 - Design: adopt the Capture One model in Develop

### 3.0 Summary of the proposal
Do NOT rebuild the data model or pipeline. Finish the job at the UI/engine seam:
1. **Edit panel becomes the one place to edit** - Background or any layer. A *layer bar* at the top of the Edit panel selects the target; the existing `tool_sections` already take `Target::Layer(id)`, so the Edit panel simply passes the selected layer.
2. Add the missing "full layer" (and "empty layer") creation, WB picker/curve presets for layers, and decouple overlay and on-canvas handles from the Masking panel.
3. Demote the Masking panel to a *mask editor* (shapes, brush settings, refine) reachable from the layer row, then fold it into the layer row in phase 2.

### 3.1 Data model (changes are small and additive)
Current -> keep: `Mask` = layer (`components` = mask stack, `opacity`, `tools` = sparse develop subset, `refine`, `invert`, `visible`, `name`).
Changes:
- **`MaskShape::Full`** (serde `{"kind":"full"}`): alpha = 1 everywhere. "New Filled Layer" = a layer with a single `Full` component, which can then be refined with Subtract/Intersect components (brush erase, gradient subtract, luma range intersect) - exactly C1's "fill then erase". Edit `lc-develop/src/settings.rs` (enum at ~1028), `lc-pipeline/src/masks.rs:120 shape_alpha` (fill 1.0), `lc-gpu` needs nothing in phase 1 (falls to the CPU mask-shape fallback, `render.rs:1356`; a trivial kernel in `wgsl/mask.wgsl` later), `lc-ui-egui icons::mask_name` (`icons.rs:526`), `masking.rs kind_label` (46), `crs_masks` untouched. Compatibility: older builds cannot parse a `full` component (unknown tag); acceptable (pre-release, and `Full` appears only when used; existing settings and golden hashes unchanged).
- **"New Empty Layer"**: a `Mask` with `components: []` is already legal; but today it is skipped and shown with no hint. Show it in the bar as "(no mask)" and have the pipeline continue to ignore it (matches C1: no effect until a mask is drawn). No schema change.
- **Layer cap**: soft-cap at 16 like C1 (warn in the "+" menu; do not reject on load).
- **Legacy `adjust` sliders**: keep as is for render-identity. Surface them as a collapsed group "Quick sliders (Lightroom-style)" at the top of the layer view, auto-expanded when any is non-zero (LR-imported layers have *only* these - without this group they would show neutral sliders over an active effect, the biggest UX trap of the change). Offer an explicit, opt-in **"Convert to layer tools"** for exposure/contrast/highlights/shadows/whites/blacks/clarity/texture/dehaze/saturation (one-time; renders change slightly because `adjust` is summed in log-domain while tools blend per stage - DEVELOP-DESIGN.md 3.2). Not automatic.
- Optional later: `Mask.kind: Adjust | Heal | Clone | Remove` (DEVELOP-DESIGN.md 3.2 sketch). Spots already reference a layer via `Spot.mask` (`settings.rs:1430`), so heal/clone layers can be phase 3.

### 3.2 Which tools are layer-aware
Layer-aware (already in `LAYER_KEYS`; target "all local-sensible tools"): White Balance, Exposure/Contrast/Highlights/Shadows/Whites/Blacks, Tone Curve (parametric + point), Vibrance/Saturation, Color Mixer / B&W Mixer, Point Color, **Skin Tone**, Color Grading, B&W treatment, Texture/Clarity/Dehaze/Structure, Vignette, Grain, Sharpening, Noise Reduction. (C1 itself excludes Grain/Vignette from layers; we allow them - keep.)
Global-only (Background only) - show the section header greyed with an inline reason line, not hidden:
| Section | Reason shown |
|---|---|
| Profile / Look / Process, Negative | "Sets how the photo is rendered - applies to the whole photo. Select Background to change." |
| Tone Equalizer (`develop_tools::tone_eq_section`) | "Works on the whole photo's luminance zones." (candidate for per-layer later) |
| Raw / demosaic, Optics (lens, CA, defringe, distortion), Colour calibration, Calibration | "Camera and lens corrections apply to the whole photo." |
| Crop / geometry / Upright, Spots, Red Eye, AI Enhance/Denoise | They are their own panels; unaffected by the selected layer. |
Implementation: in `edit::show` (`edit.rs:426-600`) each of the global-only blocks is wrapped by a helper `global_only(app, ui, target, "reason", |...|)` that, when `active_mask.is_some()`, draws the section header disabled + one dim line, and a link "Select Background" which clears `active_mask`. Matches C1's behaviour (non-layer tools stay global) but explains it instead of silently editing the background.

### 3.3 UI placement (Edit panel, no separate panel)
```
Edit                                  [Auto] [B&W] [Reset]
[histogram]
LAYERS                                          [+ v]  [eye mask-overlay (O)]
 o Background
 o Layer 1  [thumb] Sky -0.7 EV            100%  eye  ...
 > Layer 2  [thumb] Face              [opacity slider 80]  eye ...   <- selected, tinted
   Mask: Brush · Subject · - Luma   [Edit mask...]   (collapsed row; phase 2 inline)
------------------------------------------------
Editing: Layer 2  (brush badge)    [Reset layer] [Only tools this layer uses]
Light  (dot = layer sets it)
Color ...   Effects ...   Detail ...
Profile / Optics / Calibration ... (greyed with reason when a layer is selected)
```
- **Layer bar** (new `panels/layer_bar.rs`, called from `edit::show` after the histogram): Background row first, then layers; row = select (click), eye (visible), thumbnail of the alpha (phase 2), name (double-click rename - reuse `renaming_mask`), opacity % (drag/scrub), kebab (reuse `mask_menu` from `masking.rs:480`: duplicate, duplicate+invert, invert, move, delete). Compact when there are no layers: a single "Background  [+]" row.
- **"+" menu** (replaces the Masking tile grid): New Filled Layer (Full), New Empty Layer, then masks: Brush, Linear, Radial, Subject, Sky, Background, Object, Describe, Depth Range, Luminance Range, Colour Range - these call the same `mask.add` with the same `StripTool` tooltips (`tool_tips.rs:59-69`). New layer becomes selected (`mask.add` already sets `active_mask`).
- **Per-tool brush badge** (C1's brush icon next to the tool name): when a layer is selected every `tool_section` header shows a small tinted layer chip "Layer 2" (the layer-mode `tool_section` already draws a dot when the layer sets the section, `edit.rs:348-417`).
- **Layer selector in every place tools are shown**: the Edit panel is one scrolling column (not tabs), so a sticky layer bar at the top satisfies C1's "layer drop-down at the left of the tool-tab bar" (https://support.captureone.com/hc/de/community/posts/360012113377-Layer-drop-down-box-next-to-tool-tabs). Also show the current layer name as a pill in the canvas top bar so it is visible when the Edit panel is scrolled (small `detail.rs` addition). Make the bar sticky with `egui::TopBottomPanel` inside the right panel or `ScrollArea` header (right.rs:25 wraps everything in one ScrollArea; render the bar above the ScrollArea for the Edit panel).
- **Mask overlay**: remove the `RightPanel::Masking` condition (`detail.rs:726`) -> show when `active_mask.is_some() && mask_overlay && (layer has components)`; add C1's three modes to the existing mode list: Always / Never / **Only while drawing** (default for brush) with `O` (LR, existing) and an optional `M` alias only if it does not collide with LR's M (Graduated Filter in LR Classic) - recommendation: **keep LR keys** (UX standard: LR/PS users hop in), ship a "Capture One keymap" preset in phase 3 (B/E/G/M).
- **On-canvas handles** (`detail.rs:380 mask_overlay`): enable when `RightPanel::Edit && active_mask.is_some()` as well as Masking. `menus.rs:266` must stop clearing the brush tool when the panel is Edit with a layer selected. `shortcuts.rs:258` Delete: when Edit panel and a layer is selected and a canvas handle/mask is focused -> delete component; keep "never delete the photo while editing a layer".
- **Masking panel**: phase 1 keep (renamed "Mask") but remove its duplicate tool sections (`masking.rs:429-440`) and Local Adjustments; keep creation tiles as a secondary entry; phase 2 delete the tiles/list (they live in the layer bar) and fold the component list / Add-Subtract-Intersect / Invert / Refine / brush options into the selected layer row; the strip icon then focuses the layer bar. Keeps `K` / Shift+W working as "show layer bar + mask options".

### 3.4 Pipeline cost
- Nothing to change structurally: per-tool stage blend (not N full pipelines) is already implemented on CPU and GPU. Cost model: 1 base render + per layer { 1 alpha plane (f32, n pixels), + per-tool stage work for the tools the layer sets; NR = +1 denoise pass per NR-layer; sharpen/clarity/texture = local-tone planes per layer }. GPU caches per `layer/{mi}` scope so a slider drag on layer K recomputes only layer K's stage work (`primary.rs:818`).
- Budget to enforce: <= 16 layers; interactive edit of one layer's non-NR tool should stay within the existing slider-drag budget (`lc-ui-egui/src/tests_slider_speed.rs` has the harness); NR/sharpen on a layer uses the draft path during drag.
- Memory risk: alpha planes are f32 per layer at render size (`render.rs:1228 cx.zeroed(n * list.len())`): 45 MP full-res export = 180 MB x 16 = 2.9 GB VRAM. Mitigations: skip planes for `Full` layers with no other components (use scalar alpha = opacity x amount, no buffer), tile export, or R16F planes (phase 2). `Full` layers are the common C1 case, so the scalar fast-path matters.
- `Full` fast path also saves CPU: `layers::resolve` can treat a layer whose only component is `Full` as constant alpha.

### 3.5 Backward compatibility and Lightroom
- Existing `masks` open unchanged: every existing mask appears as a layer; those with only `adjust` show the legacy sliders group (3.1). No settings migration, no version bump; golden hashes in `lc-pipeline/src/tests_layers.rs` stay valid because `Full` serialises only when used.
- LR **import** (`crs_masks.rs`) unchanged: LR correction -> layer with `adjust` + components; LR `CorrectionActive=false` skipped; name from `CorrectionName`. LR "Amount" -> `adjust.amount`. Mapping LR masks to layers is therefore already done; map LR *Subject/Sky/Background* `Mask/Image` subtypes as now.
- LR **export** does not exist; do not build it for phase 1. If wanted (phase 3): write `crs:MaskGroupBasedCorrections` for layers with only `adjust` + components that LR can represent (Gradient/CircularGradient/Paint/RangeMask), skip layers with `tools` and list them in a warning. Risk: LR rewriting crs: and dropping `lc:` data; we already preserve foreign crs bytes (`sidecar.rs:9-13`).
- PSD/Compositing: develop layers inside compositing documents carry `DevelopSettings` incl. masks (DEVELOP-DESIGN.md 3.4) - no change.

### 3.6 Undo
- Every layer operation is already one `set_develop` op with a label (add, rename, opacity, tool edits "Layer Adjustment"/control label, reset). Layer *selection* stays UI-only (like C1/LR). Slider/curve/brush drags group through begin/endInteraction (already used by the Target::Layer curve path, `edit.rs:1186,1221`).
- New: "Add Layer" label should read "New Filled Layer"/"New Empty Layer" (labels only). History panel entries unchanged.
- Switching photo already resets `active_mask` to None (`lc-engine/src/cmd/library.rs:163,407`); keep a test so the Edit panel never edits a stale layer id.

### 3.7 Phased plan

**Phase 1 - Linux release (about 5-7 working days)**
| # | Work | Files (owner = who edits) |
|---|---|---|
| 1a | `MaskShape::Full` + `mask.add {kind:"full"|"empty"}` + scalar-alpha fast path + tests | `crates/lc-develop/src/settings.rs`, `lc-pipeline/src/masks.rs`, `lc-pipeline/src/layers.rs`, `lc-engine/src/cmd/masks.rs` (engine owner) |
| 1b | Layer bar + "+" menu in Edit panel; Edit passes `Target::Layer(active_mask)` into `tool_sections`; global-only wrappers with reasons | new `lc-ui-egui/src/panels/layer_bar.rs`; `panels/edit.rs`, `panels/right.rs`, `state.rs` (UI owner) |
| 1c | Decouple overlay/handles/shortcuts from `RightPanel::Masking`; add "Only while drawing" mode | `panels/detail.rs:380,726`, `menus.rs:266,140`, `shortcuts.rs:258,291` (UI owner) |
| 1d | Layer WB picker (`develop.wbPick {mask?}`), layer curve presets (apply preset to layer curve), skin/WB icons & skin swatch (Part 4) | `lc-engine/src/cmd/develop.rs:333`, `cmd/curves.rs`, `panels/edit.rs` |
| 1e | Legacy quick-sliders group in layer view; strip duplicates from Masking panel | `panels/masking.rs`, `panels/edit.rs` |
| 1f | Part 4 readability fixes | see Part 4 |
Explicitly NOT in phase 1: thumbnails, drag reorder, panel removal, heal/clone layers, LR export, Tone-EQ per layer.

**Phase 2 (post-release 1)**: fold mask editing into the layer row and retire the Masking tiles; mask thumbnails (low-res alpha from the preview pipeline, cache per layer id + settings hash); copy/paste layer(s) between photos with C1-style Skip/Duplicate/Replace; "Convert legacy local adjustments to layer tools"; Luma-range/Colour-range intersect via right-click (C1 16.7 flow) and Combine-mask UI; GPU kernel for `Full` + R16F alpha; "Show Mask" per C1 modes; 16-layer cap with message.

**Phase 3**: Heal/Clone layer kinds, Tone EQ per layer, optional LR crs export, C1 keymap preset, rasterize parametric masks, layer styles/presets applied to a layer ("Apply to Background/Layer", C1 Styles).

### 3.8 Risks
1. **Legacy `adjust` vs `tools` duplication** (two exposure sliders per layer): mitigated by the grouped legacy block; do not auto-migrate (render change).
2. Edit panel is one long egui scroll column; adding a bar plus larger layer sections increases per-frame cost (`tests_slider_speed.rs` guards slider latency; keep layer bar cheap, no per-frame thumbnail work).
3. Stale `active_mask` across photo switch, Auto Sync (layer ids differ per photo) and batch sync: sync only copies masks wholesale via the Masking group; selected layer edits through `mask.setTools` are NOT auto-synced to other photos (`auto_sync_ops` diffs full JSON, so they are - verify with a test; layer ids are per photo so synced edits could target wrong layers).
4. GPU memory for many large layers (3.4); `Full`-only fast path.
5. Hotkey conflicts: C1 `M` vs LR `M` (Graduated Filter); keep LR keys.
6. `DevelopSettings` hashes: any serialization change breaks golden tests - `Full` only when used.
7. The shared icon assets live in `crates/pc-ui-egui/src/` (editor side) and are `#[path]`-included by lc-ui-egui; adding icons touches files outside the "lc-* only" scope - coordinate with the pc owner (one-line registry additions only).
8. Doc drift: `docs/DEVELOP-DESIGN.md` still says layer tools are CPU-only and lack WB picker; update after phase 1.

### 3.9 Test strategy
- Engine unit: `Full` layer alpha == 1; layer with Full + subtract brush; empty layer ignored; JSON round trip; settings without `Full` hash-identical (existing `tests_layers.rs` goldens).
- Pipeline/GPU parity: extend `lc-gpu/src/remaining_tests.rs:364` with a Full layer + opacity 50 (CPU vs GPU within existing tolerance); Full-layer exposure +1 at opacity 100 == global +1 exposure within epsilon on a flat image (proves the "toggle to a layer and edit as normal" equivalence).
- Engine commands: `mask.setTools` via Edit-panel-equivalent calls; `develop.wbPick {mask}` writes `tools.wb` not `wb`; undo/redo restores layer tools; active mask reset on photo change; sync does not retarget layers.
- UI headless (`lc-ui-egui/src/tests_masking.rs`, `headless.rs` `ui.clickWidget`): select layer in bar, drag Exposure slider, assert `masks[i].tools.light.exposure` changed and global unchanged; global-only sections disabled with reason when a layer is selected; "Select Background" clears; overlay visible on Edit panel with layer selected; update widget ids that move (`maskNew:*`, `mask:*`).
- Visual: headless raster snapshots of curve editor (identity and edited) with contrast assertion (curve stroke luminance vs plot background >= 4.5:1 and handle outline present); skin-tone swatch present after `skinTone.pick`.
- Manual (owner, 10 min): Filled layer -> exposure -1 -> erase sky with brush -> opacity 60 -> WB on layer -> Skin Tone on layer with pick; compare to C1 behaviour.

## Part 4 - Small readability fixes (independent of layers; can ship first)

All in `crates/lc-ui-egui/src/panels/edit.rs` unless noted. These are code-derived hypotheses (no screenshot taken); verify each with a headless raster.

### 4.1 Curves: make the lines visible (`edit.rs:1014-1245`)
1. Plot frame: fill `t.grid_bg` (#0f0f0f, darker than the panel) or keep `t.canvas`, and add a 1 px border `t.button_border` (#4c4c4c) around `r` (line 1071). Replace hard-coded `Color32::from_gray(52/70)` (1076, 1080, 1083) with theme tokens (grid = `t.field_border` #3c3c3c at 1 px; quarter lines stronger on the centre cross).
2. Identity reference: draw the diagonal dashed (`egui::Shape::dashed_line`) in `t.text_dim` at ~35% alpha so a curve sitting on identity is still seen as a distinct solid line.
3. Curve stroke: 2 px -> 2.5-3 px, with a 1 px dark under-stroke (`Stroke::new(4.5, canvas)` then the coloured 2.75 px) at lines 1107 and 1141; master colour `Color32::WHITE` (not gray 225); parametric uses `t.accent`-tinted white.
4. Handles: radius 4 -> 5 at rest with a 1.5 px dark outline (`p.circle_stroke(c, 5.0, Stroke::new(1.5, t.canvas))`) at 1160; end points drawn as hollow squares/rings so they read as fixed; hover 6.5 with white ring.
5. Context: draw the histogram faintly behind the grid (reuse data used by `histogram()` at `edit.rs:862`) and, when a single colour channel is active, draw the master and other channels' curves at 35% alpha.
6. Size: plot side = `max(panel_w - 48, 240)` (line 1064); consider default `RIGHT_WIDTH.default` 270 -> 300 (`state.rs:240`) for the whole Develop panel.
7. Parametric curve: sample the real curve (`lightcraft_pipeline::finish::CurveTables`) instead of the approximation (`1090-1106`), draw the three region dividers (`split_shadows/mid/highlights`) as thin vertical lines with small drag handles, and label regions (Shadows/Darks/Lights/Highlights) in `t.text_dim` 10 pt.
8. Channel buttons (`edit.rs:1024-1050`): selected state is only a 2 px underline; use a filled `t.tool_active` tile like other tool buttons.
9. Layers: when editing a layer the same widget is used - add the layer name in the curve header ("Curve - Layer 2").

### 4.2 Picker icons and labels
- White Balance picker (`edit.rs:178-189`): keeps `wbPicker -> pipette`; add a distinct `wb-picker` glyph (pipette + small half-grey/half-white disc) so it is not identical to Film Base's `negDmin -> pipette`. Show the same button in the **layer** branch (`edit.rs:191`) and wire `develop.wbPick` to accept `mask` (`lc-engine/src/cmd/develop.rs:333`) and write `tools.wb` of that layer.
- Skin Tone (`edit.rs:241-262`): replace the text button "Pick Skin Reference" with an icon button (new `skin-picker` glyph: pipette + face/skin-tone disc) plus a short label; add `("skinTone","skin-picker")` to `DEVELOP_TOOL_ICONS` (`crates/pc-ui-egui/src/color_icon_data.rs:102-138`) and the svg to `assets/icons/tools/` and `assets/icons/color/`, registered in `crates/pc-ui-egui/src/tool_icon_data.rs` (~line 57, 156) and `color_icon_data.rs` (~line 57). The mono and colour sets must both have it (test `mono_tool_icons::tests::every_tool_style_has_the_same_rendered_silhouette`).
- Flyout row tiles use the vector `Icon` enum, not the SVG set (`widgets.rs:396 flyout_row` -> `icons::paint`): add `Icon::SkinTone` (face circle with sampler) and `Icon::WhiteBalance` in `crates/lc-ui-egui/src/icons.rs` (next to `Picker`, line ~424). Currently Point Color (`edit.rs:233`), Skin Tone (243) and Defringe (568) all use `Icon::Picker`; give Defringe its own (`Icon::Fringe` or reuse `Sliders`).
- Cursor: replace the generic crosshair for `wbPicker`/`skinTone`/`pointColor` (`panels/detail.rs:783, 816, 836`) with a pipette cursor of the matching glyph, small badge for WB vs skin (egui custom cursor via `ctx.set_cursor_icon` is limited - paint a glyph at the pointer instead).
- Menu entry for the skin picker alongside "White Balance Selector (W)" (`menus.rs:178`).

### 4.3 Skin tone: show the tones you are working on (`edit.rs:241-262`, `detail.rs:816`, `lc-pipeline/src/visualize.rs`)
`SkinTone.reference` is `[J, C, h]` in UCS 22 JCH (`settings.rs:682`); `lc-pipeline/src/ucs.rs:192 jch_to_xyy` converts it, then XYZ -> sRGB (same path as `oklch_color`, `edit.rs:1439`).
1. **Reference swatch** (32x32 rounded) with the picked colour next to the picker, plus a text readout "J 62 · C 24 · h 48 deg" (dim) and a "Clear" button.
2. **Range strip**: a 7-step lightness strip over `reference.J +/- lightness_range`, a hue strip over `h +/- hue_range` at the reference chroma/lightness, and a chroma strip over `C +/- chroma_range` - rendered as small gradient bars under the sliders; they update live while the range sliders move (this is C1's wedge on the colour wheel, in a form that costs one painter loop). Optional: a 90 pt hue/chroma polar plot with the window as a wedge.
3. **View selected range**: a checkbox like Point Color's "Visualize range" (`edit.rs:1500`): new `Overlay::SkinToneRange` in `lc-pipeline/src/visualize.rs` (grey out everything outside the window, colour inside) wired in `detail.rs view_overlay` (754) and the GPU overlay path; mirrors C1's "View selected color range".
4. Disabled state: replace silent greyed sliders (`edit.rs:254-257`) with a line "Pick a skin tone on the photo to enable".
5. Same for the **layer** target: the `reference` of `tools.skin_tone` is already per layer (`cmd/color.rs:200`).

### 4.4 Other Develop-panel cleanup candidates (low risk)
- Flyout rows are 40 pt tall with identical grey tiles (`widgets.rs:396`): use per-tool glyphs (see 4.2) and a "set" dot (as layer sections already do, `edit.rs:348-417`) for flyouts that have non-default values.
- `numeric_slider` labels are `text_dim` 12-13 pt on `chrome`; check contrast >= 4.5:1 (`text_dim` #8e8e8e on #2d2d2d computes to about 4.2:1, just under AA for small text): raise to `text_label` (#bcbcbc, about 7.3:1) for labels, keep dim for units.
- Sub-titles (`edit.rs:829 sub_title`) between groups are low weight; use `t.semibold(12)` + 1 px divider.

## Appendix - questions for the owner (two minutes in Capture One)
1. Is the layer selector you want a drop-down in the tool area (C1 16.7 shows the Layers & Masks tool plus a per-tool brush badge) or the list always visible? The spec gives both (sticky bar with list; compact when no layers).
2. In your C1, which tools are greyed when a layer is selected (Grain, Vignetting, B&W? Base Characteristics, Lens)? We allow Grain/Vignette/B&W on layers; tell us if you would rather block them to match.
3. Keep Lightroom's `O` for the mask overlay and `K/M/Shift+M` tool keys (spec default) or add the C1 keymap preset (B/E/G/M) in phase 3?

## Owner decisions (2026-10-13)
1. **Layer list:** collapsible. Show the full list when the photo has layers, and a compact bar when it doesn't (as proposed).
2. **Vignetting, Grain and B&W stay whole-image (global-only) for now.** With a layer selected they are greyed out, with the stated reason and a "Select Background" link. The engine's per-layer support for them stays unexposed.
3. **Keyboard profiles:** ship both a Lightroom profile (default: `O` overlay, `K`/`M`/`Shift+M`) and a Capture One profile (`B`/`E`/`G`/`M`, etc.). Users can also create and save their own Custom profiles: duplicate any profile, edit it, then name, rename and delete it.
   - Build on the existing shared shortcut editor (`Edit › Keyboard Shortcuts`, 3207abd2) rather than a new system.
   - The profile choice lives in Preferences and in the shortcut editor.
   - Conflicts are shown inline.
   - Profiles are stored with the user's preferences, and import/export as JSON is optional.
   - Profile switching applies to both Library/Develop and the editor where a key exists in both.
   - This moves the C1 keymap from phase 3 into phase 1.
4. **Adaptive, per-workspace shortcut sets (owner, 2026-10-13):** each workspace picks its own profile independently, and the active set follows whichever window or module has focus.
   - **Library/Develop:** choose Lightroom (default), Capture One, or Custom.
   - **Editor:** choose Photoshop (default), Affinity Photo if feasible, or Custom.
   - **Global:** app-wide keys that apply everywhere (quit, preferences, module/workspace switching, undo/redo routing). These are shown separately so a profile can't silently break them.
   - **Same key, different jobs:** the same key may do different things in Develop and the Editor (e.g. `B` = brush layer in Develop, Brush tool in the Editor) without being a conflict. Conflicts are only reported within one workspace, or against a Global key.
   - **Editing:** the shortcut editor gets a workspace selector (Global / Library / Develop / Editor) and the profile drop-down per workspace. Custom profiles can be saved per workspace, e.g. a "My C1 Develop" profile next to a "My PS Editor" profile.
   - **Where it's stored:** the profile choice per workspace lives in Preferences. Existing user overrides migrate into a Custom profile for the workspace they belong to, so nobody loses customised keys.

## Review incorporated (2026-10-13)
`docs/wip/REVIEW-develop-layers-fable.md` was reviewed and accepted. Its §6 "Revised phase-1 task list" (C1–C3, A1–A6, B1–B3, X1) and its §5 acceptance walkthroughs A–H are now the build plan and supersede the earlier phase-1 list.
- **Keyboard profiles use three scopes:** Global / Library+Develop / Editor. Library and Develop already share one scope.
- **Profiles are their own stream (B), with a prefs migration.**
- **The Auto Sync mask bug is fixed first:** with Auto Sync on, `json_delta` replaces the whole `masks` array on every other selected photo. It moves to the immediate follow-up batch because it's live data loss today.
