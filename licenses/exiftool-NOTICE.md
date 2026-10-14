# ExifTool metadata reference

Sony maker-note field layouts/cipher and DJI XMP namespace semantics follow
ExifTool snapshot 13.59, commit `2200871d9cef988051d2a99d67df3bda6cbb30a8`,
with the earlier 13.40 reference `e2a8542f9c900eae70cb016f0ed410c1bfd4c1ac`
retained to document changed layouts,
`lib/Image/ExifTool/Sony.pm` and `lib/Image/ExifTool/DJI.pm`.
Copyright 2003–2025 Phil Harvey (philharvey66 at gmail.com).
Upstream permits the same terms as Perl itself: GPL version 1 or later, or
the Artistic License. This GPL-3.0-or-later project chooses the GPL option;
see the root `LICENSE`. The Rust parser is independently implemented;
ExifTool/Perl is used only as an external reference tool under ignored target/.
No upstream camera/profile data file or runtime dependency is imported.

Update: pin the new release/full commit in `docs/PORTS.md`, review whole
Sony.pm/DJI.pm/Exif.pm field-layout changes and model discriminators, preserve unknown
layouts as unknown, compare the new focused metadata fixtures and CC0 fields
against the complete pinned ExifTool, and regenerate app attributions.
