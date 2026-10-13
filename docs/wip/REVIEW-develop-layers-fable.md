# Review: SPEC-develop-layers-c1 (Fable, 2026-10-13)

Quick, focused design review. Read-only. Written incrementally.

Spot-checked at `wip/up-raw` 6024e4c7. Files read: `lc-develop/src/settings.rs`, `lc-pipeline/src/{lib,layers,masks}.rs`, `lc-ui-egui/src/panels/{edit,masking,detail}.rs`, `lc-ui-egui/src/{menus,shortcuts,shortcuts_catalog,state}.rs`, `lc-engine/src/{lib,sidecar}.rs`, `lc-engine/src/cmd/{develop,masks,curves,library}.rs`, `lc-develop/src/presets.rs`, `lc-gpu/src/{render,primary,remaining_tests}.rs`, `pc-ui-egui/src/prefs_ui.rs`, `pc-engine/src/prefs.rs`, commit 3207abd2.

## 1. Verdict

**Layers (Parts 2-3): right-sized and correctly sequenced.** The spec's central claim is true: `Mask` already is a C1 layer (`settings.rs:1131-1153`), `LayerTools` holds 14 sparse tool sections (`settings.rs:1190-1237`), the pipeline blends per stage on CPU and GPU (`layers.rs:1-26`, `lib.rs:495 layers_need_cpu == false`), and `edit::tool_sections` already takes `Target::Layer(id)` (`edit.rs:110`, called from `masking.rs:437`). Phase 1 is genuinely a UI-seam change plus `MaskShape::Full`. Ordering 1a -> 1b -> 1c -> 1d/1e is right; 1f (readability) is independent and should ship first.

**Keyboard profiles (owner decisions 3-4): NOT right-sized as written.** The "existing shared shortcut editor" is a single flat override map (`pc-engine/src/prefs.rs:828-830` `shortcuts: BTreeMap<String,String>`) with exactly two scopes derived from a `library::` key prefix (`prefs_ui.rs:921-923`, scope filter at `prefs_ui.rs:1480`). There is no profile concept, no Global scope, and no Library-vs-Develop distinction (both are `library::`). Profiles are a separate feature with a prefs-schema migration and dispatch changes in two crates. Keep it in phase 1 as the owner asked, but run it as its own stream (B) with its own builder, not folded into the 5-7 day layers estimate. Realistic total: layers 5-7 days + profiles 4-6 days + readability 2 days.

One correction to decision 4: Library and Develop share one command catalog and one scope today (`shortcuts_catalog.rs:5 PREFIX = "library::"`), which already matches "Library/Develop picks one profile". A separate "Library" vs "Develop" selector would require tagging every `library::` command by module. Recommend **three scopes: Global / Library+Develop / Editor**, not four.

## 2. Factual errors (with evidence)

