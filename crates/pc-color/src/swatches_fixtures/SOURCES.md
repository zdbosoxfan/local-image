Original GPL-3.0-or-later Local Image synthetic fixtures, 2026-10-13.
ACO assembled independently with Python struct.pack (big endian); GPL and KPL XML
hand-authored against the published formats. They contain no proprietary books.

Format oracles (consulted, no source code copied):
- Adobe Photoshop File Formats, Color swatches (ACO): https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/
- ASE: existing ../ase/SOURCES.md and https://www.selapa.net/swatches/colors/fileformats.php
- GIMP GPL reader: https://github.com/GNOME/gimp/blob/master/app/core/gimppalette-load.c
- Krita KPL ZIP/schema: https://docs.krita.org/en/general_concepts/file_formats/file_kpl.html
- KoColorSet saveKpl/loadKplGroup: https://github.com/KDE/krita/blob/master/libs/pigment/resources/KoColorSet.cpp

No dependency additions. KPL container uses the app's existing bounded pure-Rust
photocraft-format ZIP reader (STORE/DEFLATE and CRC); pc-color owns colorset.xml.
