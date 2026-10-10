# Z-Image Turbo Variety and model tip

Continued the existing family graph builder and shared Generate/Draft components in this branch.
The implementation is Rust, adds no dependencies or unsafe code, and is left uncommitted.

1. **Variety:** Generate's Advanced model options and Draft's Sampling section have a 0–100
   slider and an Off button. Default **Off (0)**. Generate settings, Draft settings and named
   presets persist the value; older settings and presets load with Off. Generation metadata
   stores Variety and restoring a generated layer restores it alongside the seed. Only the
   tested Z-Image Turbo Create option is exposed. Base Z-Image, other families and refinement
   do not acquire the workaround.

   Core ComfyUI has no node that creates seeded Gaussian conditioning. The permitted alternate
   conditioning formulation works: encode 96 seed-derived hexadecimal blocks with the model's
   own text encoder, use `ConditioningAverage` to blend them into the positive prompt during
   the first 0.2 of ComfyUI's conditioning schedule, then restore the original prompt using
   `ConditioningSetTimestepRange` and `ConditioningCombine`. The blend weight is
   `0.35 × Variety / 100`; therefore 100 reaches the tested moderate blend, not the rejected
   60% experimental blend. Negative conditioning stays derived from the original prompt.
   Off adds no nodes. The integer mixer is pinned SplitMix64, avoiding dependence on RNG crate
   versions. This is **alternate encoded conditioning, not Gaussian tensor noise**.

2. **Real model evaluation:** RTX 5090, CUDA, ComfyUI 0.38.0
   (`6b747c0428c343e1417219641db93a4fb7cb69ae`), PyTorch 2.14.1+cu130, Python 3.12.15.
   Installed BF16 Turbo, Qwen 3 4B text encoder and `ae.safetensors`; 8 steps, CFG 1,
   `res_multistep` / `simple`, shift 3. Checked `/queue`; one image per request, serially.
   The initially offline installed ComfyUI was started headlessly with custom nodes disabled
   and input/output/temp/user/database state isolated in this worktree. No weights were
   downloaded or changed. The owner's desktop application was not opened or stopped.

   Four seeds: 42, 314159, 987654321, 20261010. Prompts:

   - Fox: “A red fox sitting beside a mossy fallen log in a green forest, soft morning sunlight,
     wildlife photograph”.
   - Chef: “A woman chef wearing a white chef hat and blue apron, holding a bowl of oranges in
     a rustic kitchen, candid photograph”.

   Mean absolute RGB difference is measured after Triangle resizing to 64×36, averaging all
   six image pairs per four-seed set, on the 0–255 scale. It is a diversity proxy, not a pure
   layout metric: every montage was also visually inspected for framing, pose, objects and
   prompt adherence. No CLIP scores are claimed.

| Core approach | Experimental strength | Fox 512×288 MAD | Chef 512×288 MAD | Chef 1024×576 MAD |
|---|---:|---:|---:|---:|
| Off | 0 | 40.792 | 37.118 | 41.999 |
| Alternate blend, early schedule only | 15% | 41.662 | 34.780 | 44.237 |
| Alternate blend, early schedule only | 35% (shipped maximum) | 41.280 | 45.899 | 47.029 |
| Alternate blend, early schedule only | 60% (rejected) | 43.308 | 52.841 | 56.590 |
| Alternate blend, all schedule | 15% | 40.903 | 34.190 | — |
| Alternate blend, all schedule | 35% | 39.590 | 46.769 | — |
| Alternate blend, all schedule | 60% (rejected) | 43.325 | 57.471 | — |
| Added seeded latent noise | 0.5× | 40.792 | 37.118 | — |
| Added seeded latent noise | 1× | 40.792 | 37.118 | — |
| Added seeded latent noise | 2× | 40.792 | 37.118 | — |

   At the selected maximum, chef diversity increased **23.7%** at 512×288, with two seeds
   changing from centered frontal framing to side framing/three-quarter poses. All four
   retained the woman chef, hat, bowl of oranges and kitchen. At 1024×576 MAD increased
   **12.0%**, largely through background arrangements; subject framing varied less. All four
   retained the requested interaction and blue apron. Fox diversity increased only **1.2%**;
   fox, log and forest remained in all four. The baseline already missed the sitting pose for
   two seeds, and the moderate blend retained the same sitting/standing split.

   This justifies an optional control, not a universal default: effectiveness varies by
   prompt and resolution, and secondary details can drift. The 60% blend changed the chef
   interaction to working at a counter and lost blue aprons; all-schedule blending also
   changed the subject's gender at high strength. Both high-strength variants were rejected.
   Latent noise gave identical pictures: full-denoise flow sampling starts at sigma 1, where
   the initial latent's coefficient is zero. The tested useful range stays capped at 35%,
   and default Off preserves the normal generation path.

   Scratch montages (rows: Off, 15%, 35%, 60%; columns: seeds in the order above):

   - [Early blend, fox](../../target/zimage-variety/blend-early/p0-montage.png)
   - [Early blend, chef](../../target/zimage-variety/blend-early/p1-montage.png)
   - [Early blend, larger chef](../../target/zimage-variety/blend-early-1024/p1-montage.png)
   - [All-schedule blend, fox](../../target/zimage-variety/blend-all/p0-montage.png)
   - [All-schedule blend, chef](../../target/zimage-variety/blend-all/p1-montage.png)
   - [Latent noise, fox](../../target/zimage-variety/latent/p0-montage.png)
   - [Latent noise, chef](../../target/zimage-variety/latent/p1-montage.png)

   Graph JSON, individual PNGs, pairwise values, durations and hardware snapshots accompany
   each montage under `target/zimage-variety/`. The repeatable Rust harness is
   `crates/li-ai/examples/zimage_variety_check.rs`. The candidate investigation produced
   112 images: 32 each for early blending, whole-schedule blending and added latent noise,
   plus 16 for early blending at 1024×576. The final production sweep uses the shipped
   `Params.variety` path rather than the experimental graph mutation.

   Final shipped slider calibration, 512×288 (24 images, four seeds per cell):