| # | Spec says | Code says | Severity |
|---|---|---|---|
| E1 | 3.8 risk 3: layer edits "are NOT auto-synced ... (so they are - verify)" (self-contradictory) | **They are, destructively.** `auto_sync_ops` diffs full JSON (`lc-engine/src/lib.rs:609-619`); `json_delta` treats arrays atomically: `(a, b) if a == b => None, (_, b) => Some(b.clone())` (`lib.rs:793-794`). So one slider drag on Layer 2 with Auto Sync on replaces the **entire `masks` array** of every other selected photo (`strip_photo_bound`, `presets.rs:145-159`, only strips `seg`/`detail` caches). Today this is reachable from the Masking panel; the layer bar makes it a daily path. Must be fixed in phase 1 (see G2). | High |
| E2 | 3.1: older builds can't parse `{"kind":"full"}`, "acceptable" | True but understated: `MaskShape` is `#[serde(tag="kind")]` (`settings.rs:1026`); an unknown variant fails the whole `DevelopSettings` parse, so an older build (the owner has test builds and a macOS build on hold) opens the photo with **default settings**, not just a missing layer. Also the sidecar carries the same JSON (`sidecar.rs:6`). Accept, but add a release note and a one-line engine test proving a catalog with `full` still loads on the new build after a downgrade/upgrade cycle. | Medium |
| E3 | 1d: "layer curve presets (apply preset to layer curve)" is listed as a small UI task | `curve.applyPreset` (`cmd/curves.rs:73,238`) has no `mask` parameter; `curve_footer` returns early for layers for that reason (`edit.rs:1256-1266`). Needs an engine change (same shape as `develop.wbPick {mask}`), so it belongs with 1d's engine half. | Low |
| E4 | 2.2 `layers::resolve` at `layers.rs:93`; 2.3 `mask_menu` at `masking.rs:480`, `kind_label` at 46 | `resolve` is at `layers.rs:100`; `mask_menu` at `masking.rs:470`; `kind_label` at 44. | Trivial |
| E5 | Owner decision 3: "Build on the existing shared shortcut editor ... rather than a new system" is presented as a cost saver | The editor UI can be reused (`prefs_ui.rs:1344 shortcuts_body`, conflict check `same_chord_in_scope` 903), but the **storage** is one flat map and the engine validator (`prefs.rs:1109-1146`) knows only `shortcuts.<id>`. Profiles need a new prefs shape (`shortcut_profiles: {scope: {active, profiles: {name: overrides}}}`) and a migration. Not an error of fact, but the spec's phase-1 cost omits it. | Medium |
| E6 | 3.3 "`K` / Shift+W" keep working | `panel.masking` is `Shift+W` (`menus.rs:162`); I found no `K` binding for masking in `menus.rs`. Verify before promising it. | Trivial |

Everything else I checked holds: `wbPick` has no `mask` param (`develop.rs:333-353`); WB row for layers is a bare `sub_title` (`edit.rs:190-192`); skin picker is a text button with `Icon::Picker` shared by Point Color and Defringe (`edit.rs:233,243,248,568`); curve greys hard-coded (`edit.rs:1076,1080,1083,1107,1138`), plot side `w-48` (`1064`), rest handles 4 px with no outline (`1159-1160`); overlay gated on `RightPanel::Masking` (`detail.rs:380,726`); tool cleared on panel leave (`menus.rs:266`); Delete scoped (`shortcuts.rs:253`); GPU alpha `n * list.len()` f32 (`render.rs:1229`), cache scope `layer/{mi}` (`primary.rs:818`); no crs develop writer (all `Exposure2012` hits are test fixtures); DEVELOP-DESIGN.md:87 "Not yet: a WGSL port" is stale.

## 3. Gaps and risks (each with a fix)

