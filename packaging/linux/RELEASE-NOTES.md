Local Image now has a Linux desktop installer with Python, Qt, the editor backend and the React/Fluent interface included. Python, Node.js and a separate browser are unnecessary to run it.

### Download

- **Ubuntu 24.04 x86-64:** download the `.deb`, open it with your package installer, and choose Install. Then open Local Image from the application menu.
- **Fedora 44 or another compatible x86-64 desktop:** extract the `.tar.gz`, open a terminal in its folder, and run `./install.sh` as your normal account. Then open Local Image from the application menu.
- `SHA256SUMS` verifies both downloads. The Source code archives contain developer files.

This preview is built on Ubuntu 24.04 and requires glibc 2.39 or newer, a graphical Linux desktop and working graphics drivers. It does not support ARM or 32-bit Linux.

### GPU controls

- GPU usage and used/total GPU memory are visible beside the generation controls. NVIDIA devices report utilization; other devices show available memory readings.
- Stop cancels the selected generation or refinement upscale through the AI backend. Running-job cancellation requires a current ComfyUI with its job cancellation API.
- A small eject icon unloads GPU models while retaining their files. It is disabled while work is active.
- Desktop and editor backend processes now use Local Image names, including LocalImageBackend. Existing settings and projects remain compatible.

### Editing improvements

- Draft and Refine share one image viewer; the selected step controls the image and its options. Options sit beside the viewer, with scroll-to-zoom, drag-to-pan and compact view controls.
- Workspaces use image tabs. Creating a workspace or closing an image keeps the app open, and switching tools preserves the selected image, including full-resolution refinement results.
- Linux layer updates and Cutout transforms keep the visible image stable while previews update.
- Cutout batch removal supports transparent PNG, white or a chosen background image. The arbitrary 100-image queue cap has been removed.
- Local AI settings include model download controls. The hardware guide supports Don't show again.
- The LoRA library uses consistent preview sizes, readable descriptions and an optional adult-content filter.

Settings, image recovery, generated results and selected model folders remain separate from application files and are preserved during updates. ComfyUI and AI model weights are optional, separate downloads; the Linux preview connects to an existing ComfyUI installation.

See the repository's `docs/LINUX-INSTALLATION.md` for installation, updates, storage and troubleshooting. This is a preview release; native Linux drag and drop is not implemented.
