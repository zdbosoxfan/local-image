# Generated image library

Choose **Generated** inside Assets. Images stay in that dock; the diagonal-arrow expand control opens the larger browser. The same control restores the dock. The collection keeps completed generation and upscale results with their settings, shows cache storage, and supports selective deletion or clearing the collection.

Click an image to focus it. An ordinary click replaces the current focus, so browsing several candidates still leaves one image available to open. **Open in editor** creates a fresh editable document; **Use as draft** loads a copy into Draft & Refine. Neither action changes the library image. The library stores the original completed result, so later retouching or cutout changes in an open document do not replace that saved library copy. Save a `.lremove` project to keep those later edits.

## Storage and deletion

The displayed cache size is the actual size of the library's image PNGs, thumbnails and entry metadata. It excludes model weights, ComfyUI's own output directory, editor recovery sessions and saved project files. Independent copies take additional disk space so the library can be cleared safely while its images remain open for editing.

Choose **Select** to enter checkbox selection for deletion. Check the copies to remove, or use **Select all** for the currently filtered results, then **Delete selected**. **Done** returns to ordinary image browsing. **Clear library…** removes all library copies, including entries outside a filter. Confirm with **Delete library copies**. These actions preserve:

- Open documents and their unsaved edits.
- Original imported files.
- Saved images and `.lremove` projects.
- Model files and other application caches.

An empty library can therefore coexist with open generated documents. The first-use migration record is kept separately from the image cache, so clearing the library does not make old recovery documents reappear on the next launch. New generations are added normally afterward.

If an entry is damaged, its owned cache files still count toward storage usage. Clear can remove incomplete entries containing only known library files. Unexpected files or links are protected: the app reports the problem instead of following them or deleting unrelated data.

A library-write failure does not discard a successful generation or upscale. The result still opens as a document and the app reports that its library copy could not be saved. Save the document as an image or project to retain it.

## Draft, refinement and upscaling

**Draft & Refine…** has a draft side and a refinement side. A library result can become the selected draft without submitting another generation job. Refinement and upscaling create separate results, keeping the draft available for comparison.

Use **Compare larger** for more image space, then **100%** to inspect real output pixels. Fit and zoom controls share the comparison position; drag either image to pan both. The comparison uses each document's full-resolution PNG, so lettering and edges can be checked beyond a thumbnail. The two images retain their own pixel dimensions: at 100%, a larger refinement occupies more canvas space than the draft. **Back to settings** restores the stage controls. See [the Image Gen guide](IMAGE-GENERATION.md) for stage-specific styles, transparency and saved recipes.

When SeedVR2 is ready, **Finish with SeedVR2 upscale** adds a restoration pass after refinement. **Upscale selected draft only** restores the selected image without semantic refinement. The output must enlarge the source and retain its aspect ratio within rounding tolerance. Size validation follows the connected workflow; the previous 4096-pixel and 16 MP application limits have been removed. Long-edge presets preserve the source aspect ratio, so a 3840-pixel long edge is not necessarily 3840 by 2160.

The upscaler snapshots the current visible image, including repair layers, cutout composition and source alpha. The source document and source file remain unchanged. Restored results are new 8-bit image documents; alpha is resized separately and retained. Restoration can synthesize fine detail, so compare the result with its source before keeping it.

Projects retain original generation settings in `generation` and the most recent restoration settings in a separate `upscale` field. For example, a generated 1024 × 1024 image restored to 4096 × 4096 keeps its 1024-pixel generation history while recording the 4096-pixel result separately. An upscaled imported photograph can have `upscale` metadata without `generation` metadata. Stock-image credits continue into the result and its saved project.

## Local API

All routes use the normal local-origin guard. Mutations also require the current `x-local-remove-token`; IDs must be canonical UUIDs. No route accepts a filesystem destination or an arbitrary image URL.

| Route | Purpose |
| --- | --- |
| `GET /api/local-remove/generation/library` | List entries, count and cache bytes. Each entry includes its thumbnail, actual dimensions, settings and creation time in Unix seconds. |
| `GET /api/local-remove/generation/library/{id}/thumbnail` | Read a local PNG thumbnail. |
| `POST /api/local-remove/generation/library/{id}/open` | Return `{session}` for a new editable copy. |
| `POST /api/local-remove/generation/library/delete` | Accept `{ids: [...]}` or `{all: true}`. Return the updated listing, deleted IDs and bytes freed. |
| `GET /api/local-remove/generation/upscale/models` | Report feature enablement, model readiness and output limits. |
| `POST /api/local-remove/generation/upscale` | Accept `session_id`, current `revision`, `width`, `height` and an optional integer `seed`. Return a new session, restoration metadata and any library warning. |

Generation and upscale responses include `library_warning`, normally an empty string. A warning concerns the additional cache copy; the returned editor document remains usable. A stale upscale revision is rejected before a GPU job, and library deletion validates the complete requested selection before deleting its first entry.

Related: [Image Gen](IMAGE-GENERATION.md), [SeedVR2 workflow and validation](SEEDVR2.md), [validation report](LOCAL-IMAGE-VALIDATION.md).
