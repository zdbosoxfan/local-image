# 7-Zip command-line helper

Local Remove uses the unmodified **7-Zip Extra 26.03 x64** standalone console program, `7za.exe`, to extract the official ComfyUI portable archive. This helper supports the archive's BCJ2 compression and does not need a system installation of 7-Zip or an external 7-Zip DLL.

7-Zip is Copyright (C) 1999-2026 Igor Pavlov. Its main license is GNU LGPL 2.1 or later, with BSD 3-clause and BSD 2-clause components described in the included upstream `License.txt`. The full GNU LGPL 2.1 text is in `copying.txt`. The unchanged upstream distribution notes are in `readme.txt`.

The matching complete upstream source distribution is included as `7z2603-src.7z`. Source code and upstream releases are also available from [7-zip.org](https://www.7-zip.org/) and the [official 26.03 release](https://github.com/ip7z/7zip/releases/tag/26.03). No changes were made to the 7-Zip executable or source.

## Provenance

Retrieved on 22 September 2026. The [official download page](https://www.7-zip.org/download.html) links to the `ip7z/7zip` GitHub release. The downloaded Extra and source archives matched the exact byte sizes and SHA-256 digests returned by the [official release metadata](https://api.github.com/repos/ip7z/7zip/releases/tags/26.03). The helper was taken from `x64/7za.exe` inside the verified Extra archive.

The upstream executable has no Authenticode signature. Verification therefore uses its official release archive checksum and the derived extracted-binary checksum recorded in `SOURCE.json`; no signature verification claim is made.

`7za.exe i` reports version 26.03 (x64) and BCJ2 encoder/decoder support. Local Remove invokes the helper with fixed arguments after its own archive metadata checks, without a command shell.
