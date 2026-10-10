# Refine correctness — completion report

Completed 2026-10-09. Changes are uncommitted in this worktree. Only `li-ai` and task
documentation changed; no dependencies or unsafe code were added. The previous builder's
FLUX.2 Klein reference-latent fix and `resolution_steps` correction are preserved.

## 1. Official workflow comparison

Wrote [REFINE-WORKFLOWS.md](REFINE-WORKFLOWS.md), covering all 16 image families and native
Edit graphs. Every row identifies the official template, our graph, differences and verdict.
For ordinary generators without dedicated img2img templates, the document explicitly combines
the official family recipe with ComfyUI's documented VAEEncode/partial-denoise img2img path.
It distinguishes graph verification from live synthesis.

Added `crates/li-ai/examples/refine_compare.rs`: an offline Rust UI-template summarizer and
comparison tool. It reads root/subgraph scopes, legacy tuple/current object links, boundary
inputs, widgets and bypass modes; prints template hashes and our graphs at both denoises and
native Edit; and compares classes/inputs, including known file variants. Optional branches
are shown as evidence rather than assumed to execute. It works with one JSON file or the
downloaded template directory. Generated inspection: `target/refine-comparison.md`.

## 2. Graph and profile fixes

- Qwen Image 2.1: Refine now respects denoise. Full denoise retains the official reference-edit
  path; partial denoise starts from VAEEncode instead of the encoder's empty latent. Vision
  reference and initial latent share the encoder's 32-pixel rounding and alpha join. The
  wrapper also respects sampler overrides and batch size. Native text-encoder edit families
  now default to denoise 1, alongside reference-latent families.
- Qwen Edit: added the original single-image encoder variant; original input/latent share the
  template's 1.5 MP scale. Plus variants scale the sampler source through FluxKontextImageScale.
  All edit variants use CFGNorm after model sampling.
- FLUX.1 Kontext: Refine and Edit reuse the same scaled source encoding for their reference
  and initial latent, including full denoise.
- FLUX.1 Fill: whole-image Refine supplies a white mask through InpaintModelConditioning,
  matching the template's model input structure while retaining requested partial denoise.
- FLUX.2 Dev/Klein: preserved reference conditioning and partial sigma splitting; only exactly
  denoise 1 uses the empty latent. At CFG above 1, negatives use text encoding instead of
  unconditional zeroed tokens. This fixes the observed base-Klein corruption.
- Added optional, serde-defaulted `file_overrides` profile rules for HiDream Dev/Fast,
  Z-Image base, FLUX.1 Schnell, Klein base and original Qwen Edit. Installed catalog entries
  publish the correct sampler/shift/steps/CFG/encoder; old profiles without the field still
  load. Kontext/Fill explicitly clear inherited Schnell rules.

Added 11 builder regression tests, including one for every changed family, plus an installed
catalog/old-profile compatibility test. Assertions pin conditioning/latent links, scaling,
sampler settings and split-sigma output indices. The existing
`every_family_builds_closed_graphs_for_every_task` remains passing.

## 3. Live refinement checker and real GPU validation

Added `crates/li-ai/examples/refine_check.rs`:

```sh
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 run --offline -p li-ai \
  --example refine_check -- --host 127.0.0.1:8188
```

It checks every recognized installed image-model main file, including alternate precisions
and base tool weights without task-specific removal LoRAs. It resolves files and profile
variants, uploads a deterministic 512×288 test landscape, runs default denoise and 0.4, and
prints RGB mean absolute difference at 64×36 on the 0–255 scale, PASS strictly below 35.
It records outputs/graphs/timings in `target/refine-check/`, reports unsupported weights and
execution failures explicitly, and exits unsuccessfully on failed/no measurements.
`--model filename-substring` permits focused reruns. Both running/pending queues are checked
before each serial job; there is no global interrupt or parallel submission.

After the owner's sandbox removal, ran against **cuda:0 NVIDIA GeForce RTX 5090**, ComfyUI
0.38.0 / PyTorch 2.14.1+cu130 / driver 615.71.09. Nine installed image-model files have
**18 final passing measurements**: two RealVisXL checkpoints, ERNIE, Klein 4B/9B/base 4B,
Qwen 2.1 BF16/INT8, and Z-Image Turbo. Full inventory run: 18 executions, 16 PASS and two
base-Klein FAIL before its negative-conditioning fix. Focused corrected base rerun: two PASS.
Base MAD fell from **65.608 / 55.703** to **5.672 / 3.926**; the rerun also uses the official
base recipe's 20 steps/CFG 5 instead of the removal tool's 28/4. Other final MAD values range
from 0.812 to 14.832. Complete per-model values and timings are in REFINE-WORKFLOWS.md.

Logs: `target/refine-check-live.log`, `target/refine-check-base-live.log`;
final merged measurements: `target/refine-check-final.tsv`; hardware snapshot:
`target/refine-system-stats.json`. Queue was empty after validation.
SeedVR2 and Yue were explicitly excluded as non-image services. No weights were downloaded,
no GUI windows opened, and the running Local Image application was left alone.

**GPU tests added:** the live `refine_check` example is the GPU execution/preservation test.
There are no new Vulkan/shader unit tests because `li-ai` builds HTTP graphs, not GPU kernels.
The measurements ran on the actual RTX 5090 backend; no missing-adapter or llvmpipe results
are counted. Families whose weights are absent were graph-tested; their synthesis still
requires installing those weights or another configured ComfyUI. Known filename variants
are covered; arbitrarily renamed variants need a corresponding user profile.

## Verification

All cargo commands used `+1.98.1`, `--offline` where applicable and `CARGO_BUILD_JOBS=3`,
with one cargo build at a time. Only the changed crate was tested.

- Development: `cargo check --offline -p li-ai --examples` and `--all-targets`; filtered builder
  tests (12 passing at the first graph-fix stage) and the subsequent `refine_` filter
  (8 unit + 2 integration passing).
- Final format: `cargo fmt -p li-ai --check` — passed.
- Final lint: `CARGO_TARGET_DIR=target/clippy cargo clippy --offline -p li-ai --all-targets -- -D warnings`
  — passed, no warnings.
- Full changed-crate suite, once: `cargo test --offline -p li-ai` — **88 unit + 261 integration
  tests passed**, zero failures/ignored; zero doc tests. Log: `target/refine-tests-final.log`.
- New tooling: `cargo test --offline -p li-ai --examples` — **3 passed** (two template-summary
  tests, one color/spatial/dynamic-range metric test).
- Offline comparison tool ran successfully against all mapped cached templates, including
  subgraphs. `git diff --check` passed.

No required implementation work remains. Live coverage is limited to the available installed
weights and one deterministic image/prompt/seed; structural correctness and this preservation
threshold do not establish quality for every possible instruction or weight variant.
