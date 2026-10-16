# LibRaw Sony codec references and ARW6 Rust port

The Sony ARW6 algorithm in `crates/lc-raw/src/vendor/arw6.rs` follows the complete
`src/decoders/sony_arw6.cpp` at LibRaw commit
`830ac963b40223c6a25add5be65036fa8d56e2d6`. Its complete inverse table is preserved.
Copyright 2026 LibRaw LLC and Jiang Yaogeng. Sony SR2 calibration cipher, ARQ
sample layout and RGB conversion follow LibRaw 0.22.2, copyright LibRaw LLC
and the upstream contributors named in those files.

LibRaw offers LGPL version 2.1 or CDDL version 1.0. This project chooses LGPL
2.1, compatible with our GPL-3.0-or-later program. The unmodified upstream
LGPL text is `libraw-LICENSE.LGPL`. There is no linked LibRaw, C/C++ source or
new C/-sys dependency in the application. External reference builds stay in
`target/refvec/` and are never Cargo dependencies.

Update: pin a new upstream commit in `docs/PORTS.md`, inspect the entire Sony
codec file/table changes, update the Rust implementation and this notice,
compare normal/HQ CC0 sensor buffers after black subtraction to that pinned
independent reference, run only the new focused codec/guard tests and strict
clippy under the owner's current testing rule, then regenerate attributions.
Never change a reference golden merely to accept a disagreement.
