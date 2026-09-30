# Workspace references

These sources informed Local Image's interface design. The application is independent; there is no Compositor connection or import integration.

| Reference | Relevant pattern | Local Image design implication |
| --- | --- | --- |
| [Affinity Personas](https://affinity.help/designer2ipad/English.lproj/pages/Introduction/about_Personas.html) | Switching a workspace changes the tools offered for its discipline. | Keep Retouch, Cutout and ImageGen switches visible; show controls relevant to the selected toolset. |
| [Microsoft command bars](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/command-bar) | Frequent actions remain visible; secondary actions use overflow as space narrows. Shared commands keep a consistent position. | Keep common image actions accessible and move occasional setup actions into Settings. |
| [Microsoft menus](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/menus) | Menus group related commands and support consistent keyboard access. | Organize file and editing actions predictably, with visible shortcuts and keyboard operation. |
| [Microsoft Photos editing](https://support.microsoft.com/en-us/windows/apps/photos/edit-photos-and-videos-in-windows) | Editing modes share an image view; background tools expose add/remove mask brushes, size and softness. | Keep the canvas central, with explicit cutout refinement and a clear apply action. |
| [Lightroom Classic workspace](https://helpx.adobe.com/lightroom-classic/desktop/workspace/workspace-basics.html) | A filmstrip supports navigation through a collection while panels serve the active task. | Put collection navigation at the bottom and show it when multiple images are open. |
| [Compositor README](https://github.com/robbietilton/Compositor) | Editable masks, numeric transforms, contextual tool controls and familiar shortcuts support precise image work. | Use compact contextual controls, a clear layer panel, direct canvas manipulation and numeric alternatives. |

These are design interpretations, not claims that the references implement Local Image's exact behavior. In particular, the conditional single-image filmstrip and model-specific generation controls are choices for this app. Model installation belongs in Settings, with a short route there when a selected model is unavailable.

The September 30 generation revision also uses [Photoshop's Contextual Task Bar](https://helpx.adobe.com/photoshop/desktop/get-started/learn-the-basics/boost-workflows-with-the-contextual-task-bar.html), [Firefly Generative Fill](https://helpx.adobe.com/firefly/web/work-with-images/edit-images/generative-fill.html), [Affinity's context toolbar](https://affinity.help/photo2/en-US.lproj/pages/Workspace/contextBar.html), and [Spectrum's accordion](https://opensource.adobe.com/spectrum-web-components/components/accordion/). The actual Firefly prompt-bar and Affinity toolbar screenshots were inspected. Their concrete application here is a prompt and primary action together beneath the image, contextual toolbar modes, and initially closed inspector disclosures with only one open at a time. Assets remains an image browser in the workspace, with a standard diagonal-arrow control for deliberate expansion.

Compositor currently targets Apple silicon Macs running macOS 26 or later. Its macOS application and `.comp` project mechanism are outside this Windows integration. The requested [release page](https://github.com/robbietilton/Compositor/releases) was used as a visual/product reference only.
