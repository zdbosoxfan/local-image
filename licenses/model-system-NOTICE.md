# The model system: sources and notices

Local Image is GPL-3.0-or-later (see `LICENSE`). The open model system (family profiles, model
detection, workflow builders, custom workflows, the Model Browser) draws on the projects below. This
file records what came from where; `docs/TOOLSET.md` › *Model system provenance* has the details.

## Code ported or adapted

**Krita AI Diffusion** — <https://github.com/Acly/krita-ai-diffusion>
Copyright (C) Acly and the Krita AI Diffusion contributors. GPL-3.0.
Ported to Rust (no Python is shipped): the per-architecture workflow construction (text encoders,
model sampling, guidance, reference and inpaint methods per family), the inpaint geometry (mask
grow and feather from the selection's size, context padding, blur pre-fill, compositing back through
the feathered mask), the "fill the green area" instruction form for instruction-edit models, the
prompt conventions of Pony and Illustrious, the built-in style presets, and the approach of
converting editor-format ComfyUI workflows with the server's `/object_info`.
Used in `crates/li-ai/src/{builders,inpaint,custom,presets}.rs` and `crates/li-ai/families/*.json`.

**ComfyUI** — <https://github.com/comfyanonymous/ComfyUI>
Copyright (C) comfyanonymous and contributors. GPL-3.0.
Local Image talks to a separately installed ComfyUI over its HTTP API (`/object_info`, `/prompt`,
`/history`, `/view`, `/upload/image`, `/templates`) and builds graphs from its node definitions. No
ComfyUI code is included. The mock server (`crates/li-ai/src/mock.rs`) restates the input order of
the core nodes the graphs use so editor-format workflows convert in tests.

## Data included

**ComfyUI workflow templates** — <https://github.com/Comfy-Org/workflow_templates>
Copyright (c) 2023-present Comfy Org. MIT License (`licenses/comfyui-workflow-templates-LICENSE`).
Ten templates, their thumbnails and a trimmed `index.json` are test fixtures in
`crates/li-ai/tests/fixtures/templates/`. At run time the Model Browser reads the templates from
the connected ComfyUI (`/templates`) or this repository.

## Data read at run time (not included)

* **ComfyUI-Manager**'s `model-list.json` (<https://github.com/ltdrdata/ComfyUI-Manager>, GPL-3.0):
  read as a community catalogue; files from it are installed only when Hugging Face publishes their
  size and SHA-256.
* **Hugging Face Hub API** and **Civitai API**: model listings, repository file trees (size and
  SHA-256), previews and downloads, under each service's terms. Each model keeps its own licence,
  which the Model Browser shows before downloading.
* **Local Image family profiles** (`crates/li-ai/families/*.json` in this repository): fetched from
  GitHub by *Update Model Profiles*.

## Ideas only (no code)

* **SwarmUI** (<https://github.com/mcmonkeyprojects/SwarmUI>, MIT) and **InvokeAI**
  (<https://github.com/invoke-ai/InvokeAI>, Apache-2.0): the model manager layout (browse by
  architecture, installed versus available, install queue) and the idea of describing model
  architectures as data.
* **stable-diffusion.cpp** (MIT) is a possible later engine; nothing is used from it yet.
