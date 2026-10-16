# rawspeed camera data

Local Image distributes the whole, unmodified `data/cameras.xml` from
[darktable-org/rawspeed](https://github.com/darktable-org/rawspeed), commit
`0c16fee7772594e98897bded01fc31bc3409b4cb`, as offline camera-calibration data.
The data file is licensed under the **Creative Commons Attribution-ShareAlike 3.0 Unported**
license, as stated in its intact header. Credit: rawspeed contributors and camera sample/profile
contributors recorded in the file. The data retains its own license; Local Image code is GPL-3.0-or-later.

[License and legal code](https://creativecommons.org/licenses/by-sa/3.0/legalcode).
SHA-256 and whole-file update procedure: `crates/lc-raw/data/cameras/manifest.json` and
[`docs/CAMERA-DATA.md`](../docs/CAMERA-DATA.md). No rawspeed C++ is compiled into Local Image.
The Rust reader follows the documented XML and ISO selection semantics; it is host integration code.
