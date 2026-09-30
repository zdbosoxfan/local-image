# Install Local Image on another Windows PC

Local Image 0.7.0 uses a Windows EXE installer. The application, bundled Python backend and native WebView2 host are installed together. Users do not need to install Python, Node.js, Git or development tools. ComfyUI and AI model weights are optional and are downloaded separately.

## Choose the application and AI folders

Run `Local-Image-Setup-0.7.0.exe` normally. Setup offers installation for all users or only the current Windows user:

| Location | Default | Purpose |
| --- | --- | --- |
| All-users application | `C:\Program Files\Local Image` | Shared application files; installation and updates require administrator approval |
| Current-user application | `%LOCALAPPDATA%\Programs\Local Image` | Application files for the current user; no administrator approval needed |
| User profile | `%LOCALAPPDATA%\Local Image` | Settings, WebView2 data, recovery sessions, generated-image library, caches and logs |
| AI model folder | A writable folder chosen during setup or in Settings | Diffusion models, text encoders, VAEs and LoRAs |
| Dedicated ComfyUI folder | A writable parent folder chosen during setup or in Settings | The optional official portable runtime, including its own Python and GPU libraries |

The application destination is editable during setup. **Local AI setup** offers **Find an existing ComfyUI installation on this PC**, **Set up a dedicated portable ComfyUI installation**, or **Set up AI later; start with local editing**. The **AI storage** page defaults to each user's AppData. Select **Choose model and portable ComfyUI folders** to place large downloads on another local drive, such as `D:\AI Models` and `D:\AI Apps`. Leaving an individual folder blank uses its per-user default. Choosing a folder does not start a model download or install ComfyUI during the Windows installation.

Use a reasonably short application destination. This build accepts up to 155 characters including the drive, leaving space for the bundled libraries' subfolders. Setup reports an overly long destination before copying files. The model and portable runtime folders are separate choices, so large AI files can still live on another drive.

Keep writable AI files out of Program Files, Windows and the installed application directory. Local Image checks AI destinations using the user's normal permissions. Do not grant broad write permission to Program Files to make downloads work. Shared model folders are possible when each intended user has the required read/write access; the application does not change folder permissions for other accounts.

Folder preferences from setup are imported into the launching user's profile. Upgrades preserve existing choices. Each Windows user keeps separate settings, documents, recovery and library data even when application files are installed for all users. An existing `%LOCALAPPDATA%\Local Remove` profile retains its location, so upgrading from Local Remove or Local Image 0.5.0 preserves its data. There is no need to move or rename that folder manually.

## Start with CPU tools or connect AI

Open Local Image from the Start menu or the optional desktop shortcut. The first launch offers **Repair a photo**, **Remove a background**, **Create an image** and **Set up AI**, with detected GPU memory and expandable **VRAM guidance by function and model**. CPU editing, selections, compositing and Quick Heal work before any AI runtime or model is installed. Choosing a task does not download models automatically. **Help > Hardware guide** opens the same guidance later.

For larger controls and supporting text, choose **Edit > Settings > Interface size**. **Compact** keeps the dense desktop workspace; **Comfortable** increases spacing and text; **Large · 200% text** increases text further and lets the Studio panels scroll. The choice is saved for the current profile.

To enable AI, open **Edit > Settings**:

1. Choose the models folder. Local Image downloads compatible files into standard ComfyUI subfolders such as `diffusion_models`, `text_encoders`, `vae` and `loras`.
2. Choose **Find existing**, select a detected installation and click **Use this**. Alternatively, **Choose existing folder…** opens a native picker for the ComfyUI code folder or portable parent folder. A running ComfyUI can be used through its local port.
3. If no suitable installation exists, expand **Install a portable copy**. Use **Change folder…** to choose a writable parent if needed, then click **Install portable ComfyUI** and allow the verified download to finish. Local Image creates a dedicated child folder and leaves other installations intact.
4. Choose **Start AI backend**, then refresh the connection. Use **Image Gen > Browse models…** to compare the supported presets and download only the models required for the intended tasks.

The managed portable package targets supported NVIDIA GPUs. Local Image's tested local AI presets require a CUDA GPU; CPU tools remain available on other hardware. Install a current NVIDIA driver separately when needed. The bundled Python backend for the editor is independent of ComfyUI's embedded Python.

Local Image supplies the selected model folder to ComfyUI through an extra-model-paths configuration. If ComfyUI is already running and the model folder changes, close that ComfyUI instance, start it from Local Image and refresh availability. Local Image does not terminate an unrelated running instance or overwrite its model-path settings. Its initial connection check is bounded so CPU editing remains available when AI is disconnected or unresponsive. ComfyUI's own guide explains [portable installation and shared model paths](https://docs.comfy.org/installation/comfyui_portable_windows).

## Download models and LoRAs

The model browser lists strengths, supported image inputs, precision, default steps, disk requirements and planning guidance for GPU memory. Model weights are large optional downloads; the EXE installer includes none of them. Download one preset at a time. Existing matching files are verified and reused, while mismatched files are preserved and reported.

Downloads use pinned publisher revisions, expected byte sizes and SHA-256 checksums. The app checks available disk space before downloading. ComfyUI installation needs space for both the archive and extracted runtime, in addition to model storage. Download and extraction progress is shown in Settings; model download progress is shown in the model browser.

