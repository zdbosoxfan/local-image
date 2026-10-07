# Install Local Image on Linux

The [0.7.1 Linux preview](https://github.com/zdbosoxfan/local-image/releases/tag/v0.7.2-linux-preview) includes the editor, its Python runtime and the Qt desktop window. You do not need to install Python, Node.js, Git or a web browser to run Local Image. ComfyUI and AI model weights are separate, optional installations.

## Download and install

This preview is for **x86-64 Linux desktops**, built for **Ubuntu 24.04 with glibc 2.39**. Ubuntu 24.04 and Fedora 44 are the initial validation targets; other distributions with glibc 2.39 or newer may also work, but are not guaranteed. It does not run on ARM or 32-bit Linux. A normal graphical desktop and working graphics drivers are required; an NVIDIA GPU is only required for the app's tested AI presets.

### Ubuntu: install the native package

1. Open the [Linux release page](https://github.com/zdbosoxfan/local-image/releases/tag/v0.7.2-linux-preview) and download `Local-Image-0.7.2-linux-preview-linux-x86_64.deb` from **Assets**.
2. Open the downloaded file with your system's package installer and choose **Install**. Ubuntu may ask for your administrator password.
3. Open **Local Image** from the application menu.

If your file manager opens the package as an archive or has no package installer, open a terminal in Downloads and run:

```sh
sudo apt install ./Local-Image-0.7.2-linux-preview-linux-x86_64.deb
```

The package installs the app in `/opt/local-image`, its launcher at `/usr/bin/local-image`, and its menu entry in `/usr/share/applications/local-image.desktop`. `apt` installs declared system-library dependencies when necessary. Normal editing runs as your account without administrator rights.

### Fedora or other compatible desktops: per-user archive

1. Open the [Linux release page](https://github.com/zdbosoxfan/local-image/releases/tag/v0.7.2-linux-preview) and download `Local-Image-0.7.2-linux-preview-linux-x86_64.tar.gz` from **Assets**. The GitHub **Source code** archives are for developers.
2. Use the file manager's **Extract** action. Keep the entire extracted folder together, including `_internal` and `backend`.
3. Open that folder, choose **Open in Terminal**, and enter:

   ```sh
   ./install.sh
   ```

4. Open **Local Image** from your desktop's application menu. The installer prints the installed location and launch command.

Run the installer as your normal account. It installs only for you and does not need `sudo`. After installation, the extracted download folder can be removed. If the menu entry is slow to appear, run `~/.local/bin/local-image` once or sign out and back in. The command `local-image` also works when `~/.local/bin` is in your shell's PATH.

To try the app without installing a launcher, run `./local-image` inside the extracted folder. This still uses your persistent app profile; it does not create a temporary editing workspace.

The release also supplies `SHA256SUMS`. To verify a downloaded asset, download that file into the same directory and compare `sha256sum <downloaded-file>` with its matching line in `SHA256SUMS`.

## Start editing or connect AI

CPU editing, layers, selections, compositing and Quick Heal work without ComfyUI or downloaded models. Use **File > Open…** to choose images or `.lremove` projects. The Linux host currently uses native file pickers; native drag and drop is not implemented.

For AI features, open **Edit > Settings > Local AI**:

1. Choose a writable **Models folder**, or keep the default. A folder shared with an existing ComfyUI installation is supported.
2. Use **Find existing** or **Choose existing folder…** to select your Linux ComfyUI installation. If necessary, install ComfyUI separately using its [official Linux/manual installation guide](https://docs.comfy.org/installation/manual_install). Its GPU/Python environment is independent of the bundled editor runtime. Local Image's Windows portable ComfyUI installer is unavailable on Linux.
3. Start or connect the AI backend, then refresh the connection. Use the model picker and **Download** in Local AI settings to install the preset needed for your task. Model downloads are large and do not start automatically.

Local Image's validated AI presets use NVIDIA CUDA. See the [model guide](GEN-MODELS.md) and the in-app **Help > Hardware guide** for requirements. Model files being present does not guarantee that the connected ComfyUI has the required model nodes; the app reports backend availability separately. Publisher licenses and gated-download requirements still apply.

When you change the model folder while ComfyUI is running, close that instance and restart it through Local Image to use the new folder. Settings reports when this is necessary. ComfyUI documents [extra model paths and the required restart](https://docs.comfy.org/installation/manual_install#adding-extra-model-paths).

## Settings, projects and storage

The following program locations apply to the per-user archive installer. Both installation methods use the same persistent data locations.

| Location with default XDG settings | Purpose |
| --- | --- |
| `~/.local/share/local-image-app/` | Installed program versions and the `current` launcher target |
| `~/.local/bin/local-image` | Command-line launcher |
| `~/.local/share/applications/local-image.desktop` | Application-menu entry |
| `~/.local/share/local-image/` | Settings, recovery, generated-image library, queues and logs |
| `~/.local/share/local-image/webview/` | Persistent interface profile |
| `~/.local/share/local-image/cache/webview/` | Disposable interface cache |
| Selected Models folder | Optional model files and adapters |
| Your chosen save/export folder | Editable projects and exported images |

When `XDG_DATA_HOME` is set to an absolute directory, the `local-image-app`, `applications` and `local-image` directories use that location instead of `~/.local/share`. Application files and user data stay separate. Updates preserve your profile, selected model folder, recovery documents and saved projects. Clearing the interface cache does not remove editing state or preferences. The Hardware guide's **Don't show again** choice is saved in app settings and can be revisited through Help.

Save an editable `.lremove` project when you want to retain image pixels, layers and editing state outside the recovery profile. Projects and exported images can be copied to another computer; regeneration still requires the appropriate models and optional LoRAs.

## Update or uninstall

**Settings → General → Updates** checks GitHub for a newer Linux preview. The app also checks quietly once a day and shows a dot on the Settings button when a newer version exists; it never downloads anything on its own. **Download update** fetches the `.deb` (or the `.tar.gz` on systems without `dpkg`) into your profile's `updates` folder and verifies it against the release's `SHA256SUMS`; a file that does not match is discarded. **Close and open package** reviews unsaved edits, closes Local Image and hands the verified file to your desktop's package installer (`xdg-open`), where you choose Install; for the archive it opens the file so you can extract it and run `./install.sh`. Nothing runs as root from inside Local Image.

Save your work and close Local Image before updating by hand. For Ubuntu, install the new `.deb` through the package installer or `apt` as above. For the archive, extract the new release and run its `./install.sh`; it installs a new program version and switches the launcher to it. Both preserve user data, model folders and separate ComfyUI installations. Use the same installation method when upgrading.

To remove the Ubuntu package, use your package manager or run:

```sh
sudo apt remove local-image
```

To remove the per-user archive installation and its launchers, close Local Image and run:

```sh
"${XDG_DATA_HOME:-$HOME/.local/share}/local-image-app/current/uninstall.sh"
```

The uninstaller preserves settings, recovery, generated images, models, ComfyUI, saved projects and exports. Those can be backed up or managed separately. It only removes files belonging to this per-user installation.

## If the app does not start

Run `local-image` in a terminal to see the startup message; for the per-user archive, use `~/.local/bin/local-image` if the command is not on PATH. Confirm that the entire archive was extracted and that this is an x86-64 system with glibc 2.39 or newer. A `GLIBC_… not found` message indicates an older system library; installing Python or Node.js will not fix it. Use a compatible operating system or [build from source](DEVELOPMENT.md#linux-source-and-release-builds).

Qt uses your desktop's display and graphics libraries. Minimal distributions can require additional runtime packages; the exact missing library is reported at startup. See [Qt's official Linux dependency reference](https://doc.qt.io/qt-6/linux-requirements.html), which lists both runtime libraries and separate development packages. Installing the entire development-package list is unnecessary for this bundled release.

If Local Image reports that the editor port is already in use, close the other Local Image window/process before retrying. Its backend listens only on `127.0.0.1:51247`. Logs are under `~/.local/share/local-image/logs/`, or the corresponding XDG directory. Include the release version, Linux distribution and startup error when reporting a problem.

This preview uses an extracted application folder and a per-user installer. It requires no AppImage/FUSE mount and does not unpack its bundled runtime on each launch. The Linux compatibility baseline follows [PyInstaller's documented glibc limitation](https://pyinstaller.org/en/stable/usage.html#making-gnu-linux-apps-forward-compatible).
