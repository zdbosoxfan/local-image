# Shared export byte goldens

Synthetic pixels (defined in `tests/shared_export.rs`) and Local Image's generated CC0 ICC
profiles. No photographs. The tests compare Library and Compositing's codec entry points to
one another and to these fixed files. Coverage: RGB8 JPEG 4:4:4/4:2:0 with ICC/EXIF/XMP/300 DPI;
RGB8 PNG/TIFF/WebP; RGBA16 PNG and every TIFF compression; float TIFF; native RGBA8/16 AVIF.

Deliberate regeneration after reviewing a codec change:

```sh
UPDATE_SHARED_CORE_GOLDENS=1 cargo +1.98.1 test --offline -p lightcraft-codecs --test shared_export
```

The regular tests never write fixtures. AVIF's muxer records nclx rather than ICC; its byte
goldens cover pixel depth and alpha. A separate test exercises multi-segment JPEG ICC and 4:2:2.
