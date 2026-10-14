# Library, Develop and Preferences

Library and Develop use Lightroom Classic's familiar terms: **Catalog** for the source panel,
**Collections** for groups of photos, and **Loupe** for single-photo viewing. Collections do not
move files. Existing collection names, catalogs and automation remain compatible: commands such
as `album.create` and `view.detail`, and widget IDs such as `sidebarSection:albums` and
`icon:detail`, keep their original IDs.

Develop starts with **Basic**: White Balance (preset, picker, Temperature, Tint), followed by Tone
(Exposure, Contrast, Highlights, Shadows, Whites, Blacks). Curve remains available below Tone.
Color contains the color-specific tools. **Negative** appears below Detail and Optics when a photo
has conversion enabled, or when revealed using **Add tool → Negative**. Revealing a tool does not
change a photo; Convert Negative enables conversion. Display order does not change engine order
or saved settings. Existing section IDs, eye switches and slider commands remain compatible.

The module picker occupies the same top-right position in Library, Develop and both Compositing
personas, immediately before custom window captions where present. Workspace controls sit to
its left. The existing module keyboard shortcuts still work.

There is one **Preferences** window, using the editor shell introduced for AI pages in PR 12.
It contains application pages, Library & Develop pages, AI pages, shortcuts and attributions.
Library's existing `app.settings {tab}` route opens the matching page in this shell. Application
changes commit on **Apply** or **OK**; **Cancel** discards unapplied changes. Catalog-specific
import defaults, XMP and maintenance actions continue to apply immediately and travel with that
catalog; their pages state this. Language and theme are set through the shared Interface page.

All application settings use one `preferences.json`: Library UI state and settings live under
`library.*` (including `library.settings` and `library.module`); typed AI settings live under `ai.*`.
The existing editor sections share this file. Every application settings write uses the host's
checked atomic writer, with durable backups and merge/retry protection against concurrent edits.
`lightcraft_sysmem::app_dirs` resolves one configuration directory, including portable mode. Legacy LightCraft camera profiles remain read-only fallback inputs; new
profiles use the shared configuration directory.
AI, Compositing selections and Develop masks use one effective models root: `ai.model_directory`
when configured, otherwise `<config>/models` (existing environment overrides remain supported).
Changes update already-open Develop sessions; SAM 3 remains optional and disabled by default.

At first native launch, absent sections import older `library-ui.json`, `mode`, AI `config.json`
and previous `dialogs["host.libraryUi"]` / `dialogs["host.module"]` keys. Legacy sources are retained
and no longer updated; exact-byte migration backups live in `<config>/preference-backups/`.
An existing legacy models folder is preserved as `ai.model_directory`. Malformed fields use defaults
while valid fields, unknown AI keys and catalog paths survive. Damaged catalog paths require choosing
the existing catalog; the app never silently substitutes another path. Invalid unified Library values
are retained in `library.recoveredOriginal`, and rejected AI fields are recorded in
`dialogs["migrations.rejectedFields"]`. Catalog-specific settings continue to live with their catalogs.

References: Adobe's [workspace basics](https://helpx.adobe.com/lightroom-classic/desktop/workspace/workspace-basics.html)
places the module picker at the upper right; [tone and color](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html)
and [Develop options](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-options.html)
describe Basic's white balance and tone controls; [Collections](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/photo-collections.html)
describes collection groups and the Collections panel.
