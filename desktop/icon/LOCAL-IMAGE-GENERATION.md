# Local Image icon

Created with the built-in imagegen tool on 2026-09-29. The generated PNG has real
alpha and is retained at `desktop/icon/local-image-icon.png`. `convert_icon.py`
packages it into `local-image.ico` at 16, 20, 24, 32, 40, 48, 64, 96, 128 and 256 px,
and creates the 64 px UI asset `backend/frontend/app-icon.png`. This is format
conversion and resizing only. The native launcher, backend and installer embed
the new icon; the page uses the same icon as an inline local asset.

Final prompt:

> Use case: logo-brand. Create one finished Windows desktop application icon for an image editor named Local Image. No lettering. Modern, colorful, restrained professional creative-tool identity, crisp at small sizes. A simple rounded-square photographic frame made of three broad interlocking translucent color planes: electric cyan, rich indigo-violet, warm coral-orange. Within the frame, abstract a small sun and two clean diagonal landscape shapes; substantial simple silhouettes, balanced asymmetry, generous clarity. Subtle dimensional overlap and polished color transitions, not chrome, not glossy plastic. Centered square composition, icon fills about 86 percent of the image, outside the rounded silhouette is genuinely transparent alpha. Clean precise contours, no text, no watermark, no sparkle/star, no palette/paintbrush/camera lens, no surrounding mockup, no cast shadow outside the icon. Deliver a single isolated icon, 1024 by 1024.

The tool returned a 1254 × 1254 source, retained in this repository as
`desktop/icon/local-image-icon.png`.
