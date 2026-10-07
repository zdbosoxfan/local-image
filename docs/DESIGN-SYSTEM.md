# Design system

Local Image uses Microsoft's **Fluent UI 2** through
[`@fluentui/react-components`](https://react.fluentui.dev/) (v9), the design
system behind Windows 11 and Microsoft 365. It matches the Windows/WebView2 host
and the Segoe UI typeface. New interface work should use Fluent's own
components, icons and tokens rather than hand-built equivalents.

## Rules

1. **Components come from Fluent.** Use the Fluent component for each job:
   `Button`, `ToggleButton`, `Toolbar` (`ToolbarButton`, `ToolbarToggleButton`,
   `ToolbarRadioButton`, `ToolbarDivider`), `Menu`, `TabList`, `Field`, `Input`,
   `Select`, `Slider`, `Checkbox`, `Dialog`, `Accordion`, `Badge`, `ProgressBar`.
   Do not style a plain `<button>`, `<details>` or `<span>` to imitate one.
   One-of-many pickers (model, precision, output folder, zoom) use
   `ChoiceSelect` (`features/shell/ChoiceSelect.tsx`), which is Fluent's
   `Select` fed by a plain list of choices, so every picker looks and behaves
   the same. Do not build a picker from `Menu` and a button.
2. **Icons come from Fluent System Icons.** Every glyph is registered once in
   `frontend/src/features/shell/Icon.tsx` and used as `<Icon name="…" />`.
   To add one, import the matching `…20Regular` icon from
   `@fluentui/react-icons` and add it to that map. Never draw an SVG by hand or
   type a character (`+`, `×`, `…`, arrows) as an icon.
3. **Icon-only buttons need a name and a tooltip.** Give them `aria-label`
   and wrap them in `<Hint content="…">` (`features/shell/Hint.tsx`), which is
   Fluent's `Tooltip`. Use `relationship="description"` when the tooltip adds
   detail such as a shortcut. Do not use the browser's `title` attribute for
   this.
4. **Colors, radii and shadows are Fluent tokens.** In CSS use
   `var(--colorNeutralBackground2)`, `var(--colorBrandStroke1)`,
   `var(--borderRadiusMedium)` and so on. Only two fixed colors are allowed,
   because they must look the same over any image: the transparency
   checkerboard and the brush-cursor ring in `frontend/src/editor.css`.
5. **The theme is `webDarkTheme`.** `frontend/src/theme.ts` only adapts
   typography (Segoe UI Variable, and sizes that follow the Interface size
   setting). Do not override colors or radii there.
6. **Toggles look like toggles.** On/off commands use `ToggleButton` or
   `ToolbarToggleButton`; one-of-several choices use `ToolbarRadioButton` in a
   `Toolbar` with `checkedValues`. Do not hand-style `aria-pressed` on buttons;
   the shared image-picker style in rule 8 is the only exception.
7. **One home per command.** A command appears once in its natural place: the
   command bar (Undo/Redo, Assets, Remove backgrounds in the Cutout workspace,
   Inspector, Export, Settings), the zoom controls in the status bar (Zoom,
   Fit), the document tabs row (New workspace) or the document bar (Original,
   Save). Menus may repeat commands with their shortcuts.
8. **Image pickers share one selected style.** Add the `li-selectable` class
   to a thumbnail button and set `aria-pressed` (or `aria-current`); do not add
   a separate selected style.
9. **Large interface size must keep working.** Check new controls at
   **Settings → Interface size → Large** and at an 800 × 560 window. The
   command bar collapses to icon-only buttons when its labels do not fit.

## Layout

| Region | Contents |
| --- | --- |
| Menu bar | File, Edit, Layer, Select, View, Help |
| Workspace bar | Retouch / Cutout / Generate tabs; command bar |
| Tool options | Options for the active tool |
| Tool rail (left) | Move, Quick Heal, selection tools, Hand |
| Canvas | Document bar (name, status badge, Original, Save) above the image |
| Inspector (right) | Layers, opacity and cutout properties, or the Generate panel |
| Status bar | Tool hint, status message, zoom controls |

## Code style

The frontend is formatted with Prettier (`frontend/.prettierrc.json`). Run
`npm run format` in `frontend/` before committing; `npm run format:check`
verifies it.