| UI Variety | Internal early blend | Fox MAD | Chef MAD |
|---|---:|---:|---:|
| Off / 0 | 0% | 40.792 | 37.118 |
| 40 | 14% | 41.698 | 34.830 |
| 100 | 35% | 41.280 | 45.899 |

   The 16 Off/maximum PNGs match the earlier evaluated candidate outputs byte for byte.
   Intermediate values need not improve diversity monotonically, as the 40 row shows.
   Production montages: [fox](../../target/zimage-variety/shipped/p0-montage.png) and
   [chef](../../target/zimage-variety/shipped/p1-montage.png); rows Off/40/100, same seed columns.
   A fresh `KSamplerAdvanced` full-denoise replay bypassed cached `KSampler` output and
   reproduced **identical RGB pixels** for fox, seed 42, Variety 100. Its graph, image and
   result are in `target/zimage-variety/shipped/replay*`.

   The first final attempt stalled at the first sampling step and timed out after 20 minutes
   with Variety **Off**. No image was produced. Only this task's server process was stopped.
   Retrying the same graphs with core ComfyUI flags `--disable-dynamic-vram`,
   `--disable-pinned-memory`, `--disable-async-offload` completed: cold first image 28.460 s,
   warmed Off images about 0.456–0.458 s and warmed Variety images about 0.606–0.751 s.
   The precise cause of the initial stall was not established; this is a runtime observation,
   not a change to the app or its graph. Both attempts' logs are retained as
   `comfy-dynamic-stall.log`, `shipped-initial-stall.log`, `comfy-shipped.log` and
   `shipped.log` under scratch.
   The task-owned server was stopped with an empty queue after validation; no owner's
   desktop app, model weights or ComfyUI configuration were changed.

3. **Tip:** Generate, Draft and the shared model picker tooltip show the task's exact
   Turbo-specific note, including “or raise Variety”. Other model IDs, including base
   Z-Image, do not show it. The slider tooltip explains the prompt-detail tradeoff.
   English strings are in `PENDING_AI_TRANSLATIONS`; existing Off translations are reused.

4. **Verification:** one final full pass of the two changed source crates, offline with
   Rust 1.98.1 and `CARGO_BUILD_JOBS=3`. Tests use isolated data/config/cache/temp directories,
   `RUST_TEST_THREADS=1`, `LOCAL_IMAGE_TEST_ADAPTER="NVIDIA GeForce RTX 5090"` and
   `LOCAL_IMAGE_REQUIRE_GPU=1`.

