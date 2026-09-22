# Local Remove third-party credits

## Optional AI setup

Portable archive metadata validation uses unmodified **py7zr 1.1.3**, licensed under
**LGPL-2.1-or-later**. The package's Python source is included by the installer
build alongside its complete `py7zr-1.1.3.dist-info/licenses/LICENSE` notice.
The corresponding upstream source is available from
[py7zr](https://github.com/miurahr/py7zr/tree/v1.1.3). YAML configuration discovery
uses unmodified **PyYAML**, under the **MIT license**; its copyright and complete
license are retained in `pyyaml-*.dist-info/licenses/LICENSE`. The installer
retains installed distribution metadata and license notices for their bundled
dependencies as well.

Archive decoding uses the unmodified **7-Zip Extra 26.03 x64** standalone helper
by **Igor Pavlov**, distributed at `tools/7zip/7za.exe`. This supports the official
ComfyUI archive's BCJ2 compression without requiring a system 7-Zip installation.
The same folder contains the upstream `License.txt`, complete LGPL 2.1 text in
`copying.txt`, upstream `readme.txt`, matching complete source archive
`7z2603-src.7z`, and `NOTICE.md`/`SOURCE.json` with publisher links and verified
checksums. Its main license is **LGPL-2.1-or-later**, with BSD components detailed
in `License.txt`. Source and releases are also available from the
[official 7-Zip 26.03 release](https://github.com/ip7z/7zip/releases/tag/26.03).

Optional ComfyUI and FLUX artifacts are downloaded separately from their verified
publishers. ComfyUI's portable archive is retained unchanged during verification
and its extracted notices remain in the selected installation. The pinned URLs,
sizes, and SHA-256 values are recorded in `ai_download_catalog.py`. The required
`qwen_3_4b.safetensors` file is FLUX's text encoder, independent of the removed
Qwen image-edit option. Existing compatible user-provided model files are
verified and reused; Local Remove does not relicense those files.

## Quick Heal: Texture

Quick Heal's default Texture method uses **texture-synthesis 0.8.2** by **Embark Studios, Anastasia Opara, and Tomasz Stachowiak**. This is their existing multiresolution stochastic texture synthesis implementation, not an algorithm or model developed by Local Remove. It copies appropriate detail from unselected image pixels without downloading model weights or using ComfyUI.

- [Upstream project, examples, and limitations](https://github.com/EmbarkStudios/texture-synthesis)
- [Pinned 0.8.2 release](https://github.com/EmbarkStudios/texture-synthesis/releases/tag/0.8.2)
- [Command-line implementation and mask semantics](https://github.com/EmbarkStudios/texture-synthesis/blob/0.8.2/cli/src/main.rs)
- [MIT license](https://github.com/EmbarkStudios/texture-synthesis/blob/0.8.2/LICENSE-MIT)
- [Apache License 2.0 alternative](https://github.com/EmbarkStudios/texture-synthesis/blob/0.8.2/LICENSE-APACHE)

The project is dual-licensed **MIT OR Apache-2.0**. Local Remove uses it under the MIT license; the complete upstream MIT and Apache license files are included in `licenses/texture-synthesis-LICENSE-MIT.txt` and `licenses/texture-synthesis-LICENSE-APACHE.txt`. Retain these notices when redistributing the native helper. The project is archived; Local Remove pins the official release rather than tracking an unreviewed fork.

The official Windows executable is distributed unchanged at `tools/texture-synthesis/texture-synthesis.exe`. The download archive's SHA-256 was checked against the hash published with the upstream release:

```
Archive: texture-synthesis-0.8.2-x86_64-pc-windows-msvc.zip
Archive SHA-256: 6a07d712a89ebf4c0afe9034b5449181f5c3fc77633b28442d36e1e70db6f643
Executable SHA-256: 6c381b2f65ab7ec0623e12d474e105f1bf78252aaa5b9a0701741931b82411b6
```

Local Remove supplies a native-resolution crop, excludes the complete selection from donor sampling, caps worker threads at eight, runs the helper without a console window, and stores only the repaired region as an editable layer. It retains the selection's original alpha for one composition and preserves unselected pixels exactly. Texture repair is useful for small objects on nearby grass, ground, and similar textures. It does not understand scene geometry; large objects or complex boundaries can need AI Remove. Oversized requests are rejected instead of silently reducing image resolution.

## Quick Heal: Dust & scratches

The optional Dust & scratches method uses the existing **OpenCV `cv2.inpaint` implementation of Alexandru Telea's fast-marching inpainting algorithm** (`INPAINT_TELEA`). The algorithm is described in Telea, *An Image Inpainting Technique Based on the Fast Marching Method*, Journal of Graphics Tools 9(1), 2004, pp. 23-34. We did not develop or train this algorithm.

- [OpenCV inpainting documentation and algorithm attribution](https://docs.opencv.org/4.x/df/d3d/tutorial_py_inpainting.html)
- [OpenCV inpainting API](https://docs.opencv.org/4.x/d7/d8b/group__photo__inpaint.html)
- [OpenCV implementation and its original Intel license notice](https://github.com/opencv/opencv/blob/5.x/modules/photo/src/inpaint.cpp)
- [OpenCV Apache License 2.0](https://github.com/opencv/opencv/blob/5.x/LICENSE)
- [OpenCV Python packaging repository and MIT license](https://github.com/opencv/opencv-python)

OpenCV 5.0.0 is already installed in this application environment. The OpenCV binary is distributed under Apache License 2.0; its Python packaging carries the MIT license. The inpainting source retains its original permissive Intel license notice. Complete installed package notices remain in `.venv/Lib/site-packages/cv2/LICENSE.txt` and `LICENSE-3RD-PARTY.txt`; retain these notices when redistributing the package. The library implementation is used without modification.

This method is intended for dust, scratches, cracks, and other narrow defects on smooth areas. It propagates nearby colors and can smear texture on larger objects. Processing runs locally on the CPU without downloaded model weights or ComfyUI. It remains an explicit choice and is never silently substituted if the Texture helper fails.
