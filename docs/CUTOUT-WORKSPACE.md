# Cutouts, backgrounds, and shadows

Local Remove has two switchable workspaces: **Retouch** for healing and object
removal, and **Cutout** for isolating a subject and composing it over a background.
They edit the same document. Switching workspaces keeps your applied changes.

Open an image with **File > Open**, or drop it into the window. A single image has
no filmstrip. Opening a folder or several images displays the filmstrip at the
bottom; each image keeps its own editing state as you navigate.

## Remove a background

1. Choose **Cutout** above the image.
2. Select **Compact · INT8** or **Full · BF16** in the right panel. These are
   different precision versions of the same Qwen Image 2.1 model.
3. Wait for **ComfyUI ready**, then click **Remove background**.
4. Inspect the edges against the checkerboard. It represents transparent pixels
   and is not included in your exported image.

Use **Model files & download** to download or verify the selected model in the
Windows desktop app. Configure your ComfyUI installation and model folder in
**Edit > Settings**. Model availability is checked against the running ComfyUI
service. If a required file or workflow node is unavailable, the panel explains
what is missing. See [Qwen setup and model details](QWEN-IMAGE-21.md).

**Recalculate cutout** runs the model again. **Restore full image** temporarily
disables the cutout; **Show saved cutout** enables it again. The **Original**
comparison displays the imported image, including any transparency it already
had, without your repair layers or cutout composition.

## Refine with brushes and the pen

Select an area with the brush, pen, rectangle, or ellipse. For the pen, click
points around an edge and close the path by clicking its first point or pressing
Enter. Add and Subtract adjust the selection before you apply it.

- **Erase selection** removes the selected pixels from the cutout.
- **Restore selection** restores selected pixels from the original image and
  its current repair layers.
- **Feather** softens the cutout boundary. It remains adjustable rather than
  being permanently painted into the saved alpha mask.

You can work without the model: select the subject, choose **Keep**, then
**Keep selection**. This creates a cutout directly from the selection. Erasing a
selection before making a cutout removes that area from the full image.

**Undo cutout** and **Redo** cover the last 20 applied cutout changes, including
refinement, backgrounds, transforms, shadows, and visibility. They leave repair
layers intact. In the Cutout workspace, Ctrl+Z first undoes pending selection
changes; once none remain, it undoes an applied cutout change. Ctrl+Shift+Z redoes
an applied cutout change. A new applied change clears the redo history.

## Position, scale, and rotate the subject

Open **Transform subject** in the Cutout panel.

- Choose **Move subject · V**, or press V, and drag the subject on the canvas.
  Its position previews while dragging and is applied when you release.
- Set **X** and **Y** for offsets in image pixels. Positive X moves right;
  positive Y moves down.
- Adjust **Scale** from 5% to 400% and **Rotation** from -180° to 180°.
  Positive rotation is clockwise. Scaling and rotation use the image center.
- **Reset** restores the original position, 100% scale, and 0° rotation.

The background stays fixed. The shadow updates to follow the transformed subject.
Brush and pen refinements continue to target the visible subject after moving,
scaling, or rotating it. The original pixels and alpha mask remain available, so
moving part of the subject outside the canvas does not delete it. Export uses the
original canvas dimensions and clips anything outside them.

## Choose or generate a background

Use the **Background** selector to choose **Transparent**, **Solid color**, or
**Image**. Click **Choose image…** to import a JPEG, PNG, TIFF, or WebP. The image
fills the canvas while preserving its aspect ratio, cropping excess edges from
the center. Selecting another background leaves the foreground cutout editable.

**Add folder…** creates a thumbnail library. In the desktop app, the library is
remembered between sessions and reads supported images directly inside the
selected folder. Subfolders are not scanned, and a library supports up to 1,000
images. Selecting a thumbnail copies that background into the document, so a
saved project can reopen without the library folder. Re-add the folder after
changing its contents. A folder selected in a browser is available for that page
session; the selected background is still included in a saved project.

**Browse stock library…** opens the in-app Openverse search. Enter a subject or
scene, select a thumbnail, and review its creator, source and license. **Use as
background** imports the image behind the current subject while preserving its
position, mask and shadow. **Open image** creates a normal editable document;
the Image Gen References tab also offers **Add reference**. Search pagination
and imports work without a stock-provider account.

Credits stay with the imported image, background and editable project. Open
**File → Image credits…** to review them later, and include any required credit
when sharing exported images. **External stock websites** still offers Pexels
and Unsplash links: those open your browser and require downloading a photo
before local import. They are not authenticated in-app connections.

For a generated background, describe the environment in **Generate a background**
and click **Generate empty background**. For example:

> Warm studio, beige plaster wall, pale stone surface, soft window light from the left.

Local Remove automatically adds instructions for an empty scene and exclusions
for people, products, text, and foreground subjects. It supplies both positive
instructions and negative conditioning. The default Qwen guidance setting ignores
negative conditioning, so inspect the result: a prompt cannot guarantee that
every generated scene will be empty. The new image is placed behind the existing
cutout; it does not regenerate the foreground subject.

## Add an editable shadow

Enable **Shadow**, then adjust:

| Control | Effect |
| --- | --- |
| Opacity | Strength of the shadow. |
| Softness | Blur around its edge. |
| Offset X / Offset Y | Horizontal and vertical distance from the subject. |
| Height | Compresses the shadow vertically around the subject's lower edge. |

The shadow is built from the current transformed cutout alpha. It sits behind the
subject and above the background, and remains separate from the original pixels.
On a transparent background, its translucent pixels are included in PNG export.

## Save and continue editing

Choose **File > Save project** to keep the original image, repair layers, cutout
mask, feathering, subject transform, current background, and shadow settings in a
portable `.lremove` project. Disabled cutouts are included too. The project does
not depend on the original file or background folder remaining at their old paths.
Earlier projects remain readable; projects without transform settings open with
the subject in its original position.

Pending selections and unfinished pen paths are not saved in a project. The
current applied result is saved, while the 20-step cutout undo history belongs to
the working session and starts fresh when a project is reopened. Save the project
before closing a document if you want to continue editing it.

Use **Export PNG…** in the Cutout panel for a flattened image with real alpha.
Transparent PNG, WebP, and RGBA TIFF are supported. JPEG export requires an opaque
background. Exporting a 16-bit source as TIFF retains its native precision;
explicit PNG, JPEG, or WebP exports are 8-bit. Project originals retain their
source bytes and bit depth.

Qwen evaluates a bounded working image, up to 4 megapixels and 4,096 pixels per
side. Large photos retain their original document dimensions; the generated
alpha is resized back for compositing. The cutout uses Qwen's alpha on the
existing photo and repair pixels, so the model cannot silently recolor or redraw
the retained subject. Fine hair, translucent material, and difficult edges can
still need manual refinement. Transforms resample premultiplied color and alpha
together to avoid introducing dark interpolation fringes.

To remove an object from the scene instead of making a cutout, switch to
**Retouch**, choose **AI Remove** and the Qwen provider, make a selection, and
apply it. The generated repair is restricted to the selected area and remains
an editable repair layer. Merge repair layers while the cutout is disabled, then
enable it again to continue composing.
