from pathlib import Path

HERE = Path(__file__).resolve().parent
out = HERE.parent.parent/'outputs'
path = out/'Local Remove - quick guide.md'
guide = path.read_text(encoding='utf-8')
guide = guide.replace('The executable and window now use a custom photo-and-eraser icon.', 'The executable and window use a custom photo-and-eraser icon. Layer toggles now use cached image patches; editable `.lremove` projects let you keep your layers after closing an image.')
guide = guide.replace('Sessions save automatically and appear under **File > Recent sessions**.', 'Open working sessions are cached automatically and appear under **File > Recent sessions**. Explicitly closing an image clears its working session; save a `.lremove` project to keep the editable original and layers.')
guide = guide.replace('Completed edits persist when you close the application; apply a pending selection before closing if you want to retain that repair.', 'Moving between images keeps the work open. Closing an image or the application reviews those open edits and clears the working sessions after you choose to save a project or discard. Apply a pending selection before saving a project if you want to retain that repair.')
guide = guide.replace('Saving produces a flattened image, but the original and individual repairs stay editable in Local Remove.', 'Saving an image produces a flattened file; the original and individual repairs stay editable while the working session remains open. Save a `.lremove` project to retain those layers after closing.')
section = '''## Save editable projects and close images

Choose **File > Save editable project** (`Ctrl+Alt+S`) and select a `.lremove` filename. A project contains the original image, completed removal/healing layers, their visibility and discard states, masks, and merged snapshots. It can be moved or backed up as one file; the models are not included or needed to view its existing layers. For 16-bit TIFF sources, the original and merged snapshots retain their native precision and ICC profile.

Use **File > Open editable project** (`Ctrl+Alt+O`), drop a `.lremove` file into the app, or pass it to the executable to continue editing. **Save project as** creates another project file. Reopened projects export flattened image copies; they do not automatically gain permission to overwrite the image from which they were originally made.

Click the **×** beside the image name, choose **File > Close image**, or press **Ctrl+W**. If the image has working layers, a dialog offers **Save project and close**, **Discard layers and close**, or **Cancel**. A flattened PNG/JPEG/TIFF/WebP save does not count as saving editable layers. Once you confirm closing, the working session is deleted and the folder entry is reset. Your source image and saved project files remain on disk. Opening the source image again starts fresh; opening the project restores its layers.

Closing the native application reviews all images opened in that window. Cancelling the review or any project Save As dialog keeps every working session open. Folder navigation itself keeps each image's layers, selection, and view.

**Pending brush selections, unfinished Pen paths, selection undo, and the current zoom/pan are not stored in project files.** The close dialog warns about pending selections. Cancel and apply the repair first if you want that result saved as a layer.

In a regular browser, project saving downloads a `.lremove` file. Local Remove cannot confirm whether you completed the download, so it keeps the working image open. Finish saving the download, then close again and choose to discard the working layers. A normal browser-tab close can show a browser warning but cannot perform the native app's save-and-discard review; use **Close image** first for explicit cleanup.

Working sessions retained after an unexpected exit may be recovered under Recent sessions. Existing sessions from older app versions are retained by the update. Project files are the supported way to deliberately save editable work before closing.

## Faster layer comparisons

The editor loads the original and transparent repair patches once, then switches their visibility immediately while saving layer state in the background. A toggle no longer rebuilds and transfers the full photo or reloads the filmstrip. Rapid clicks are saved in order; a failed state update restores the last confirmed state. Decoded image caching is bounded while navigating a folder, so returning to an evicted image may reload its assets.

'''
guide = guide.replace('## Merge visible to a new layer\n', section+'## Merge visible to a new layer\n')
guide = guide.replace('For a previously exchanged TIFF, use **Open With > Local Remove** to return to its saved session.', 'For a previously exchanged TIFF whose working session is still open, use **Open With > Local Remove** to return to it. After explicitly closing that session, open its `.lremove` project to restore editable layers.')
guide = guide.replace('Capture One receives a flattened TIFF. The removal and healing layers remain editable in Local Remove rather than becoming Capture One adjustment layers.', 'Capture One receives a flattened TIFF. Save a `.lremove` project to retain removal and healing layers for later editing in Local Remove; they do not become Capture One adjustment layers.')
guide = guide.replace('Persistent originals and layers are stored in:', 'Temporary working originals and layers are stored in:')
guide = guide.replace('Back up that folder to retain editable sessions elsewhere.', 'Explicit close removes the corresponding working session. Save and back up `.lremove` project files to retain editable work elsewhere.')
guide = guide.replace('## Verified behavior\n', '## Verified behavior\n\n- Project/cache/close tests cover exact 16-bit pixel and ICC round trips, hidden/discarded layers, native snapshots, safe cancellation, and all-or-nothing revision validation before closing multiple images. UI tests confirm immediate cached layer switches with no image requests per toggle. Native host checks cover project arguments, trusted bridge messages, and matching close approval.\n')
path.write_text(guide, encoding='utf-8')
print('Updated guide for editable projects, explicit close, and cached toggles.')
