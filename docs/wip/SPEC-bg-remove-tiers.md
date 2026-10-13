# Background removal: Fast + Better Quality tiers (owner requirements, 2026-10-13)

The model choice comes from the bake-off report (~/.local/share/local-image-dev/model-eval/REPORT.md, final two-tier section).

1. **Two tiers only: Fast and Better Quality.** No Max tier.
   - Better Quality must finish in under 45 s per photo on CPU, model load included.
   - The models are CPU-only via tract (li-seg) and must work on Apple Silicon.
   - Keep the current IS-Net only if the report names it for a tier; otherwise it becomes an optional legacy entry or is removed from the default UI.
2. **One shared settings model for the single-image tool and the batch tool.** The single-image tool is the Remove Background section in Properties (`crates/pc-ui-egui/src/background_ui.rs`). The batch tool is File › Automate › Remove Backgrounds (AI)…, command `li.batchRemoveBackgrounds`, which opens via `generate_ui::open_batch()`.
   - Both expose the same controls with the same labels, order, defaults and tooltips: the model tier (Fast / Better Quality), edge refinement/matting options, and any output choices that apply to a single image (mask vs delete pixels, decontaminate colours, etc.).
   - Implement the controls once, as a shared widget/struct, and use it in both places. Don't copy them.
   - The last-used settings are shared, so changing the tier in one place shows it in the other.
3. **The batch tool adds only:**
   - the list of input photos
   - an output folder picker
   - a file-name convention: e.g. `{name}-cutout`, a prefix/suffix, or a token template with a live example preview; overwrite / skip / rename-on-conflict
   - the output format: PNG / TIFF / PSD with alpha
   - progress and cancel
4. **Tooltips state the expected time**, e.g. "Better Quality: about 15 s per photo on this computer". Prefer a measured estimate if one is available.
5. **Models download through the existing verified model catalogue,** with sha256, size, licence and attribution rows. Better Quality can be downloaded on first use, with a clear prompt.
6. **Tests:**
   - the shared settings round-trip between the single and batch UIs
   - both UIs list both tiers
   - the batch naming template and conflict handling
   - cancel mid-batch
   - a cutout smoke test per tier on a tiny fixture (CC0 test images from the bake-off are in ~/.local/share/local-image-dev/model-eval/images)
7. **Keep the Qwen options (owner, 2026-10-13).** The engine list today (`background_ui.rs` MODELS) is Standard (on-device, IS-Net), Qwen AI · Compact (int8) and Qwen AI · Full (bf16), all via ComfyUI/GPU.
   - The new list: **Fast** (on-device), **Better Quality** (on-device), **Qwen AI · Compact**, **Qwen AI · Full**.
   - The Qwen entries keep their existing readiness/download/ComfyUI behaviour. Their tooltip says they need a capable NVIDIA GPU and won't run on a Mac.
   - All four appear in both the single-image tool and the batch tool, with the same shared settings.
   - Existing saved `cutout_engine` values migrate: "quick" maps to the tier that replaces IS-Net; the qwen values stay unchanged.
