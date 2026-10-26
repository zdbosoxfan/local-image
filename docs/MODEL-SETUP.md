# Model setup: running any image model ComfyUI supports

Local Image runs a model you installed in ComfyUI in one of two ways:

- **Builders.** The curated models (Qwen Image 2.1, FLUX.2 Klein, Z-Image, ERNIE-Image, SDXL and
  the other families in `crates/li-ai/families/`) run through Local Image's own workflow builders,
  tuned and tested for each family.
- **Official templates.** Any other model that the connected ComfyUI ships an official workflow
  template for (FLUX.3 Klein, MOSS-Image, Krea 2 and later releases) runs through that template.
  Local Image reads the template from the ComfyUI server itself, so its nodes always match the
  server. A model profile can add polish (labels, ranges, value fixes, the licence), but a model
  does not need a profile to run.

Model setup decides which applies to each file, checks that it can run, and says exactly what to
do when it cannot. The design is recorded in
[ADR 0002](adr/0002-template-native-ai-runtime.md).

## What setup does

For every main model file that ComfyUI lists (`diffusion_models/`, `unet/`, `checkpoints/`):

1. **Identify the file by its weights.** The safetensors or GGUF header (tensor names and shapes,
   metadata) gives the architecture, the variant (`distilled`, `base`, `turbo`, `9b`) and the
   precision (BF16, FP8, INT8, NVFP4, GGUF types). File names are a fallback only when no header
   can be read, and a name that names a model generation nothing describes yet (for example
   "flux-3") is never mapped to an older family.
2. **Choose the engine.** A curated model or a family with a builder stays on its builder. Any other
   file is matched to the server's official templates.
3. **Match templates.** A template names one precision of a model. A file matches when its name is
   the same, when it is the same model at another precision (equal precision-independent stems:
   `flux3-image-klein-bf16` matches a template that declares `flux3-image-klein-int8-convrot`), when
   its profile lists its stem for the variant (renamed files), when it has the same network as a
   file already verified with that template, or when you choose the template.
4. **Bind companions.** Each loader in the template (text encoder, VAE, LoRA, model patch) is
   bound to an installed file: the declared file, the same file at another precision, or an
   equivalent listed in `crates/li-ai/families/runtime.json` (for example `flux2-vae` for
   `flux2-vae_small`). VAEs with a different latent channel count are never substituted.
5. **Compile a recipe.** The template is converted to ComfyUI's API format, its optional branches
   resolved, and each tool derived from it: Create, Create with references, Edit, Refine, Fill and
   Upscale. The recipe is stored, so running needs no network.
6. **Check it.** See the checks below. Nothing is rendered or downloaded.
7. **Publish.** Usable models appear in Generate's model picker.

Results are stored per ComfyUI server in `ai-models.json` in Local Image's state folder (`state/`
under the data folder; `LOCAL_IMAGE_DATA_DIR` moves it). The template package is cached per server and package version
in `state/templates/<host-hash>/<version>/`.

## Checks and messages

| Check | Message (shortened) | What to do |
|---|---|---|
| ComfyUI is too old for the template, and its node list confirms it | `{model} needs ComfyUI {need} or newer. This server runs {have}.` | Update ComfyUI (the message shows how for a git install). |
| Older than the template says, but every node and option is there | `{template} was made for ComfyUI {need} or newer … It may still work` | Verify it, or update ComfyUI. |
| A core node is missing | `This ComfyUI ({have}) doesn't have the node {class} …` | Update ComfyUI. |
| A custom-node class is missing (for example the GGUF loader) | `{file} needs the {pack} custom nodes …` | Install the pack with ComfyUI-Manager, restart ComfyUI. |
| A combo option is missing (for example `CLIPLoader` type `flux3`) | `This ComfyUI ({have}) doesn't offer {class} {input} "{value}" …` | Update ComfyUI. |
| A companion file is missing | `{model} needs the {role} {file} ({size}) in {folder}/. Any precision of {stem} works.` | Install it (setup never downloads on its own). A file only one tool needs, such as Krea 2's style LoRA, turns only that tool off. |
| An equivalent file is used | `{model} expects {declared}. Using {installed}, which decodes the same format.` | Nothing. |
| A Blackwell-only precision (NVFP4, FP4) on an older GPU | `{file} uses {quant}, which needs an NVIDIA Blackwell (RTX 50-series) GPU.` | Download the precision the template declares (offered when this GPU runs it), or Try anyway. |
| MXFP8 on an older GPU | `It runs fastest on Blackwell GPUs and may be slower on {gpu}.` | Nothing required. |
| The model may not fit in GPU memory | `{model} needs about {need} GB of GPU memory …` | It may run slowly; a smaller precision helps. |
| The template has an optional prompt enhancer | `This model can use an optional prompt enhancer ({file}, {size}). Turn on Enhance prompt in Generate to use it.` | Optional. It is never downloaded or turned on by itself. |
| Licence | `Licence: {licence}. {summary}` or `Licence unknown - check before client work.` | Read the licence before paid work. |
| No template fits | `Local Image couldn't match {file} to an official ComfyUI template on this server.` | Choose a template, update ComfyUI, or update Model Profiles. |

