# LensFun: lens correction code and database

Local Image is GPL-3.0-or-later (see `LICENSE`). Its lens profile corrections (Develop › Optics ›
Lens Profile) use **LensFun** (<https://github.com/lensfun/lensfun>) in two ways.

## Code: the `lensfun` crate (LGPL-3.0-or-later)

Local Image depends on the **`lensfun`** Rust crate, version 0.7.0
(<https://crates.io/crates/lensfun>, <https://github.com/vdavid/lensfun-rs>, crate commit
`64d015c3332d0ee44cd366b646a345f789feeede`). It is a pure-Rust port of LensFun, licensed under
the GNU Lesser General Public License, version 3 or (at your option) any later version. LensFun's
C++ library is not compiled or shipped. The crate's own dependencies are MIT OR Apache-2.0.

```
LensFun:          Copyright (C) 2007-2024 Andrew Zabolotny <zap@homelink.ru>
                  Copyright (C) 2014      Roman Lebedev
                  Copyright (C) 2010      Klaus Post (database, calibration data)
                  and the LensFun contributors
Rust port:        Copyright (C) 2026      David Veszelovszki <veszelovszki@gmail.com>
```

Local Image uses the crate for the following:

* the database and its XML parser;
* camera and lens lookup;
* interpolating the calibrations for the shot's focal length and aperture.

`crates/lc-engine/src/lens_db.rs` contains Rust ports of LensFun's
`rescale_polynomial_coefficients` for distortion, TCA and vignetting, from `libs/lensfun/mod-coord.cpp`,
`mod-subpix.cpp` and `mod-color.cpp` at commit `43f9e001ab66c4fcdd4f400463f13b72f94b2288`, all
"Copyright (C) 2007 by Andrew Zabolotny", LGPL-3.0-or-later. `crates/lc-pipeline/src/lensdb.rs`
evaluates the correction-mode models of those files and `modifier.cpp`, including the
normalisation of coordinates to the image's half diagonal. The engine's tests check the results
against the crate's `lensfun::Modifier`. Both files are listed in [`docs/PORTS.md`](../docs/PORTS.md).

LGPL-3.0-or-later code may be combined into a GPL-3.0-or-later program. You may replace the
`lensfun` crate with a modified version by changing the dependency in the workspace `Cargo.toml`
and rebuilding Local Image from its source.

## Data: the LensFun database (CC BY-SA 3.0)

The lens calibration database (the XML files under the crate's `data/db/`, snapshot timestamp
`1577948414`) is built into the application. It is the collective work of the LensFun community
and is licensed under the **Creative Commons Attribution-ShareAlike 3.0 Unported** licence
(<https://creativecommons.org/licenses/by-sa/3.0/>). Local Image reads the database and does not
modify it. If you modify the database files and redistribute them, you must release them under
the same licence. The application's About dialog credits the database:
"Lens data: LensFun database (CC BY-SA 3.0, lensfun.github.io)".
