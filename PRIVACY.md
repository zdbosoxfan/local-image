# Privacy

Local Image runs on your computer. Your library, edits and previews stay in local folders. This
page lists every case in which the app connects to a network service, what it sends, and what
it stores locally. The statements here were checked against the source code of this release.
If the behaviour changes, this page changes with it.

## Summary

- The app has no telemetry, analytics, crash reporting, accounts or sign-in.
- Photos leave your computer only when you use a cloud AI provider, or a ComfyUI server on
  another machine, and start an operation that sends them.
- Face grouping and scene classification (Smart Sort) run on your computer. Their model files
  are downloaded once, when you ask for them.
- The update check is off by default. Checking for updates is always your choice.
- API keys and access tokens are stored in plain text in the preferences file on your computer. On macOS and Linux the file is saved readable by your user account only.

## Network connections

### Cloud AI providers (only when you add a key and use a cloud model)

Providers: OpenAI, Google (Gemini image), Black Forest Labs (FLUX) and Stability AI. Each one is
used only if you enter its API key in Local AI settings and then choose one of its models in
the Generate panel. The Generate panel names the provider that will receive your input.

Requests go straight from your computer to the provider's HTTPS API. There is no relay. The app
refuses to contact any other host.

What is sent:

- **Create** (text to image): your prompt and the generation settings. Reference images are
  also sent if you add any.
- **Edit and Fill** (change an existing image): the whole image you are editing, not only the
  selected area. Its longest side is reduced to at most 2048 pixels before sending. For Fill, the
  selection is sent as a mask, and where the provider has no native mask the selected area is
  painted green in the image so the model knows what to replace.
- Reference images, if any, are reduced the same way.

Metadata: the image is re-encoded as PNG from its decoded pixels. Metadata from the original
file (EXIF, GPS location, XMP, and similar) is not forwarded. Pixel content is still sent, so a
location or a face that is visible in the picture is visible to the provider.

Keys: each key is sent only to its own provider, in a request header: a bearer token for OpenAI
and Stability AI, `x-goog-api-key` for Google, and `x-key` for Black Forest Labs.

What the provider does with the prompt and images, how long it keeps them, and whether it uses
them for training are governed by that provider's terms and privacy policy. This project does
not control those.

### ComfyUI server (the server you configure)

Local Image talks to a ComfyUI server over HTTP and a WebSocket. The server address is set in
Local AI settings. The default is `127.0.0.1` on port 8188. The address can be another computer
on your local network, or a remote machine you reach through a tunnel or VPN. Any address that
is not the loopback address is treated as a remote server.

To a ComfyUI server the app sends prompts, workflows, model names and images that it uploads for
a job. It also reads status, queue, history, node information and the results it asked for.

If an installation folder is set and auto-start is on (the default), the app starts ComfyUI on
this computer. ComfyUI, its custom nodes and any models you load run under their own rules.
This project does not control their network behaviour.

**Model setup** (see `docs/MODEL-SETUP.md`) reads, from the same ComfyUI server only: its version
and GPU (`/system_stats`), its node list (`/object_info`), and its own official workflow templates
(`/templates/index.json` and `/templates/<name>.json`), during background inventory refreshes
(about every 30 seconds), when the server or template version changes, when you open Set Up Models,
or when you run `local-image-cli ai doctor`. Templates are cached per server and package version.
These requests stay between this computer and that server; no third party is contacted. Background
checks also read model headers from the local model folder and the stored setup results
(`state/ai-models.json`) to detect changes. Verification runs only when you ask for it:
it sends a fixed test prompt and a synthetic test picture (never one of your photos) to the
server, reads `/system_stats` every half second during each render to record GPU memory in use, and
asks the server to free its memory (`/free`) after each model.

In Set Up Models, **Download** on a file a model needs is a model download (below): its size and
SHA-256 come from the file list of its Hugging Face repository (`api/models/<repo>/tree`), or from a
pinned public copy, and nothing downloads until you confirm. **Move** renames a file between
ComfyUI's model folders on this computer only. **Check online** opens ComfyUI's release page in your
browser; the app itself sends nothing.

**Find on LAN** is a button you press. It reads the active private IPv4 networks of this computer
(at most two networks, each limited to 256 addresses) and requests `/system_stats` from the
configured port and from port 8188 on every address in them. It stops after 30 seconds. It only
reads. It does not queue work or upload anything.

### Model downloads (when you ask for a download)

Downloads start from a control you use, such as a Download button in Smart Sort or the People
panel, an install action in the model browser, or a Download fix in Set Up Models.

Hosts that may be contacted, each limited to HTTPS and an allow-list of hosts:

- Hugging Face (`huggingface.co` and its file delivery hosts).
- Civitai (`civitai.com`, `civitai.red`, and the delivery storage they redirect to).
- GitHub (`github.com` and its release file hosts, and `raw.githubusercontent.com` for files
  pinned to a commit).

Model files are checked against their published size and SHA-256 before they are installed.

As with any download, the host you download from sees your IP address and the file requested.

The optional SAM 3 model (AI masks) has no built-in download source in this release. It is
downloaded only from mirror addresses you configure (see the environment variables below).