### Layer and mask UX a Capture One user expects
- **G1 Naming/duplicate/reorder/eye/opacity/invert: present** (`mask.rename/duplicate/move/visible/invert/setOpacity`, `masking.rs:470-486`, `360-372`). Gap: reorder is menu-only ("Move Up/Down"). Fine for phase 1; make Alt+Up/Down (profile key) move the selected layer so it is at least fast. Also auto-name by mask kind on creation ("Sky 1", "Brush 2"), not "Layer N" only; C1 users rename, LR users rely on auto-names.
- **G2 Sync / Copy-Apply (E1):** phase 1 must (a) drop `masks` from the Auto Sync delta unless the change *is* a mask structure change the user asked to sync, and (b) in `develop.sync`/`develop.copy` keep the existing "Masking" group checkbox but copy layers by **name**, with C1's Skip / Add as duplicate / Replace prompt deferred to phase 2. Minimal phase-1 fix: in `auto_sync_ops` remove `masks` from `delta` (`lib.rs:620-623`, next to `spots`/`red_eye`) and add a test "layer slider with auto-sync does not touch other photos' masks". Add a note in the Sync dialog: "Layers are copied whole".
- **G3 Merge/flatten:** C1 has no adjustment-layer merge either; skip. Do not add.
- **G4 Feather / Refine:** per-layer `refine` (guided edge snap) exists (`settings.rs:1145`); there is no per-layer **feather** (blur radius), which C1 has as "Feather Mask" and LR as brush feather. Phase 2: add `Mask.feather: f64` (serde skip when 0, golden-safe) applied as a blur of the summed alpha. Phase 1: keep per-shape feather (radial/brush) and label the existing slider "Refine Edges" not "Refine".
- **G5 Luma range:** `LuminanceRange` + `Intersect` op exist (`settings.rs:1055-1060`, `MaskOp` 998). Phase 1 only needs the "+" menu inside the selected layer row to offer "Intersect with Luminance Range" (one `mask.addComponent {op:"intersect"}` call). Cheap; add to 1b.
- **G6 Presets/styles to a layer:** `mask.setTools` already accepts a partial settings object filtered to `LAYER_KEYS` (`settings.rs:1341-1346`). "Apply preset to selected layer" is therefore: load preset JSON, keep only `LAYER_KEYS` (minus vignette/grain/treatment/bw_mix per decision 2), call `mask.setTools`. ~1 day. Promote from phase 3 to phase 2; add a context-menu entry on presets "Apply to Layer N" when a layer is selected.
- **G7 Before/after per layer:** the eye toggle is it (C1 same). Add Alt-click on the eye = "solo this layer" only if cheap (UI-only: temporarily set other layers invisible via one `mask.visible` batch; undoable as one step). Phase 2.
- **G8 Global-only tools (decision 2):** the engine still accepts `vignette/grain/treatment/bw_mix` via `mask.setTools`; `used_only` and the "dot" would still show them for layers that already have them (e.g. from earlier builds). Phase 1: `tool_sections` must skip those four keys for `Target::Layer` even when set, and `mask.resetTools` for such a layer should clear them. Add a test.
- **G9 Empty layer trap:** an empty layer renders nothing and the spec shows "(no mask)". Also show the "+" mask menu inline in that row and auto-open it on creation, else LR users (who never see empty layers) will think the tool is broken. Brush stroke with no mask on the selected empty layer should add a Brush component (C1 behaviour), not create "Layer N+1".

### History and undo granularity
- **G10** Each mask op is one `set_develop` step and drags group through `begin/end_interaction` (`lib.rs:652-672`). Good. Gaps: (a) labels are generic ("Layer Adjustment", "Edit Mask"); make them "<Control> - <Layer name>" so History reads like C1's; (b) `active_mask` is not in undo, so **undoing "Add Layer" leaves `active_mask` pointing at a deleted id**. Fix: after every undo/redo and on every frame, the layer bar resolves `active_mask` against `masks` and falls back to Background. Add the test the spec already proposes, plus one for undo-after-add.

