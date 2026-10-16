# Bundled swatch libraries

The application compiles the `.gpl` data, not the upstream JavaScript sources.
Source snapshots are retained for reproducibility; copyright/licence notices
must accompany distributed palette data. All three imported libraries are MIT.

| Library | Pinned source commit | Data | Licence |
| --- | --- | --- | --- |
| Open Color | `3a716ee1f5ff5456db33cb8a6e964afdca1e7bc3` | `yeun/open-color/open-color.json` | `open-color-LICENSE.txt` |
| CSS/SVG named colours | `9ec3195524951f58bc1d33797fccc0f8ecfe070c` | `colorjs/color-name/index.js` | `color-name-LICENSE.txt` |
| Tailwind CSS v3.4.17 | `4f9f603e12b51cc53b8a09c7739b8f88c8eb87eb` | `tailwindlabs/tailwindcss/src/public/colors.js` | `tailwind-LICENSE.txt` |

`assets/attributions-curated.json` records source URLs, commits, authors,
licences and installed licence paths. No proprietary colour books are bundled.

Local Image's generated libraries are original GPL-3.0-or-later data, implemented
in `crates/pc-ui-egui/src/swatches_panel.rs`: the web-safe Cartesian cube uses
six evenly spaced channel levels (0,51,102,153,204,255), CMYK uses its four unit
ink vectors, and the named starter groups use twelve evenly spaced HSV hues
with independently chosen saturation/brightness. These are not Adobe datasets.
