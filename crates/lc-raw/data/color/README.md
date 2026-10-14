# Spectral illuminant colour matching data

The complete, unmodified CIE 1931 2° observer CSV and upstream metadata are from
CIE 018:2019, Table 6 (360–830 nm, 1 nm samples), DOI
https://doi.org/10.25039/CIE.DS.xvudnb9b. Attribution: CIE 2019,
Colour-matching functions of CIE 1931 standard colorimetric observer,
International Commission on Illumination (CIE), Vienna, AT.

License: Creative Commons Attribution-ShareAlike 4.0 International
(https://creativecommons.org/licenses/by-sa/4.0/), as recorded in the unchanged
metadata. This data file is a separate CC BY-SA data work; application Rust
code remains GPL-3.0-or-later.

Pin: CIE's 2019 dataset, CSV SHA-256
`fa663e3535a7e0763a745993a1f0a192eb0275ac46ad2d1befd7626841e713c1`.
Downloaded 2026-10-14. To update, download **both whole files** from
https://cie.co.at/datatable/cie-1931-colour-matching-functions-2-degree-observer
(the linked URLs are under `https://files.cie.co.at/Publications-datasets/`),
verify the CSV checksum against the metadata, and update this pin and PORTS.md.
Do not crop, edit or regenerate the upstream CSV.

DNG IlluminantData spectral whites follow the Adobe DNG SDK's piecewise-linear
spectral interpolation, endpoint extension and column-normalized integration.
The data are parsed once; profiles store their derived xy white, not a copy of
an Adobe profile.
