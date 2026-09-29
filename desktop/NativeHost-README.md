# Local Image desktop host

The Windows Forms host embeds Local Image using Microsoft Edge WebView2. Native Open Files, Open Folder, and Explorer drag-and-drop provide real source locations so approved edits can be saved beside the source image. The launcher credential stays in the native process. The installed host starts the bundled `backend/LocalRemoveBackend.exe` relative to its own executable; no Documents or development workspace folder is used.

Launch `Local Image.exe` normally, with one folder argument, with image-file arguments, or with one `.lremove` project argument. Capture One external-editor image arguments continue to work. The backend starts in the background if needed.

Editable projects can be opened through the native Open Project picker or dropped from Explorer. A project cannot be opened in the same drop/argument batch as images or another project. Save Project creates or updates a portable `.lremove` archive containing the original, removal layers, merged snapshots, layer visibility, and the current cutout mask, background, transform and shadow. Unapplied brush selections, pen outlines and undo history are not included; the editor reviews pending edits before closing.

The initial project save uses a native Save As picker. Later saves use the session's existing project location, resolved privately by the backend. Saving to another existing project through the picker requires its normal overwrite confirmation; the host then supplies a SHA-256 conflict check. Page-provided filesystem paths and hashes are never accepted. Flattened image Save/Save Unique remains separate from editable project saving.

After the trusted editor reports ready, the window's Close button sends a correlated close request to the editor. The editor reviews all open documents, including images visited elsewhere in a folder, before responding. Cancel keeps the window open. The host closes only on an explicit matching approval; it never forces closure after a timeout or a failed page response. A startup failure before editor readiness may be closed normally. Unexpected process termination still relies on cached-session recovery.

Requirements: Windows 10/11 x64, .NET Framework 4.6.2 or later, Microsoft Edge WebView2 Evergreen Runtime. Keep the two Microsoft.Web.WebView2 assemblies and WebView2Loader.dll beside the executable. The installer handles the Runtime when it is missing. Fresh profiles store browser data, logs, settings, caches and recovery beneath `%LOCALAPPDATA%\Local Image`; existing `%LOCALAPPDATA%\Local Remove` profiles retain their data location. Application files may be installed in Program Files and remain separate from writable per-user data. Test profile overrides `LOCAL_IMAGE_DATA_DIR` or legacy `LOCAL_REMOVE_DATA_DIR` must agree between host and backend. See [installation and storage](../docs/INSTALLATION.md).

Build with `Build-NativeHost.ps1` after extracting the official Microsoft.Web.WebView2 NuGet package version 1.0.4191.47 into `webview2-sdk/package`. Package SHA-256: `F492BBF547D0DA329553B6727435B677579B1E9F91CC9E4A1AD029366D5F23D0`.

The custom Local Image icon is embedded in the executable and is also used by the application window. The build includes 16, 20, 24, 32, 40, 48, 64, 96, 128, and 256 pixel icon resources with transparency. The source artwork is `icon/local-image-icon.png`; `icon/convert_icon.py` packages it for Windows using Pillow. `-PackageRoot` can point the build script at an existing extracted WebView2 package.

Quick Heal now defaults to **Texture repair**, using the model-free, CPU-based [Embark Studios texture-synthesis](https://github.com/EmbarkStudios/texture-synthesis) 0.8.2 helper under the MIT license. It copies nearby texture for small object removal. The working-region bush test took approximately 0.45 seconds; actual operation time varies. **Dust & scratches** remains available in the Quick Heal dropdown and uses the previous OpenCV Telea method for narrow defects. Texture repair can copy unsuitable surroundings; larger objects or complex backgrounds are better handled with the existing FLUX AI Remove option. Both methods create reversible layers, including above generated edits or a **Merge visible to new layer** snapshot. Keep the helper and its license installed with the connector in `tools/texture-synthesis/`.

Diagnostics (no editor window shown):

- `Local Image.exe --self-test --output result.json`: checks trusted-page validation and installed WebView2 runtime.
- `Local Image.exe --probe-webview --output result.json`: initializes an invisible WebView2, loads the editor, reports its URL and title, and exits.
- `Local Image.exe --no-open --output result.json [folder-or-image-or-project-paths]`: starts the backend, optionally registers images or imports one project, reports the editor URL, and exits.

Bridge version 2 reports `{native:true,version:2,projects:true,closeRequests:true}` for `ready`. New actions are `openProject`, `saveProject` (`session_id`, `revision`, optional `saveAs:true`), and `closeReady`. Close events use `{type:"local-remove-native",action:"requestClose",id}` and require `{action:"closeReady",id,approved}` in response. Older running native hosts still report version 1; the editor must use their browser download fallback for projects rather than assuming these capabilities exist.

The setup bridge reports `setup:true`. `chooseBackgroundFolder` opens a native folder picker and registers only the chosen folder through `/api/local-remove/backgrounds/register-folder`. `setupDownloadQwen` accepts only the `int8` or `bf16` variant, then calls the authenticated Qwen download endpoint. It never accepts a page-supplied URL, destination path or command. Existing ComfyUI discovery, model-folder selection, start and eject actions remain available. The host's backend version check must match the release version, currently 0.6.0.

Native downloads are limited to the same-origin session `download` and `download-project` routes. Both use owned Save As dialogs. The pure self-test covers origin/download boundaries, project payload allowlisting and mixed-argument rejection, project navigation, and close-request correlation/cancellation/replay rejection without starting the editor or backend.

Only the exact `http://127.0.0.1:51247/remove` page can send native messages. There is no generic filesystem bridge. HTTPS credit links open in the normal browser. Native HTTP redirects are disabled so the launcher credential cannot be forwarded to a remote service. The host verifies the backend identity, version, and user-data root before using it.

`--configure` opens the native AI connection settings dialog. Its model folder picker and ComfyUI port are saved atomically in the user's `config.json`; the `configureAi` bridge action exposes the same dialog to the editor. Configuration changes are reloaded by an authenticated endpoint. The installer uses `--shutdown-backend` only after all editor windows are closed.

Open windows renew the authenticated backend heartbeat every 10 seconds. The packaged backend exits after 75 seconds without a heartbeat once an active AI generation finishes. This prevents an unused service from remaining running after the app closes. Direct development runs with uvicorn are not subject to this idle shutdown.

Microsoft WebView2 is distributed under Microsoft's SDK license, included as `Microsoft-WebView2-LICENSE.txt` and `Microsoft-WebView2-NOTICE.txt`. Sources: <https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.4191.47>, <https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security>.
