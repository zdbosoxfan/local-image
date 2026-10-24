# 0002. Official ComfyUI templates are the recipe for models without a builder

- Status: Accepted (2026-10-24)
- Scope: running AI image models in `crates/li-ai` (identification, recipes, setup checks and
  verification), `local-image-cli ai doctor`, and the model picker

## Context

Local Image ran image models through its own workflow builders, one generic builder driven by a
family profile per model family. New models arrive faster than profiles can be written and tested,
and each new architecture needs new loader types, text encoders, samplers or conditioning nodes
that the generic builder does not know. A review of the model system (findings 61-72) found that:

- name and metadata fallbacks filed new generations under old families (FLUX.3 as FLUX.2 Klein,
  HiDream-O1 as HiDream-I1), producing graphs that fail or silently render garbage;
- detection was compiled code, so a profile update could not teach the app a new architecture;
- unknown-family models were blocked by a node check with an empty node list;
- companions were found by loose name patterns (`ae.safetensors` matched every `*vae`);
- failures were reported as raw ComfyUI text with no version gating and no check before downloads.

The owner wants every model the connected ComfyUI supports to run without waiting for a profile,
and wants to know, before rendering, whether a model will run and what to do if not. The app must
not need an LLM or an AI agent to support a new model.

ComfyUI ships an official workflow template for each model it supports, versioned with the server
(the server serves its own template package at `/templates/`), and the templates declare their
model files with download URLs.

## Decision

1. **Templates are the primary recipe for models without a builder.** Setup matches an installed
   file to the server's own templates by precision-independent stem, profile stems, header
   signature or the person's choice, converts the template to ComfyUI's API format, resolves its
   optional branches, and derives every tool (create with references, edit, refine, fill, upscale,
   LoRA styles) by model-agnostic graph transforms. The compiled recipe is stored per server and
   runs offline.
2. **Profiles are polish, not a precondition.** A profile adds labels, ranges, value fixes, prompt
   rules, licence text and detection rules (`detect` blocks evaluated before the built-in rules,
   with a safety rule for blocks that claim built-in tensors). Curated models stay on their tuned
   builders; their graphs are pinned by golden tests.
3. **`/object_info` decides.** A template's `minComfyUIVersion` only warns; the static check of
   every class and option of every derived graph against the server's node list blocks. Missing
   core nodes ask for a ComfyUI update; missing custom-node classes name the pack.
4. **Verification is explicit and measured.** Small renders through the app's own generation path,
   judged by image measurements, run only on request, one model at a time, on an idle queue.
5. **Runtime data rides the profile channel.** Equivalent files, never-substitute pairs, VAE latent
   channels, mirrors, precision and GPU rules, custom-node packs and error patterns live in
   `families/runtime.json`.
6. **The optional prompt enhancer is opt-in** (owner decision, 2026-10-24): a template's LLM prompt
   upsampler is kept as a separate, off-by-default recipe variant and never downloaded or enabled
   by itself.

## Alternatives considered

- **Synthesising builder profiles for new models.** Rejected: the generic builder would need new
  code for each architecture's loaders and conditioning, which is the bottleneck this removes.
- **Hand-written builder families for each release.** Rejected for new models (it does not scale and
  lags ComfyUI); kept for the curated models, where the tuned graphs are worth their upkeep.
- **Fetching templates from the upstream repository.** Rejected as the default: the server's own
  package matches its nodes; the upstream index is for diagnosis only.

## Consequences

- The UI-to-API converter must follow ComfyUI's workflow format (subgraphs, V3 dynamic inputs,
  widget ordering). Its fixtures (`crates/li-ai/tests/fixtures/comfy-0.42.5/`) are refreshed with
  each ComfyUI release that changes templates, and the corpus test converts and validates them all.
- A template the converter cannot read turns off only that capability, with a message, and a stored
  recipe that still validates keeps working after an update ("last known good").
- Setup state (`ai-models.json`) and the template cache are new per-user data; the network inventory
  in `PRIVACY.md` lists the template reads, verification and their storage.
- Runtime errors are explained from data, with the failing node's files named.

## Revisit when

- ComfyUI stops serving its templates from the server, or changes the template format in a way the
  converter cannot follow without per-release code.
- Most supported models need polish that the profile `template` block cannot express.
