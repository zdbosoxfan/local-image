# Optional LoRA library

Open **LoRAs → Open LoRA library** in Image Gen. Installed adapters are filtered to the selected model. Browse retrieves current Hugging Face results when opened or refreshed; new community releases can appear without an app update. Results are ordered by publisher update time and show when the query completed. Search is on demand, with no background polling or account login.

Choose a repository, review its model page and safetensors files, then download a selected file. Return to Installed and choose **Use** to enable it. Downloads never enable an adapter automatically. Up to three selected adapters can be applied in order, each with strength from −2 to 2. Start with one adapter near its publisher's recommended strength. Changing the base model clears the selected adapters in the interface.

## Compatibility means different things

- **Curated** identifies an exact reviewed repository revision, filename, size and SHA-256. It records model compatibility evidence, not a guarantee of visual quality across every prompt.
- **Publisher declares compatibility** means the repository's base-model metadata names an accepted model. This is a publisher claim, not a Local Image benchmark.
- **Unverified** means useful base-model metadata is missing. Download requires the explicit assignment checkbox naming the selected model. Such an adapter may still fail to load or produce poor results.
- A declared different base model is rejected. Qwen Image 2.1 is distinct from earlier Qwen Image releases; Z-Image Turbo is distinct from Z-Image Base; Klein 4B differs from 9B and Dev. HiDream-O1 Full differs from HiDream-I1 and O1 Dev. BFL recommends using base-trained Klein adapters with the corresponding distilled model, so the matching 4B and 9B pairings are accepted separately. [BFL training and inference guide](https://huggingface.co/blog/black-forest-labs/flux-2-klein-lora)

The browser can show repositories with incomplete metadata. An unverified name match does not establish compatibility. Selecting a different file or future revision in a curated repository does not inherit its curated status. Known accelerators that need unsupported custom workflows are listed with an explanation and cannot be installed through the standard Image Gen workflow.

## Reviewed starting points

The current collection has **10 adapters**: three for Qwen Image 2.1, two for Z-Image Turbo, two for Klein 4B and three for Klein 9B. The author cards name the exact family. File revisions, sizes and SHA-256 values are pinned in `backend/lora_catalog.py`; future revisions and neighboring files do not inherit reviewed status.

| Model | Style | Approximate size | Start with |
| --- | --- | --- | --- |
| Qwen 2.1 | [Natural exposure](https://huggingface.co/prithivMLmods/Qwen-Image-2.1-Natural-Exposure-LoRA) | 84 MB | **Reference image required.** Neutral exposure/color edit, not a general photorealism generator. Trigger: `Transform the image with balanced neutral exposure`. App starting point: strength 1, 25 steps, guidance 1. Publisher marks this preview experimental. |
| Qwen 2.1 | [Anime character consistency](https://huggingface.co/WarmBloodAban/Qwen-Image-2.1-LoRAs) | 168 MB | **Reference image required.** Character/expression edits; identity is not guaranteed. Start with strength 0.7 within the author's 0.6–0.8 range, 25 steps and guidance 1. Experimental. |
| Qwen 2.1 | [Faceted card illustration](https://huggingface.co/Airmongsity/Qwen-Image-2.1-Sts2-Cards-Drawer) | 101 MB | Bold planes, dark shadows and rim light. Include `sts2 card art`; publisher settings are strength 0.9, **25 steps and guidance 3**. Describe concrete shapes and surfaces. Faces and complex machinery are weak spots. |
| Z-Image Turbo | [Children's drawings](https://huggingface.co/ostris/z_image_turbo_childrens_drawings) | 170 MB | Rough, playful hand drawing. No required trigger; strength 1, 8 steps, guidance 1. |
| Z-Image Turbo | [Photographic realism](https://huggingface.co/suayptalha/Z-Image-Turbo-Realism-LoRA) | 85 MB | Photographic scenes and portrait detail. Include `Realism`; app starting point: strength 1, 8 steps, guidance 1. |
| Klein 4B | [Watercolor wash](https://huggingface.co/rehan-fal/klein-style-fleet) | 96 MB | Include `wtrclr style`; strength 1, 4 steps, guidance 1. Author specifies the distilled 4B model. Small synthetic training set. |
| Klein 4B | [Claymation miniature](https://huggingface.co/rehan-fal/klein-style-fleet) | 96 MB | Include `claymtn style`; strength 1, 4 steps, guidance 1. Clay-like characters and miniature scenes; small synthetic training set. |
| Klein 9B | [Orange splatter illustration](https://huggingface.co/DeverStyle/Flux.2-Klein-Loras) | 166 MB | Include `dvr_osi_style`; app starting point: strength 1, 4 steps, guidance 1. Orange-accented expressive illustration. |
| Klein 9B | [Teal dark illustration](https://huggingface.co/DeverStyle/Flux.2-Klein-Loras) | 166 MB | Include `dvr_tldr_style`; strength 1, 4 steps, guidance 1. New images or reference edits; a dark background can strengthen the effect. |
| Klein 9B | [Blueprint wireframe](https://huggingface.co/DeverStyle/Flux.2-Klein-Loras) | 166 MB | Include `dvr_wf_style`; strength 1, 4 steps, guidance 1. Artistic line drawings from text or references, not accurate engineering diagrams. |

Values labeled “app starting point” use the base model's normal sampler and are not claimed as a publisher benchmark. The card illustration's author also supplies a negative prompt in its model card; negative conditioning becomes effective at guidance above 1. The downloadable QA manifest includes an example prompt and the settings for every adapter.

The library displays descriptions, usage, trigger phrases, strengths and source links. **Add trigger** changes the prompt only when clicked. **Apply recommended settings** is also explicit. Using a curated adapter starts at its recorded strength; it does not silently rewrite the prompt or sampler settings. The backend rejects the two reference-edit adapters when no image reference is supplied.

Licenses apply at both adapter and base-model levels. Qwen adapters remain subject to Qwen Image 2.1's research license, including the anime repository whose metadata says Apache-2.0. The Klein 9B author's card contains a research/non-commercial notice despite Apache metadata, and the BFL base license also applies. These qualifications appear alongside the corresponding styles. Z Turbo and the reviewed Klein 4B adapters declare Apache-2.0. These descriptions report publisher terms; they do not grant additional rights.

Qwen 2.1 accelerators [Viggle](https://huggingface.co/Viggle/Qwen-Image-2.1-viggle-turbo) and [Pruna](https://huggingface.co/PrunaAI/Pruna-Qwen-Image-2.1) require custom sigma workflows that the ordinary generator does not implement. They remain marked unsupported. Original Qwen Image/Edit adapters are not substituted for Qwen 2.1. Historical HiDream and FLUX Dev registry entries remain readable; they are never reassigned to another model.

## Downloads and local storage

Every download resolves the selected public Hugging Face repository to a 40-character commit ID. The app retrieves the selected file's byte count and SHA-256, downloads only that safetensors file, checks its integrity and checks that its header contains recognizable adapter tensors. Pickle/checkpoint files, repository code and arbitrary download URLs are not accepted. Access-gated repositories report the publisher access requirement; the app does not bypass it or collect credentials.

Downloaded filenames are namespaced beneath `models/loras/local-image/`, with a model-specific identity. The local profile keeps the source repository, revision, filename, checksum and assigned model in `state/lora-library.json`. The sampler accepts registered installed IDs only and rejects model mismatches, duplicate selections and out-of-range strengths. It also confirms the chosen file appears in ComfyUI's loader choices.

Downloads use the same generation/setup lock as model installation. Completed files and installed entries survive app restarts. Installed inventory is read locally and remains available offline. A saved image project retains LoRA IDs and strengths, not the weights themselves; retain the registry and model files to reproduce a workflow.

## Live search behavior and limits

Search queries use the fixed Hugging Face Hub API origin, exact base-model tags and a broader name search for adapters with incomplete metadata. Each query requests newest-updated results; the combined list is deduplicated and paged in batches of 20, up to 100 displayed results. There is no application search cache. A metadata request times out after 25 seconds; partial search failures are reported while usable results remain visible. If the Hub is unavailable, refresh later. This is not a promise of exhaustive indexing or a third-party uptime guarantee. [Hugging Face Hub API](https://huggingface.co/docs/hub/api)

The API is `/api/local-remove/loras`: inventory at the root, live `/search`, selected-repository `/files`, and job status at `GET /download`. Only the credentialed native host can call `POST /download`; it supplies model, repository, filename, pinned revision and any explicit unverified assignment. The UI never supplies filesystem destinations or remote code.

Validation on 2026-09-29: **22 library tests and 16 generation API tests passed** after curation. Coverage includes native authorization, exact revision/file trust, model bindings, streamed metadata, directory containment, offline inventory, same-repository styles, descriptions on earlier installs, and rejection of reference-edit requests before GPU work. Eleven candidate files were downloaded or reused with SHA-256 and adapter-header checks. GPU acceptance then found six unmatched global modulation tensors in the Limbicnation pixel-art file; the author's alternate format has the same missing mappings. That repository is now marked unsupported, and only its newly installed registry ID was removed from the three profiles. Its weight file and test outputs remain for audit. The final **10 supported adapters total 1,297,544,776 bytes**, registered in the normal user profile and both isolated QA profiles using one shared weights copy. Existing unrelated registry entries were preserved.

Author metadata/cards are saved under `qa-artifacts/loras/curation/`; `installed-manifest.json` preserves the original candidate record, while `release-manifest.json` lists the final 10 supported adapters with installed IDs, artifact pins, triggers, reference requirements, sampling settings and example prompts. `pixel-art-withdrawal.json` records the precise removed entries. Actual GPU runs are in `qa-artifacts/v05/lora-quality/results.json`; all ten retained styles had clean adapter loads. Visual suitability remains prompt-dependent and is evaluated separately from load success.

## What the actual style tests showed

Each of the ten retained adapters completed a real local GPU job with no adapter-load warnings. The final comparison uses fresh Qwen reruns after correcting opaque RGBA compositing; all ten exported images have alpha 255 throughout. The three Qwen results took 5.578 seconds for exposure, 9.235 seconds for the anime edit and 10.110 seconds for card art. The other examples took 2.313–9.078 seconds. These are individual requests, not averaged speed comparisons or controlled adapter-on/adapter-off quality benchmarks.

The tested strengths were recognizable: playful marker-like submarine drawing, photographic ceramic teapot, neutral portrait exposure, a consistent anime character with a changed expression, faceted shield art, watercolor, miniature clay, orange splatter, dark teal illustration and blueprint-like lines. The anime edit preserved the fresh reference's outfit and pose while adding the requested gentle smile. Card art retained clear shield facets, rivets and red rim light. The exposure edit kept the broad scene but did not correct malformed source lettering.

Instruction adherence remains imperfect. Watercolor produced two foxes instead of one and a signature-like mark; clay produced two watering cans; blueprint micro-labels were invented glyphs. The anime baseline omitted its requested park, so compositing it over white corrects transparency handling without recovering that missing scene. None of these styles guarantees object counts, accurate technical diagrams, exact identity, typography or source-pixel preservation.

Evidence: [final ten-style comparison](../qa-artifacts/v05/lora-quality/accepted-styles-comparison.png), [retained results](../qa-artifacts/v05/lora-quality/accepted-style-results.json), [corrected Qwen runs](../qa-artifacts/v05/lora-quality/alpha-fixed/results.json), and [independent visual review](../qa-artifacts/v05/lora-quality/visual-review.json). The original eleven-candidate contact sheet is historical diagnostic evidence and includes the withdrawn pixel-art adapter and the earlier Qwen compositing defect.

Related: [Image Gen guide](IMAGE-GENERATION.md), [model setup and licenses](GEN-MODELS.md).
