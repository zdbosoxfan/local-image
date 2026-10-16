# Smart Sort exploratory photo evaluation

Frozen and visually reviewed 2026-10-10, **before model inference**. `manifest.json`
contains 330 actual photographs, their Wikimedia Commons file-page IDs, creators,
licence/version/terms URL, original URL, acquisition URL, byte size and SHA-256.
The image bytes live only in `target/tagging-eval/images/` and are not committed.
All sources permit reuse: CC0, public domain, CC BY or CC BY-SA (including the
recorded national variants). Attribution and share-alike terms remain applicable
if photos or derivatives are redistributed. Public-domain entries link to the
source page's licensing section rather than inventing a CC licence.

Discovery used [Openverse's image API](https://api.openverse.org/v1/) restricted
to Wikimedia and [Commons' API](https://commons.wikimedia.org/w/api.php) search
and imageinfo/extmetadata. Current Commons author/licence data and recommended
960-pixel thumbnails were preferred. Three retained early acquisitions are the
original files; their exact acquisition URLs are recorded. HTTP 429/timeouts
were skipped rather than evaded. The final fetcher is polite and verifies the
exact bytes; a changed thumbnail is an explicit hash failure, not a new fixture.

The seed search/category label was checked in contact sheets. Paintings, slides,
diagrams and obvious duplicate/burst shots were excluded; incorrect labels were
corrected. Each accepted photo has one primary label and `reviewed: true`.
`source_label` preserves the seed label for audit. No model outputs influenced
inclusion, annotation or prompts. These are agent-reviewed exploratory labels,
not independent expert annotations. Similar scenes, overlapping event labels,
unequal class counts and Commons selection bias limit generalization.

| Preset | Photos | Categories |
| --- | ---: | --- |
| Conference | 53 | The six unmodified built-in categories |
| Wedding | 32 | The seven unmodified built-in categories |
| Sports | 60 | The five unmodified built-in categories |
| General | 185 | People, Landscape, Food, Architecture, Animals, Events |

General is an **evaluation-only custom preset**, because the product currently
ships Conference, Wedding, Sports and an empty Custom preset. Its exact prompts
are in `examples/tagging_eval.rs`; the built-ins use their existing product
prompts. No prompt tuning, training or exemplars were used. The Wedding
Getting ready category has just one example; its recall is especially uncertain.

## Reproduce

From the repository root, with the authorised pinned model bundles installed in
an isolated test root whose `segmentation/` directory contains the model files:

```sh
scripts/fetch_tagging_eval.sh
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 build --offline \
  -p lightcraft-engine --example tagging_eval
/usr/bin/time -v env LOCAL_IMAGE_GPU_BACKEND=off target/debug/examples/tagging_eval \
  clip-b32-laion crates/lc-engine/tests/references/tagging-eval/manifest.json \
  target/tagging-eval/images ~/.cache/local-image-test-models
```

The evaluator registers the application's real filesystem decoder and runs
`smart_sort::analyze` (decode, develop/render, resize, CPU inference, cache) with
normal half-core parallelism. It classifies with the real `Classifier`, reports
raw-ranked top-1/top-3 and assigned-category precision/recall at Balanced
sensitivity, then times warm reclassification and a cache-only resume. Top-k
measures ranking; precision/recall include Unsorted abstentions. RAM is separate
process peak RSS from `/usr/bin/time -v`, including decoding and model loading.

CLIP is the only shipped tagging model; the current evaluator runs only CLIP.
The rejected candidate's archived measurements cannot be rerun with product code.
`results-*.json` retain measurements, category counts and predictions. CPU timings
are exploratory on a shared developer machine, not isolated release benchmarks.
