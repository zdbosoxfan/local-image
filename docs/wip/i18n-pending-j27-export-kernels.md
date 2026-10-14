# J27 pending translations

The shared resample list (`li-raster::resample::ResampleChoice::label` / `hint`) is shown by both
export dialogs; the Library translates through `crate::i18n::tr`, the editor through `tl!`. Add
these source strings to both catalogs in the final i18n consolidation.

Library Export only:
- `Resample:`
- `Develop renderer (fastest)`
- `Scales while developing, on the GPU when available.`
- `Develops at full size, then resizes exactly like the editor's Image Size (slower).`

Editor Export As only:
- `Resample`

Both (labels):
- `Bicubic Automatic`
- `Preserve Details (enlargement)`
- `Bicubic Smoother (enlargement)`
- `Bicubic Sharper (reduction)`
- `Bicubic (smooth gradients)`
- `Lanczos (sharpest)`
- `Bilinear`
- `Nearest Neighbor (hard edges)`

Both (hints: Library shows them under the menu, the editor as tooltips):
- `Bicubic Sharper when reducing, Bicubic Smoother when enlarging.`
- `Lanczos with light sharpening when enlarging.`
- `Soft, clean enlargements (Mitchell cubic).`
- `Keeps edge contrast when reducing (sharper cubic).`
- `Balanced cubic for smooth gradients.`
- `Most detail; may show slight halos on hard edges.`
- `Soft and fast.`
- `Copies pixels: for pixel art and hard-edged graphics.`
