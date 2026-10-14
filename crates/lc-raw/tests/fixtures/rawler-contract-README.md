# rawler version-bump vectors

`rawler-contract.tsv` records 22 source-pinned CC0 raw.pixls.us files: 20 adapter samples and
native Nikon D750 / Sony ILCE-7M3 baselines. `scripts/rawdecode_samples.py` downloads them under
`target/rawdecode` and verifies the source SHA-256. Large camera files are not checked into Git.

The complete integer sensor fingerprint is FNV-1a-like **over u16 values** (not bytes): start at
`0xcbf29ce484222325`, then `hash = (hash XOR sample) * 0x100000001b3`, wrapping to 64 bits.
Geometry, sensor-origin CFA, active area, active-relative crop, black/white, WB and orientation are
also pinned. Float WB tolerates 1e-6; sensor fingerprints are exact on every architecture.

All 20 new adapter sensors match the independent **system LibRaw 0.22.2** unpacked raw buffer
fingerprint exactly, including C-RAW, dual-pixel, Fuji lossy X-T4, and compressed Bayer GFX 50S.
The D750 native fingerprint also matches. The native Sony decoder's existing ARW2 linearization
has a different fingerprint from LibRaw; its pre-existing output is locked, not substituted.

Mapping goldens follow rawler metadata. LibRaw uses different crops/white levels on several bodies;
these are not sensor mismatches. Notably rawler supplies Canon specular whites (14008 / 12735) and
ORF white 4000, versus LibRaw's nominal 16383 / 4095, and it includes Fuji's black levels. `bits`
is rawler's informational storage depth (16 in these adapter samples); the codec payloads here
are 14-bit CRX/RAF and 12-bit ORF. X-T20 header metadata is corrected at the boundary using
rawler's public compressed-mode camera record.

Independent harness source, fetched LibRaw 0.22.2 headers, executables and logs are retained in
`target/refvec` and `target/rawdecode`; no C is a build dependency. Use `docs/RAW-DECODERS.md`
for the exact update procedure. A failing golden requires investigation, not blind regeneration.