Optional tokens: a Hugging Face token and a Civitai token can be entered in Local AI settings,
or read from the `HF_TOKEN` and `CIVITAI_TOKEN` environment variables. A token is sent only to the
host it belongs to, and only on requests to that same host. It is not sent to the CDN hosts that
a download is redirected to.

### Model browser catalogues (when you open or search the model browser)

Opening the model browser, or changing its search or filters, requests catalogue data from:

- the Hugging Face API, including the search text you typed;
- the Civitai API, including the search text you typed;
- preview images from `image.civitai.com`;
- the ComfyUI-Manager model list and the official workflow templates, from
  `raw.githubusercontent.com`.

The responses are cached in a folder under the app's data directory. Installing a template card
first reads the connected ComfyUI's `/system_stats` and `/object_info` to check that it can run the
template.

### Model profile updates (when you press Update Model Profiles)

Fetches an index file and any newer model profile files from this project's GitHub repository
(`raw.githubusercontent.com`). Each profile is checked before it is kept. The update changes the
settings the app uses for a model family. It does not change your photos.

The update also fetches `runtime.json` from the same place: the lists of interchangeable model
files, GPU rules and error explanations that model setup uses. It is checked the same way.

### Update check (GitHub releases)

- **Help > Check for Updates** requests the release list from the GitHub API
  (`api.github.com`, for this repository). The request has a 15-second deadline and a 2 MiB
  response limit. It sends no photos, library data, identifiers or tokens. The request carries
  the User-Agent `Local-Image-update-check`. GitHub sees the request and your IP address.
- **Check for updates at startup** is a preference under General. It is **off by default**. When
  it is on, the same request is made once per launch, and a dialog is shown only if a newer
  release exists.
- The app never downloads or installs a release. The only link it offers is the release page on
  GitHub, which opens only when you choose it.

### Map link

**Show on Map** in the Info panel opens OpenStreetMap in your browser, with the coordinates of
the photo in the address. It happens only when you click it.

### Links

Links in menus, dialogs, and the credits and attributions pages open in your browser only when
you choose them.

### Local listeners (not reachable from the network)

- The `--control` port of the desktop app and the `--port` listener of the CLI are bound to
  `127.0.0.1`. Each requires a token. They are not reachable from other computers.
- The MCP server (`local-image-cli mcp`) talks over standard input and output. In bridge mode it
  connects to a running desktop app on the loopback address only.

## Environment variables that change where requests go

For testing or custom setups, these variables redirect the model catalogue, profile and cloud
sources:

- `LOCAL_IMAGE_HF_BASE`, `LOCAL_IMAGE_CIVITAI_BASE`, `LOCAL_IMAGE_MANAGER_LIST`,
  `LOCAL_IMAGE_TEMPLATES_BASE`, `LOCAL_IMAGE_FAMILIES_BASE`, `LOCAL_IMAGE_CLOUD_BASE`.
- `LIGHTCRAFT_SAM3_MIRRORS` adds download sources for the optional SAM 3 model.

If you set one of these, the requests go to the host you named, and a Hugging Face or Civitai
token then goes to that host too. A cloud base address must still be one of the provider hosts
or an explicitly allowed test host.

## What is stored on your computer

| Data | Location | Notes |
|---|---|---|
| Preferences, including AI settings, API keys and tokens | `preferences.json` in the app's configuration folder: `~/Library/Application Support/Local Image/` on macOS, `$XDG_CONFIG_HOME/local-image/` (default `~/.config/local-image/`) on Linux, `%APPDATA%\Local Image\` on Windows. The folder can be changed with `LOCAL_IMAGE_CONFIG_DIR`, or is the app's own folder in portable mode. | Plain text. On macOS and Linux the app saves it with owner-only permissions (0600); on Windows it keeps the folder's default access rules. Protect it as you would any file that holds passwords. |
| Library catalog | `catalog.snap` and `catalog.log` in the library folder | Metadata, albums, edits and history. The photos stay where they are. |
| Face data | `AI/` folder inside the library folder | Stored on this computer only. |
| Model files | The model folder set in Local AI settings, or the default model folder | Installed only after the size and SHA-256 check. |
| Model setup state | `state/ai-models.json` under the app's data folder | Per ComfyUI server: what each model file is, its compiled recipe, the issues found and the verification results (times, GPU memory, image measurements). No images. |
| Template cache | `state/templates/<version>/` under the app's data folder | The ComfyUI server's official workflow templates, per template package version. |
| Catalogue cache | `catalog-cache/` under the app's data folder (`~/Library/Application Support/Local Image/` on macOS, `$XDG_DATA_HOME/local-image/` on Linux) | Model browser responses. |
| Log | `local-image.log` in the platform log folder (`~/Library/Logs/Local Image/` on macOS) | Diagnostic messages, kept for the last two runs. Not uploaded. |

Crash handling only writes a log entry on this computer. Nothing is sent.

## Smart Sort, face grouping and scene classification

Smart Sort's scene classification and its face detection and recognition models run on your
computer. Image data is not sent anywhere for these steps. Their model files are a
one-time download that starts when you press Download. Face embeddings are kept in the library's
`AI/` folder.

## What this document does not cover

- The behaviour of third-party services and software you connect to the app, including
  providers, ComfyUI, custom nodes, model hosts and your operating system.
- Your own backups. Backups copy the catalog, not the original photos or your preferences.
