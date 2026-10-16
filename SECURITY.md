# Security policy

## Supported versions

Only the latest alpha release receives security fixes. At the time of writing that is
**2.0.0-alpha.1**. Earlier alpha builds and the older `v0.x` tags are not supported. If you
find a problem, check that it still occurs on the latest release before you report it.

## Reporting a vulnerability

Please do not open a public issue or pull request for a security problem.

Report it privately with GitHub's private vulnerability reporting:

1. Open the repository's **Security** tab.
2. Choose **Report a vulnerability**.

You can also go straight to <https://github.com/zdbosoxfan/local-image/security/advisories/new>.

Reports are read by the maintainer. Use the advisory thread for all follow-up, so the discussion
stays private until a fix is ready.

## What to include

- The version (shown by `local-image --version`, or in Help > System Info) and your platform
  (Linux x86_64 or macOS on Apple Silicon).
- The component involved, if you know it: the MCP server or control port, a file importer
  (RAW, PSD/PSB, TIFF, SVG, PDF, Affinity, `.pcraft`, HEIF, and so on), a cloud AI provider, a
  model download, or the ComfyUI connection.
- Steps to reproduce, and what you expected and what happened.
- A small sample file if the problem depends on one. Do not send your private photos. A crafted
  or truncated file that shows the problem is enough.
- The impact as you understand it, and what an attacker would need (for example a local user
  account, a malicious file opened by the user, or network access to a port).
- Logs or a stack trace, with API keys, tokens and personal paths removed.

## Response targets

- We acknowledge a report within 7 days.
- We aim to fix the problem or publish a disclosure within 90 days of the report. If a fix needs
  longer, we will say so in the advisory thread and explain why.
- Credit is given in the advisory and release notes if you want it. Tell us the name or handle
  to use.

## Scope

In scope:

- **MCP server and control port.** The `local-image-cli mcp` server runs on stdio. The
  `--control` port (also `LOCAL_IMAGE_CONTROL_PORT`) and the CLI's `--port` listener bind to
  `127.0.0.1` and require a token. The read and write roots for automation are set with
  `--automation-read-root` and `--automation-write-root`. Anything that lets another process or
  a web page reach these listeners, or read or write outside the configured roots, is in scope.
- **File parsers.** RAW, PSD/PSB, TIFF, SVG, PDF, Affinity, `.pcraft` documents, HEIF and the
  other image decoders. Malformed input that causes memory corruption, unbounded resource use,
  writes outside the chosen destination, or code execution is in scope.
- **Cloud AI adapters.** The OpenAI, Google, Black Forest Labs and Stability AI adapters. Requests
  should go only to the provider's HTTPS hosts. API keys should not be sent anywhere else or
  written to logs. Details are in PRIVACY.md.
- **Model downloads.** Downloads from Hugging Face, Civitai and GitHub, including the host
  allow-list, the size and SHA-256 checks, and the handling of tokens on redirects.
- **ComfyUI connection and LAN discovery.** The client that talks to a ComfyUI server, and the
  read-only LAN probe.
- **Library files.** Malformed or hostile catalog files (`catalog.snap`, `catalog.log`) and
  settings files that cause data loss, code execution or a crash that repeats on every launch.

Out of scope:

- The behaviour of ComfyUI itself, its custom nodes, or model weights that you download and run
  yourself. Report those to their own projects.
- Vulnerabilities in third-party libraries that this project does not reach in a way you can
  demonstrate. Report those upstream, and tell us too if the project is affected.
- Problems that need an attacker who already has write access to your user account or
  unrestricted access to your unlocked computer.
- Denial of service that needs a file or input larger than the documented limits.

Ordinary bugs with no security impact should go to the normal issue tracker.
