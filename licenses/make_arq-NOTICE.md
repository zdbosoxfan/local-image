# make_arq

Sony Pixel Shift shot displacement and half-pixel phase ordering in
`crates/lc-merge/src/pixelshift.rs` follows `make_arq.py` from
https://github.com/agriggio/make_arq at commit
`912069a8e6d3c6738cd76ba0fe6c62ed4d314fe3`.

Copyright (C) 2018–2020 Alberto Griggio <alberto.griggio@gmail.com>.
Licensed under GNU General Public License version 3 or later; see the root
`LICENSE`. Reconstruction, validation and motion fallback are independently
implemented in Rust. No upstream Python or C++ is part of the build.

To refresh: review `get_sony_frame_data` and `get_frames` at the new upstream
revision, verify against synthetic physical sensor samples and real capture
groups, and update the commit/date in `docs/PORTS.md` and this notice together.