ComfyUI errors at run time are explained the same way, from the `error_patterns` in
`runtime.json`: a model ComfyUI cannot read asks for an update and names the file; a shape mismatch
names the text encoder (or VAE) in use and the one the template declares; a file ComfyUI hasn't
listed yet names its folder. ComfyUI's original text stays in the log.

## Verification

Verification is a few small test renders, run only when you ask, one model at a time, and only
while ComfyUI's queue is empty. It uses the same generation path as Generate:

- **Create**: "A single bright red ball in the centre of a plain white background …" at the model's
  native size (at most 1 megapixel) with the template's own steps and guidance.
- **Edit** (models with an edit template): the Create result with "Make the ball blue …".
- **References** (template models that take reference images): the Create prompt with the Create
  result as one reference, through the style graph (Krea 2 Style references) or the edit graph.
- **Refine**: the refine checker's test picture at the recipe's default strength.
- **Fill**: the same picture with the area around its sun selected, "a red ball".

A tool the checks already turned off (a file only it needs is missing) is not rendered. The
report lists any tool a passing test did not render as "not tested".

Each image is measured. Black, blank white, a single flat colour, noise or the wrong size fail
Create, Edit and References; an edit that changed nothing fails Edit; a fill that changed nothing,
changed the picture outside its selection, or left the middle of its selection black or noise fails
Fill (a flat fill passes: the test picture is a flat illustration); a refine that changed the
picture too much (difference over 35) fails Refine. A Create picture without a red ball in the
middle (it usually means the text encoder doesn't fit), an edit that didn't make the ball blue and
a fill with no red in its selection are warnings. Create failing marks the model failed. For a
template model, a failed Edit, References or Fill test turns that tool off until the model is
verified again; Refine tests a model again when you choose it and won't run it if it drifts again.
A built-in model keeps its tools and reports the failure. Time, the first-run flag and the GPU
memory in use during the test are recorded, and ComfyUI's memory is freed after each model.

If a test prompt waits in ComfyUI's queue for over a minute while nothing runs, ComfyUI's prompt
worker has stopped: verification withdraws the prompt, stops (later models are not tested) and
asks you to restart ComfyUI. This was seen on ComfyUI 0.42.5 when `/free` followed several failed
runs (an `IndexError` in `unload_all_models` in ComfyUI's log). One verification runs at a time
across the app and the CLI (`state/ai-verify.lock`); a lock left by a process that has ended, or
not refreshed for two minutes, is taken over.

A verification stays valid after a ComfyUI or template update when the recompiled graph is
unchanged ("Still valid on ComfyUI …"); otherwise the model is marked for a quick re-check, and it
keeps working with its last good recipe while the stored graph still validates.

## Set Up Models

**Window › Set Up Models…** (also the banner above Generate's model picker, the **Set up…** button
next to a model that needs it, Preferences › AI › Models, and **Set up and test** after a template
install in the Model Browser) shows the same results as the doctor, for the connected server:

- **The list** groups the models: *New* (not tested yet), *Needs attention* (blocked, no template,
  failed, or due a re-check), *Verified* and *Built-in*; text encoders and VAEs and ignored files
  fold away below. Each row has a status icon whose shape and colour both
  say the status, and a screen-reader label such as "FLUX.3 Klein · distilled · BF16, ready, not
  tested". Up and Down move between rows, Enter opens Check, the context menu has Ignore and Show
  again.
- **Identify** says what the file is (kind, architecture and variant, how Local Image knows, the
  precision and size), how it runs and through which templates; **Change template…** picks
  another official template (a passing test is what makes it verified). Tensor evidence stays in a tooltip.
- **Check** lists the problems first, each with its fixes, then what passed. Fixes run one at a
  time and every one re-checks afterwards: **Download** (planned like a Model Browser install, its
  size and SHA-256 from the publisher, and confirmed before anything downloads), **Use … instead**,
  **Move to …/** (inside the model folder only, never over a file, with Undo for 30 seconds),
  **Choose template…**, **Copy commands** (shown, never run), **Try anyway** and **Ignore**. **Check online…** opens ComfyUI's releases in the browser.
- **Verify** runs the tests above for that model, shows each tool's result, time and the GPU
  memory in use, and can be cancelled. **Verify all ready** tests every untested model one at a
  time.
- **Use** lists what the model can do and opens Generate on it.

The inventory worker checks files and recipes again when the server or template version changes,
and at each periodic inventory refresh. These are static checks; test renders still need a click.
Every executable branch, resolved companion choice and derivation setting participates in the
tested signature. File size, header and modification time detect normal main-weight replacements;
a payload replacement that preserves all three is not detected. A previously tested template
recipe remains available while it still validates if its new package cannot be read or matched.
Verification retries use the checked recipe, so a tool disabled by its old failure is actually
tested again. Unknown checkpoints get a metadata-aware universal Refine test. A verification result
is discarded if the setup record or server versions changed while its renders ran.

A missing optional style file disables that tool while plain Create remains usable. The runtime
checks the chosen tool's files and nodes again before submitting its graph.

When the number of new models rises during a session, one notice says so. Generate's picker shows
a chip on template models ("Not tested", "Test again", "Verified"); a model that can't run yet (no
recipe, or its test failed) is listed under *Other installed models* with the reason and
**Set up…**. A model whose template has a local prompt enhancer gets an **Enhance prompt** switch in
Generate, off by default and remembered per model; it can be turned on once the enhancer's file is
installed (Set Up Models offers its download).

## `local-image-cli ai doctor`

```
local-image-cli ai doctor [--host <host:port>] [--model-dir <dir>] [--templates-dir <dir>] [--json] [--verify <model|file|all>]
```

Prints every model, its engine, templates, companions, capabilities and issues with fixes. It reads
only: no downloads, moves or renders, and it never starts ComfyUI. Ctrl-C during verification
cancels the doctor's owned prompt, releases model memory and exits 130. `--verify` runs the tests above
(`all`: every template model not tested yet) and stores the results where the app reads them.
`--templates-dir` reads the official templates (an `index.json` and its templates) from a folder
instead of the server. `--json` prints the report (`report_version` 1). Exit codes: 0 nothing
blocks, 1 blockers or failed tests, 2 usage, 3 ComfyUI not reachable, 130 verification cancelled.
The host is `--host`, else
`LOCAL_IMAGE_COMFY`, else the Local AI settings.

An installed optional prompt enhancer gets its own verification render and result; plain Create
stays the default and is tested separately.

### Not implemented yet

These parts of the model setup design are deferred; nothing in the app or the doctor claims them:

- The doctor's **built-in tools** section (for example AI Remove's missing
  `flux-2-klein-base-4b`).
- `--online` (look up a newer ComfyUI and template package), `--offline` and `--compare-engines`
  (render a curated model through its builder and its official template side by side). Check online…
  in the app opens ComfyUI's releases page instead.
- **Verified by use** (a model that rendered fine in Generate counting as verified).
- Separate messages for a file in the model folder that ComfyUI hasn't listed yet, a recipe that
  stopped working after a ComfyUI update (reported as blocked, with what the check found) and a
  text encoder or VAE that changed since the test. A render with no image is reported as
  `VerifyFailed:error`.

## Live test

`crates/li-ai/tests/live_models.rs` checks the owner's installed models against a running ComfyUI.
Every test is ignored by default and refuses to run unless:

- `LOCAL_IMAGE_COMFY` is a literal `127.0.0.1:<port>` with a port of 1024 or more that is not 8188
  (a dedicated, memory-capped test server, never the everyday one);
- `LOCAL_IMAGE_LIVE_MODELS` is the absolute path of the model folder;
- the server's queue is empty.

It never downloads, and never starts or stops ComfyUI. Build first with the server stopped, start
the test server, check `/queue`, run the tests one at a time
(`-- --ignored --test-threads=1`), set `LOCAL_IMAGE_DATA_DIR` to a scratch folder so your own setup
state is untouched, then free and stop the server.

## Maintaining the data

- `crates/li-ai/families/runtime.json` holds the equivalent and never-substitute stem groups, VAE
  latent channels, pinned mirrors, precision and GPU rules, custom-node packs and error patterns.
  It rides the profile update channel; a copy that fails validation is ignored.
- A profile's `detect` block identifies a new architecture from its tensors; its `template` block
  adds polish. A detect block that claims a built-in architecture's tensors must say which one it
  `refines`.
- The template fixtures in `crates/li-ai/tests/fixtures/comfy-0.42.5/` come from the template
  package the server reports; refresh them with each ComfyUI release that changes templates.
