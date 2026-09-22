# Local Remove backend

This folder contains the Python server and web editor used by Local Remove. See the [application instructions](../README.md) for installation and [development guide](../docs/DEVELOPMENT.md) for source builds.

The Windows release bundles Python and the dependencies. Mutable settings, caches, and recoverable sessions live in the current user's Local AppData folder, defined in `app_paths.py`. The application does not require a ChatGPT project, a Documents-based connector installation, or an existing Python environment.

The backend derives from [RapidRAW AI Connector](https://github.com/CyberTimon/RapidRAW-AI-Connector). Its existing [Apache 2.0 license](LICENSE) is preserved. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for additional component attribution.