| Suite | Passed | Existing ignored | Failed |
|---|---:|---:|---:|
| `li-ai` unit | 110 | 0 | 0 |
| `li-ai` coverage | 319 | 1 | 0 |
| `photocraft-ui-egui` unit | 994 | 5 | 0 |
| `photocraft-ui-egui` GPU integration | 59 | 4 | 0 |
| `photocraft-ui-egui` other integration | 14 | 0 | 0 |
| Total final crate pass | **1496** | **10** | **0** |
| Evaluator targeted tests | 2 | 0 | 0 |
| Attribution targeted tests | 7 | 0 | 0 |

   Both crates' doc-tests have zero examples. The inherited ignores cover the known
   inventory metadata-classification bug, optional icon/contact-sheet checks and benchmarks;
   this task adds no ignored tests. Full pass command:
   `cargo +1.98.1 test --offline -p li-ai -p photocraft-ui-egui`.
   Logs are `target/zimage-variety/full-tests.log`, `example-tests.log` and
   `attributions-tests.log`. Attribution generation adds only the SplitMix64 entry and its
   bundled CC0 notice. Formatting and `git diff --check` pass.

   Added graph tests cover Off/on, seed determinism, restoring original late conditioning,
   leaving negative conditioning unchanged, rejecting invalid request levels, and keeping
   base/other families and non-Create tasks unchanged. Settings tests cover old Generate,
   Draft and preset documents, new round trips, model switching and request propagation.
   UI tests click the model picker, hover the Turbo tooltip, drag Generate and Draft sliders,
   and click Off. The Draft interaction test scrolls the control into view at 1280×800,
   using the existing settings scroll area. Its screenshots use the actual RTX 5090 Vulkan
   renderer; no native GUI window is opened. The new active GPU test is
   `zimage_turbo_variety_tip_follows_picker_clicks_in_generate_and_draft_gpu`.
   Adapter: NVIDIA RTX 5090, PCI `0000:01:00.0`, NVIDIA driver `615.71.09`, Vulkan;
   `target/draft-style/gpu-adapter.txt` records it. Review screenshots:
   [Generate note](../../target/zimage-variety/ui/generate-tip.png),
   [picker tooltip](../../target/zimage-variety/ui/picker-tooltip.png),
   [Draft note and dragged slider](../../target/zimage-variety/ui/draft-tip.png).

   Real app backend timing uses `Ai::generate` / `GenerateRequest`, including validation,
   family/file resolution, graph construction and ComfyUI transport, with unique seeds so
   sampler outputs are fresh. Same fox prompt, 512×288, warmed RTX 5090:

| Variety | Seed | App-path time |
|---|---:|---:|
| Off | 4242424242 | 0.498 s |
| Off | 4343434343 | 0.492 s |
| 100 | 4444444444 | 0.716 s |
| 100 | 4545454545 | 1.243 s |

   These four measurements show the extra conditioning encoder's cost in the actual app
   path; they are not a latency guarantee. Raw values and saved images are
   `target/zimage-variety/app-times.json` and `app-*.png`. The GPU UI test also records five
   full-app drag update/render/readback timings at `ui/interaction-times.json`:
   **6.64–6.85 ms per frame**, 1280×800, actual hardware renderer, including readback.
   No native-window frame-rate claim is inferred from that small headless sample.

   Clippy **passes** for both changed crates, including all targets, with warnings denied:
   `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p li-ai -p photocraft-ui-egui --all-targets -- -D warnings`.
   Log: `target/zimage-variety/clippy.log`. No Cargo manifest or lockfile changes, new
   dependencies, unsafe code, custom ComfyUI nodes or commits.

   **Remaining scope:** none. Gaussian tensor conditioning was unavailable in core nodes;
   the explicitly permitted encoded alternate-conditioning fallback is shipped instead.
   The control is deliberately limited to the tested Turbo Create path. Efficacy on other
   models/tasks is unproven, and the inherited ignored tests above remain outside this task.

Sources followed:

- [Official Z-Image Turbo model card](https://huggingface.co/Tongyi-MAI/Z-Image-Turbo):
  documents low diversity and the distilled model's intended behavior.
- [SeedVarianceEnhancer source](https://github.com/ChangeTheConstants/SeedVarianceEnhancer/blob/4078b9cb6f1d807010e47c3943c7fc4975f34e33/seed_variance_enhancer.py)
  and [MIT-0 license](https://github.com/ChangeTheConstants/SeedVarianceEnhancer/blob/main/LICENSE):
  actual v2.2 uses **uniform** `rand_like` noise, a Bernoulli mask and early/late conditioning
  ranges, not Gaussian noise. Its scheduling idea informed the experiment; no custom node
  or Python implementation is shipped. The blog is background, not the implementation source.
- [Installed-version ComfyUI core conditioning](https://github.com/Comfy-Org/ComfyUI/blob/6b747c0428c343e1417219641db93a4fb7cb69ae/nodes.py),
  [core noise nodes](https://github.com/Comfy-Org/ComfyUI/blob/6b747c0428c343e1417219641db93a4fb7cb69ae/comfy_extras/nodes_custom_sampler.py),
  [flow noise scaling](https://github.com/Comfy-Org/ComfyUI/blob/6b747c0428c343e1417219641db93a4fb7cb69ae/comfy/model_sampling.py),
  and the actual `/object_info` snapshot at `target/zimage-variety/object_info.json`.
  The seeded alternate-text formulation and its calibration are this job's experiment; no
  established core-node replacement for tensor conditioning noise was found.
- [SplitMix64 canonical reference](https://prng.di.unimi.it/splitmix64.c) and its
  [pinned CC0 snapshot](https://github.com/svaarala/duktape/blob/84131700dfd32d9d1063f99b5b0a900f5935a613/misc/splitmix64.c).
  Five integer steps were implemented in Rust; provenance is in `docs/PORTS.md` and
  `licenses/splitmix64-NOTICE.md`.