### Performance, 45 MP, CPU and GPU
- **G11** GPU allocates one f32 plane per evaluated layer at render size (`render.rs:1229`): 45 MP x 4 B = 180 MB/layer; 16 layers = 2.9 GB plus NR planes. The `Full`-only scalar fast path (1a) is the phase-1 mitigation and must be real on both paths (`layers::resolve` constant alpha; `render.rs` skip plane). Also: Filled layer + Subtract brush is the common C1 case and does **not** hit the fast path. Phase 2: R16F planes (halves it) and export tiling. Phase 1 budget: measure `tests_slider_speed.rs` with 1, 4 and 8 brush layers on a 45 MP raw (the owner's RTX 5090 is not the floor; use the draft path during drag for NR/sharpen layers as the spec says).
- **G12** Each NR layer is a full extra denoise pass (`layers.rs:12`). Show a small "slow" hint in the Detail header when more than two layers set NR.
- **G13** Thumbnails are correctly deferred; keep the layer bar free of per-frame pipeline work.

### Lightroom XMP import / round-trip
- **G14** Import is unchanged and LR masks land in `adjust`; the legacy "Quick sliders" group (3.1) is the right answer. Add to 1e: the group header must say "Lightroom local adjustments" (the user will recognise it) and the "Convert to layer tools" button stays off phase 1.
- **G15 Round-trip trap:** an existing `crs:` block is preserved byte for byte (`sidecar.rs:8-12`), so after importing an LR photo and editing layers here, the sidecar still carries LR's **old** masks; LR will show the old edits and, on re-save, may rewrite the sidecar and drop `lc:`. Phase 1: none to build, but document it in the import dialog ("Lightroom will keep its own copy of these edits"). Phase 3: crs export for `adjust`-only layers as the spec says.

### Accessibility and narrow panels
- **G16** `RIGHT_WIDTH.min = 250` (`state.rs:240`) gives a 202 px curve plot and a layer row that must hold name + % + eye + kebab. Rule for 1b: name truncates with an ellipsis and full name on hover; opacity shows as "80%" text with scrub, the slider only in the selected row. No UI text-scale preference exists in lc-ui-egui (only `pixels_per_point` from the OS, `lib.rs:669`), so test at 1.0 and 1.5 ppp in the headless raster.
- **G17** Keyboard: layer bar must be reachable (Up/Down selects layer when the bar has focus; Enter renames; Delete removes with confirm). Colour-only "set" dots need a tooltip ("Layer 2 sets Light"). The overlay colour is already configurable (`state.rs:304`), good for colour-blind users; default red on skin should remain overridable.
- **G18** The readability numbers in 4.4 (text_dim 4.2:1) are plausible but unverified; have the headless raster assert >= 4.5:1 for labels before and after.

### Keyboard-profile conflicts and migration
- **G19 Data model (E5):** add to `Prefs`: `shortcut_profiles: { "library": {active: "Lightroom", custom: {name -> BTreeMap<id,chord>}}, "editor": {...} }`, with built-in profiles (`Lightroom`, `Capture One`, `Photoshop`, `Affinity`) as code tables, not prefs. Keep the old `shortcuts` map: on first run, if non-empty, split it by the `library::` prefix into "Custom (migrated)" per scope, select those as active, and leave the old map in place (downgrade-safe). The engine validator (`prefs.rs:1109`) gains `shortcut_profiles.*` paths.
- **G20 Conflicts:** today's rule "conflict only within scope" (`same_chord_in_scope`) already matches decision 4. Add the **Global** list as a fixed, non-overridable set in phase 1 (quit, prefs, undo/redo, module switch, fullscreen); a profile chord equal to a Global chord is shown inline as a conflict and ignored at dispatch.
- **G21 C1 profile collisions inside Library+Develop:** C1's `B/E/G/M` collide with LR Library keys this app ships (`B` target collection, `E` loupe, `G` grid - verify in `menus.rs:ui_commands`). Since Library and Develop are one scope, the C1 profile must either (a) remap those Library commands too or (b) make `B/E/G` context-sensitive (Develop with Edit panel focused). Recommend (b) via the existing `panel`/`tool` checks in `shortcuts.rs` (same pattern as `shortcuts.rs:291`), and document "when the Edit panel is active".
- **G22 Same key, different jobs across windows** is already the case (editor ignores `library::` ids, `shortcut_dispatch.rs:68,86`); only the active-profile lookup changes. Tests exist to extend: `shared_library_shortcut_tests.rs`, `shortcut_capture_tests.rs`.
- **G23 Affinity profile:** "if feasible" - ship it as a table with gaps marked, not as a promise; most Affinity keys equal Photoshop's.

## 4. Enhancements, ranked (value / cost)

| Rank | Item | Phase | Why |
|---|---|---|---|
| 1 | Auto Sync must not replace other photos' layers (G2) | 1 | Data loss; one-line fix plus test |
| 2 | Readability pack (Part 4.1-4.3: curve contrast, picker icons, skin swatch + range strips) | 1, ship first | Every Develop user, independent of layers, low risk |
| 3 | Layer bar in Edit panel + Filled/Empty layer + overlay decoupling (1a-1c) | 1 | The feature itself |
| 4 | Layer WB picker and layer curve presets (1d, engine `mask` params) | 1 | C1 parity for the two tools users reach for first on a layer |
| 5 | Keyboard profiles with three scopes, Global fixed list, migration (G19-G21) | 1, own stream | Owner decision; L/C1/PS users feel at home |
| 6 | Undo-safe `active_mask` + layer-named history labels (G10) | 1 | Cheap, avoids a visible bug on day one |
| 7 | "Intersect with Luminance Range" in the layer "+" menu (G5) | 1 | One command call; signature C1 workflow |
| 8 | Apply preset to selected layer via `mask.setTools` (G6) | 2 (early) | ~1 day, high value for C1 Styles users |
| 9 | Copy layers to other photos with Skip / Duplicate / Replace, by name (G2b) | 2 | Needs a dialog and name matching |
| 10 | Per-layer feather (G4), mask thumbnails, R16F alpha, export tiling (G11) | 2 | Model/pipeline work, measurable before building |
| 11 | Solo layer on Alt-click eye (G7), drag reorder, Affinity table completion | 2-3 | Nice-to-have |
| 12 | crs export for `adjust`-only layers (G15), heal/clone layer kinds, Tone EQ per layer | 3 | As the spec says |

## 5. Acceptance checklist (scripted walkthroughs a builder must pass)

Each walkthrough is run on a 45 MP raw on the owner's machine; "assert" items are also headless tests where marked (H).

**A. Filled layer basics (H)**
1. Open photo, Edit panel. Layer bar shows compact "Background [+]".
2. "+" > New Filled Layer. Assert: layer "Layer 1" selected, bar expands, `masks[0].components == [Full]`, overlay visible (full red), tool headers show the layer chip.
3. Drag Exposure to +1.0. Assert: `masks[0].tools.light.exposure == 1.0`, global `light.exposure` unchanged, render within epsilon of global +1 on a flat image.
4. Select Background. Assert: Exposure slider reads 0, Vignette/Grain/B&W enabled.
5. Select Layer 1. Assert: Vignette, Grain, B&W headers greyed with reason and a working "Select Background" link.

**B. Mask refine, invert, opacity, undo (H for 6-7)**
1. On Layer 1 (Filled, exposure +1): layer row "+" > Subtract > Radial; draw a radial on canvas. Assert: components = [Full add, Radial subtract]; overlay updates without visiting the Masking panel.
2. Kebab > Invert. Assert: `invert == true`, overlay inverted.
3. Scrub opacity to 50. Assert: `opacity == 50`, render halfway between Background and 100%.
4. Rename via double-click to "Sky". Enter. Assert: name shown in bar, in tool-header chip, and in the canvas pill.
5. Eye off / on. Assert: effect disappears/returns; one history step each.
6. Undo three times. Assert: opacity 100, invert false, radial removed, in that order; `active_mask` still "Sky"; one more Undo removes exposure; Undo again removes the layer and `active_mask` falls back to Background with no panic and the Edit panel editing Background.
7. Redo five times. Assert: state of step 3 restored.

**C. Sync and copy (H)**
1. Select 4 photos, active photo has "Sky". Auto Sync ON. Drag Contrast on "Sky". Assert: the other 3 photos' `masks` are unchanged.
2. Sync dialog with "Masking" checked, sync to 3 photos. Assert: each gets a "Sky" layer with the same tools; AI caches stripped; History on each shows one "Sync" step.
3. Copy / Paste with Masking unchecked. Assert: target masks untouched.

**D. WB and Skin on a layer**
1. New Filled Layer "Face". Color > WB picker button visible in the layer section; click, pick a grey. Assert: `tools.wb` set, global `wb` unchanged, label "White Balance - Face" in History.
2. Skin Tone > picker icon (distinct glyph), pick a cheek. Assert: swatch shows the picked colour with a J/C/h readout; three range strips update as Hue/Chroma/Lightness range sliders move; "View selected range" greys everything outside the window; sliders were disabled with the hint text before the pick.
3. Erase eyes and lips with Subtract > Brush; lower opacity to 60.

**E. Curves on a layer and readability (H raster)**
1. Headless raster of the curve editor at 270 px and 250 px panel width, identity and edited, Background and layer. Assert: stroke-to-plot contrast >= 4.5:1, handles outlined, dashed identity visible, header reads "Curve - Face" for the layer.
2. Curve presets dropdown present for the layer; apply "Medium Contrast". Assert: `tools.curve` changed, global curve unchanged.

**F. Empty layer and brush (H for 2)**
1. "+" > New Empty Layer. Assert: row shows "(no mask)" with an inline "+" and the mask menu opens automatically.
2. Press the profile's brush key and paint. Assert: a Brush component is added to the empty layer (not a new layer).

**G. Keyboard profiles (H)**
1. Fresh prefs: Library/Develop = Lightroom, Editor = Photoshop. `O` toggles overlay, `B` does what LR does.
2. Switch Library/Develop to Capture One. Assert: `B` = brush, `E` = erase, `G` = gradient, `M` = overlay; the Editor still uses Photoshop keys; Global keys (undo, prefs, quit) unchanged; the shortcut editor shows C1 chords and no conflicts.
3. Duplicate C1 as "My C1", rebind `M` to `Shift+O`, bind `Cmd+Q` to something. Assert: inline conflict against Global shown and the binding ignored at dispatch; rename and delete "My C1" work; deleting the active profile falls back to Capture One.
4. Migration: start with a legacy `prefs.shortcuts` containing one `library::` and one editor override. Assert: two "Custom (migrated)" profiles exist and are active; old map untouched; downgrade build still reads it.
5. Focus follows: with the Editor window focused, Develop keys do not fire and vice versa.

**H. Performance (owner, numbers in the PR)**
1. 45 MP raw, 1 / 4 / 8 brush layers each with exposure + clarity: slider drag on layer 4 stays within the `tests_slider_speed.rs` budget; VRAM peak reported.
2. 8 Filled-only layers: alpha planes not allocated (scalar path); same drag budget.
3. Export full-res with 8 layers (2 with NR): completes; time and peak VRAM recorded.

## 6. Revised phase-1 task list (one builder per task, file ownership)

Stream C first (independent), then A and B in parallel with disjoint files.

| Task | Scope | Files owned | Done when |
|---|---|---|---|
| C1 | Curve readability: theme tokens, dashed identity, under-stroke, outlined handles, `max(w-48, 240)`, real parametric sampling, region dividers, layer name in header | `lc-ui-egui/src/panels/edit.rs` (curve fns 1014-1266 only), `theme.rs` | Walkthrough E1; headless raster test added |
| C2 | Picker icons: `Icon::WhiteBalance`, `Icon::SkinTone`, Defringe icon; SVGs + registry rows; cursor glyphs | `lc-ui-egui/src/icons.rs`, `panels/detail.rs:783-840` (cursor only), `pc-ui-egui/src/{tool_icon_data,color_icon_data}.rs` (one-line rows, pc owner informed), `assets/icons/**` | Icon silhouette test passes; D2 glyph distinct |
| C3 | Skin tone display: swatch + readout + Clear, three range strips, disabled hint, `Overlay::SkinToneRange` CPU+GPU | `panels/edit.rs:240-262`, `lc-pipeline/src/visualize.rs`, `lc-gpu` overlay path, `panels/detail.rs:721-760` | Walkthrough D2 |
| A1 | `MaskShape::Full`, `mask.add {kind:"full"|"empty"}`, scalar-alpha fast path CPU+GPU, auto-naming by kind, labels "New Filled/Empty Layer", skip `vignette/grain/treatment/bw_mix` from layer reset/used set | `lc-develop/src/settings.rs`, `lc-pipeline/src/{masks,layers}.rs`, `lc-gpu/src/render.rs` (plane skip), `lc-engine/src/cmd/masks.rs`, tests | Goldens unchanged; parity test with Full@50%; Full exposure == global exposure |
| A2 | Auto Sync and Sync guard: drop `masks` from auto-sync delta; `develop.sync/copy` copy layers whole with dialog note; `active_mask` resolved after undo/redo; layer-named history labels | `lc-engine/src/lib.rs:605-650`, `cmd/develop.rs:606-700`, `cmd/masks.rs` (labels), tests | Walkthroughs B6-7, C |
| A3 | Layer bar + "+" menu + inline component menu (Add/Subtract/Intersect incl. Luminance Range) + global-only wrappers with reason + canvas pill | new `panels/layer_bar.rs`; `panels/edit.rs` (`show` 426-600 and `tool_sections` header only), `panels/right.rs`, `state.rs`, `panels/detail.rs` (pill only) | Walkthroughs A, B1-5, F |
| A4 | Decouple overlay / handles / tool clearing / Delete from `RightPanel::Masking`; "Only while drawing" mode; brush on empty layer adds component | `panels/detail.rs:380,726,1158+`, `menus.rs:140,266`, `shortcuts.rs:253-300` | Walkthrough F2; overlay visible from Edit panel |
| A5 | Layer WB picker and layer curve presets: `develop.wbPick {mask?}`, `curve.applyPreset {mask?}`; UI in layer branch | `lc-engine/src/cmd/develop.rs:333-360`, `cmd/curves.rs`, `panels/edit.rs:166-192` and `curve_footer` | Walkthroughs D1, E2 |
| A6 | Legacy "Lightroom local adjustments" group in layer view; strip duplicate tool sections and Local Adjustments from Masking panel; rename panel "Mask" | `panels/masking.rs`, `panels/edit.rs` (group only) | LR-imported photo shows the group expanded; no duplicate sections |
| B1 | Prefs model: built-in profile tables (Lightroom, Capture One, Photoshop, Affinity), `shortcut_profiles` per scope, fixed Global list, migration of `prefs.shortcuts`, validator paths | `pc-engine/src/prefs.rs`, new `lc-ui-egui/src/keymaps.rs` (L/C1 tables), new `pc-ui-egui/src/keymaps.rs` (PS/Affinity tables), tests | Walkthrough G4; downgrade-safe |
| B2 | Dispatch: resolve chord through active profile per scope; Global precedence; context-sensitive `B/E/G/M` in Develop; focus-follows | `lc-ui-egui/src/shortcuts.rs`, `pc-ui-egui/src/{shortcut_dispatch,shortcuts}.rs`, `apps/local-image/src/library_host.rs` | Walkthroughs G1-2, G5 |
| B3 | Editor UI: scope selector (Global / Library+Develop / Editor), profile dropdown, duplicate/rename/delete, inline conflicts incl. against Global, Preferences entry; optional JSON import/export last | `pc-ui-egui/src/prefs_ui.rs:1344-1575`, `pc-ui-egui/src/panels.rs` (prefs page) | Walkthrough G3 |
| X1 | Docs: DEVELOP-DESIGN.md 3.2 (GPU layer tools, WB picker, Full), release note on `full` compatibility | `docs/DEVELOP-DESIGN.md` | After A1-A6 merge |

Order: C1-C3 (can land today) -> A1 + B1 in parallel -> A2, A3 -> A4, A5, B2 -> A6, B3 -> X1 -> test build (per `test-build-first` memory) before final reviews.