FLUX.2 Klein 9B requires publisher approval through a Hugging Face account. Follow the access link shown by the app, accept the license if suitable and download the exact requested file into the displayed model subfolder. Retry in Local Image to verify the installed file. Local Image does not collect account credentials or bypass gated downloads. Other models and individual LoRAs also retain their publisher license terms; see [model licenses and preset files](GEN-MODELS.md).

The LoRA library shows image examples first, with details behind a small **i** button. The browser searches the live Hugging Face catalog when searched or refreshed. Curated styles and community results are labeled separately, as are local test and publisher examples. LoRAs must match the exact base model. Downloading an adapter does not automatically enable it or change generation settings.

## Update, uninstall and move to a new PC

Save edits and close Local Image before updating or uninstalling. Let active generation and download jobs finish. The editor backend exits after it becomes idle and loses the desktop heartbeat; wait briefly and retry if Windows still reports a file in use. Avoid force-closing a generation or deleting a running ComfyUI folder.

The uninstaller removes installed application files and shortcuts. User settings, recovery, generated-image library, saved recipes and treatments, batch queue caches, selected model folders, dedicated ComfyUI and saved projects are retained. This avoids deleting shared model files or work during an update. The generated-image library can be cleared selectively or in full from inside Local Image; its displayed storage total covers its own image copies, thumbnails and metadata, not model weights or saved projects. **Batch treatment & export > Previous queues & cache** separately manages batch storage. Clearing either cache retains files already exported elsewhere.

To move work to another PC, save editable `.lremove` projects and copy them to the new machine. Projects contain the image pixels and editing state. Install Local Image normally on the new PC, select local storage and detect or install ComfyUI there. Copy model files into the new selected model folder if desired, then let the app verify and discover them. Regeneration requires the same model and optional LoRA dependencies. Old machine-specific paths in a copied profile do not establish a valid installation on the new PC; choose those folders again in Settings.

## Deployment and troubleshooting

The installer supports Inno Setup's installation scope and destination parameters, plus optional AI preferences:

```powershell
.\Local-Image-Setup-0.7.0.exe /CURRENTUSER /DIR="D:\Applications\Local Image"
.\Local-Image-Setup-0.7.0.exe /ALLUSERS /DIR="C:\Program Files\Local Image"
.\Local-Image-Setup-0.7.0.exe /CURRENTUSER /AISETUP=portable /MODELDIR="D:\AI Models" /AIDIR="D:\AI Apps"
```

`/AISETUP` accepts `discover` (default), `portable` or `later`. `/MODELDIR` selects model storage; `/AIDIR` selects the parent in which a dedicated portable runtime will be installed later. Omitted paths use each user's AppData. Folder choices are validated during setup and again with the launching user's permissions. Existing user settings take priority over installation defaults.

Choose AI storage interactively during setup or in the installed application's Settings. Administrator permission applies to installation, not normal editing. Launch setup normally and let its own elevation prompt handle all-users installation; starting setup from an already-elevated shell can also launch its final application step with elevated credentials. Inno documents [administrative and per-user modes](https://jrsoftware.org/ishelp/topic_admininstallmode.htm) and [original-user launch behavior](https://jrsoftware.org/ishelp/topic_runsection.htm).

If Microsoft WebView2 is missing, setup uses Microsoft's signed Evergreen bootstrapper. That step requires internet access. An offline PC can install Local Image when the WebView2 Runtime is already present; fully offline first-time runtime provisioning requires Microsoft's separate [Evergreen Standalone Installer](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution). WebView2 browser data always belongs in the writable user profile, following [Microsoft's user-data-folder guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/user-data-folder).

Use **Help > Hardware guide** to review GPU memory guidance, and the logs folder under the active AppData profile when reporting startup or ComfyUI errors. Check that removable drives are connected, selected AI folders remain writable, the ComfyUI port matches its local service and the service supports the selected preset. Local Image's editor uses loopback port `51247`; two different profiles should not run on that port simultaneously.

Developer build and smoke-test instructions are in [DEVELOPMENT.md](DEVELOPMENT.md). The release package contains no personal photos, generated test images, model weights, account tokens, or developer virtual environment.

## Earlier installer plan validation

The 0.6.0 plan-only acceptance matrix passed 23 cases using the real installer wizard and parameter handling. It checked all three AI setup modes, default AppData choices, custom folders with Unicode and spaces, independent model/runtime choices, protected and application-contained folders, UNC paths, relative paths, invalid characters, extra colons, non-letter drive identifiers, an invalid AI setup mode and the application-folder length boundary. Valid choices were preserved and invalid choices were reported. No application or AI destination was created.

The matrix uses current-user scope and aborts before installation. Separate tests verified packaged startup in a folder that denies writes, two fresh AppData profiles, CPU editing and the interface. A per-user install/uninstall cycle passed using temporary QA registration identifiers and the same installer code and app package; model, runtime and profile storage survived uninstallation. This validation used one physical Windows PC. See [the deployment validation report](INSTALLATION-VALIDATION.md) for evidence and remaining test scope.
