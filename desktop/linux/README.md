# Linux desktop host

Local Image embeds its React interface in a native PySide6 QtWebEngine window. File and folder pickers, project saves, export destinations, and close requests use the desktop host. Remote links open in the system browser.

Normal launches use a persistent named Qt profile under the app's user data directory, including interface preferences. On Linux the directory is `$XDG_DATA_HOME/local-image`, or `~/.local/share/local-image` when XDG is unset. The UI cache is kept separately under `cache/webview`; clearing a cache does not remove preferences or recovery documents. Windows uses its existing native WebView2 host and persistent profile. macOS has an Application Support data location; a macOS app bundle is not included here.

The Hardware guide's **Don't show again** choice is saved in backend settings so it also survives a fresh embedded UI profile. The guide remains available through Help.

To run from a checkout, build the frontend with `npm --prefix frontend ci` and `npm --prefix frontend run build`, install `backend/requirements.txt` and `desktop/linux/requirements.txt` in a Python environment, and run `python desktop/linux/local_image.py`. CPU Dust & scratches works without AI models. The optional native Texture helper belongs at `backend/tools/texture-synthesis/texture-synthesis`; AI features use a separate ComfyUI installation.

`python desktop/linux/local_image.py --smoke-test` uses an isolated disposable test profile, loads the actual React UI, verifies the native bridge, and exits. It does not open user documents. Native drag and drop is not implemented in this Linux host; use File → Open.
