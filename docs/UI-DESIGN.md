# UI design: persona and workspace layout

How the Local Image 2 window is organised and who it is for. The tokens, widgets, icons and rules that
build it are in [DESIGN-SYSTEM.md](DESIGN-SYSTEM.md); read that before changing any UI. (This page used to
review the retired "Local Remove" web editor; that proposal and its layout no longer exist. Historical
text is in git before this rewrite.)

## Who it is for

The editor inherits PhotoCraft's look and layout (Photoshop-like) and Library/Develop inherit LightCraft's
(Lightroom-like); the owner likes both, so they are kept (see DESIGN-SYSTEM.md section 1).

Photographers and retouchers who already work in Lightroom Classic, Capture One, Photoshop or Affinity
Photo. They know the module names, the panel order, the tool groups and the shortcuts, and they judge the
app on whether they can sit down and work. So:

- Names, menus and shortcuts follow those apps (Photoshop's menu order and tool groups, Lightroom's
  Library/Develop modules, rating, flags, filmstrip and slider behaviour). Where this app differs, the
  command still sits where those users look first.
- AI is a tool or command in the usual place (AI Remove in the healing group, AI Cutout with selection
  tools, Generate as a dock panel), not a separate mode. Every AI change is non-destructive and one undo step.
- The photograph owns the centre; chrome is flat, neutral and compact; nothing promotional appears in the
  working area. A job that takes time runs in the background with status-bar progress and Esc to cancel.
- When something cannot work (no document, model missing, AI engine off) the control is disabled with the
  reason, or the app says what to do in one sentence with the button that does it.

## One window, three modules

The module switch sits in every title bar: **Library** (Ctrl/Cmd+Alt+1), **Develop** (+2),
**Compositing** (+3) (`pc-ui-egui/src/lib.rs:22`, hosted by `apps/local-image/src/library_host.rs`).
A theme change recolours all three; the photo carries across Develop and Compositing without a copy
(Compositing shows it as a Develop layer; Camera Raw Filter edits in Develop and returns).

### Library and Develop (`lc-ui-egui`)

Lightroom-style. Top bar with the module switch and menus; a left sidebar (200 to 480 pt, default 268)
for catalog, folders, collections and presets; the centre shows the photo grid (Photo Grid, Square Grid),
Detail (loupe), Compare, Survey, Reference or People views (`lc state.rs:7`); a filmstrip below; a
bottom bar with ratings, flags and view options; and one **right panel** (250 to 520 pt, default 270) that
shows exactly one of Edit, Crop, Remove, Masking, Red Eye, Versions, Activity, Keywords, Info or Profiles
(`lc state.rs:26`, `panels/right.rs`). The Edit panel is a stack of collapsible sections of sliders;
Masking and the Develop layer bar share the 24/22 pt inset. Photo badges and scrims use the overlay colours.

### Compositing (`pc-ui-egui`)

Photoshop-style and Affinity-style. Custom title bar with menus and module switch, an options bar, a
tool rail on the left, document tabs over the canvas, an optional filmstrip, a status bar, and a **dock**
on the right (250 to 520 pt, default about 290) holding panel groups (`pc dock.rs:37`):

| Group | Tabs |
| --- | --- |
| Color | Color, Swatches, Gradients, Patterns |
| Properties | Properties, Adjustments |
| Character | Character, Paragraph (opened from Window) |
| Navigator | Navigator, Histogram, Info |
| History | History, Actions, Layer Comps |
| Layers | Layers, Channels, Paths (fills the remaining height) |
| Generate | Generate, Library (AI image generation; off by default, opened from Window or a prompt) |

Groups collapse to their tab strip, resize in height, and scroll when their content is taller than they are.
Tabs can be moved between groups, and panels can float.

**Personas.** Compositing has two toolsets, **Pixel** (the raster editor) and **Vector**, switched from
the options bar (`pc-engine/src/toolsets.rs:25`, `pc vector_packages/persona.rs`), following Affinity's
persona switch. Each has its own default tool set; tool ids never alias between them. A saved or custom
tool set can be applied on top.

**Themes.** Pro (dark), Pro Medium Gray (default), Studio (dark), Studio Light and Classic (Window > Theme).
Library and Develop follow the same choice.

## Panel and layout contract

- A new panel goes into an existing dock group or the right-panel enum; it does not get its own window
  unless it is a dialog.
- It must be usable at a 250 pt dock and at 400 pt, in dark and light, at 1.0 and 1.5 scale (see
  DESIGN-SYSTEM.md, layout rules and testing).
- Panels list controls in task order (most-used first), group them with section headers or hairlines, and
  keep rarely used options behind a disclosure or a dropdown rather than showing them all at once.
- Every control is built from the shared widgets. If one is missing, add it to `widgets.rs` and use it
  everywhere it applies.

## Review checklist

Before a UI change merges: shared widgets and tokens only; both themes; 250 and 400 pt; 1.5x; every control
named, disabled ones explain why; Tab reaches everything; screenshots looked at; a Lightroom, Capture One,
Photoshop or Affinity user would find it where they expect it. (UX pass requirements for reviews are in the
owner's final-review plan.)
