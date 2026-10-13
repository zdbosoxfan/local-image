# FIXUP fu-psd (PSD, PDF and text format fix-ups)

Branch wip/fu-psd, based on claude/v2-continue (a2d123f7). All five failures reproduced on this base
before any change. Packages touched: photocraft-io, photocraft-pdf, photocraft-text.

## Items

### 1. Odd-length PSD layer tagged blocks: pad inside the length. FIXED (code was wrong)

- **Photoshop behaviour.** The Adobe PSD spec gives the additional-layer-info length as "rounded up
  to an even byte count", so the pad belongs inside the length. Upstream e2aa1934 checked this
  against 567 Photoshop-authored fixtures: Photoshop pads every layer-level tagged block to a
  multiple of 4, inside the length. psd-tools reads layer blocks with no pad skip, so a pad written
  outside the length misaligns every later block (#200). ag-psd writes the pad outside but skips it
  on read, so it still reads pad-inside files, as it must to read Photoshop's own files. Upstream's
  current writer (`crates/io/src/psd_export.rs` ~455) pushes the pad into the data for every odd
  block, preserved blocks included.
- **Diagnosis.** FC009 (55c6dd34) added an exception: an odd preserved block identical to its source
  bytes got `padding = Some([0])`, which puts the pad outside the length. That broke both tests and
  the psd-tools/Photoshop layout.
- **Fix.** crates/pc-io/src/psd_export.rs: always pad odd layer blocks inside the length, with a
  comment explaining the preserved case. Photoshop-authored blocks are never odd, so they still
  re-save byte for byte. A third-party odd block keeps the same bytes on disk (payload, then its pad
  byte); only the length now covers the pad. FC009's little-endian opaque TIFF envelope (`liOP`)
  carries its own payload length, so that payload stays exact.
- **Tests.** `psd_block_layout::odd_layer_blocks_carry_their_pad_inside_the_length` and
  `roundtrip::raster_layer_blocks_preserved` now pass unchanged.
- **TEST-CHANGED:** `tiff_layers::private_layer_and_global_blocks_keep_bytes_and_source_order_over_two_tiff_saves`
  (FC009). Its big-endian raw `ZzLy` block (7 bytes, pad outside in the source) now reads back as 8
  bytes after a save. The test now asserts:
  - the source holds `ZzLy`, length 7, payload, 0;
  - each saved TIFF holds `ZzLy`, length 8, the same payload and 0, so the bytes are identical and
    only the length changes;
  - the model is stable over both saves;
  - the little-endian envelope payload is still exactly 7 bytes.

  Nothing was weakened: the original exact-payload check is still there for the first import and
  for the LE path.

### 2. vector_fallback F03 (shape with a separate vector mask). FIXED (test was stale)

- **Diagnosis.** 745a5797 (Oct 11 10:45) wrote this test when a shape with an independent vector
  mask always reopened as pixels. FC009 (55c6dd34, Oct 11 20:50, later) deliberately attaches a
  private `liED` record bound by a fingerprint to the exact pixels and blocks. Local Image then
  restores the editable shape on reopen, and other readers still see plain pixels.
  `contracts_psd::private_editable_shape_is_stale_after_external_pixel_or_block_edits` asserts
  that same Shape-on-reopen behaviour. The loss notice stays correct, because Photoshop and other
  readers do lose editability.
- **TEST-CHANGED:** crates/pc-io/tests/vector_fallback/roundtrip.rs. It replaces "reopened layer is
  Raster" with stronger checks:
  - the PSD record has rendered colour channel data and no `vmsk`/`SoCo` (already asserted);
  - the record carries `liED`;
  - the reopened layer is the Shape with the original path and vector mask.

  All the other assertions are unchanged: pixel match within 1/255, the loss notice, the second
  save, and the source left intact.

### 3. heif_unsupported under feature unification. FIXED (test gate was wrong)

- **Diagnosis.** The test is gated on `photocraft-io/heif`. `local-image/heif` turns on
  `photocraft-codecs/heif` directly, so the test binary has a decoder while the pc-io feature is
  off.
- **Fix.** crates/pc-io/tests/it/heif_unsupported.rs now asks the codecs crate at runtime
  (`photocraft_codecs::caps(Format::Heif).read`):
  - without HEIF it still asserts "isn't included in this build" for all three names;
  - with HEIF unified in, it asserts that message is *not* shown.

  The `#![cfg(not(feature = "heif"))]` gate stays. Checked both ways: plain run, and
  `--features photocraft-codecs/heif` (which reproduces the release-gate unification). Both pass.

### 4. PDF nested independent blends (`groups::pdf_groups_nested_independent_blends_do_not_reuse_outer_backdrop_coverage`). FIXED (test oracle was wrong)

- **Diagnosis.** Only the `/I false` nested case failed (worst 0.0355 > 0.025, mean was 0.0078),
  everywhere inside the nested group Y. I worked the ISO 32000 group-compositing formulas (11.4.8)
  by hand for the fixture. Y nonisolated inside nonisolated X starts from X's running state, which
  includes X's backdrop. At (10,10) that gives alpha 0.544 and RGB-on-white 0.612/0.675/0.737.
  - pc-compose gives exactly this (0.6115/0.6767/0.7383).
  - Ghostscript 10.06 (pdf14, spec-conforming), run on the same PDF, gives 0.612/0.675/0.737.
  - Poppler Splash 26.01 gives 0.647/0.698/0.749. That is bit-for-bit the same as its `/I true`
    render: Splash starts a nested nonisolated group from the outer group's own empty coverage, so
    it draws it as isolated.

  So the compositor (f63fb309 pdf_group.rs) is correct, and Splash is no oracle for this one case.
- **TEST-CHANGED:** crates/pc-pdf/tests/import/groups.rs. Adds `spec_nested`, a closed-form ISO
  32000 compositor for this fixture. For both isolation values, all 49 samples must match it to
  within 0.002, which is tighter than the old 0.025 Splash tolerance. The `/I true` case still also
  runs the Splash oracle (Splash and spec agree there). For `/I false` the test also asserts that
  the spec result differs from the isolated one by more than 0.03, so the case still tests
  something. No pc-compose change is needed.

### 5. on_path bidi/ligature/vertical. FIXED (code was wrong)

- **Diagnosis.** On a right-aligned 1000-unit line, the last Arabic cluster was marked invisible
  (overflow): `d + width = 1000.0000016784668` against `available + 1e-6`. Line widths were summed
  in f32 (`c.x + c.advance`, `end - l.x0`) but cluster distances in mixed f32/f64. The f32
  rounding (about 4e-6 at this size) is bigger than the 1e-6 tolerance, so a cluster that fits
  exactly was hidden.
- **Fix.** crates/pc-text/src/on_path.rs: line extents and cluster offsets are now computed in f64
  from the f32 metrics. The test passes unchanged, and all 9 on_path tests pass.

### 6. Upstream 6f1c4481 leftover: TySh trailing bounds as f32. PORTED

- crates/pc-text/src/psd.rs: `TySh.bounds` is now `[f32; 4]`, read and written as big-endian
  f32. `build_tysh` writes zeros for unwarped type and the text-space bounds when warped (not
  `warpNone`), which is what Photoshop writes.
- Ported upstream's test `tests::tysh_trailing_bounds_are_f32` into crates/pc-text/src/tests.rs.
- crates/pc-text/tests/coverage/ds_psd_coverage.rs: literals are now f32.
- crates/pc-io/tests/it/psd_type_bounds.rs: writes `[0.0; 4]`, as upstream does.
- **TEST-CHANGED:** ds_psd_coverage `build_tysh_point_and_box_shape`
  expected the old integer floor/ceil of the ink, [0,0,50,20]. It now expects [0.0; 4] for unwarped
  type, which is Photoshop's bytes. The ink still goes in the `bounds`/`boundingBox` descriptors.

### 7. Upstream 6547ca0b leftover: DisplayInfo alpha import. SKIPPED (patch ready, NEEDS-OTHER-OWNER)

My side cannot be correct alone. Without the pc-compose change, an alpha-type channel of a
Multichannel document stops printing, which changes the composite. That is worse with the old 1007
resource, where every record is kind 0/1, so the whole image would go blank. I prepared and ran the
pc-io half (new unit test passed, both existing Multichannel it tests passed), then reverted it.
Apply both patches together.

pc-io half (crates/pc-io/src/channel_map.rs):

```diff
diff --git a/crates/pc-io/src/channel_map.rs b/crates/pc-io/src/channel_map.rs
index f8dada8b..bb6cfcb7 100644
--- a/crates/pc-io/src/channel_map.rs
+++ b/crates/pc-io/src/channel_map.rs
@@ -50,6 +50,8 @@ pub(crate) fn apply_display_info(data: &[u8], versioned: bool, channels: &mut [A
         match rec[12] {
             2 => c.spot = Some((color, opacity)),
             k => {
+                // An alpha channel, not an ink (a Multichannel import starts every channel as one).
+                c.spot = None;
                 c.color = color;
                 c.opacity = opacity;
                 c.indicates = if k == 0 { ColorIndicates::SelectedAreas } else { ColorIndicates::MaskedAreas };
@@ -111,6 +113,27 @@ mod tests {
         assert_eq!(c[0].opacity, 0.5);
     }
 
+    /// Upstream 6547ca0b: DisplayInfo kinds 0/1 make a channel an alpha channel even when the
+    /// importer started it as an ink (Multichannel); kind 2 keeps it a spot ink.
+    #[test]
+    fn alpha_kinds_clear_a_provisional_ink() {
+        let mut a = AlphaChannel::new("A", Surface::new(PixelFormat::GRAY8));
+        a.color = Color::rgb(1.0, 0.0, 0.0);
+        a.opacity = 0.5;
+        a.indicates = ColorIndicates::MaskedAreas;
+        let mut s = AlphaChannel::new("S", Surface::new(PixelFormat::GRAY8));
+        s.spot = Some((Color::rgb(0.0, 1.0, 1.0), 0.0));
+        let bytes = display_info(&[&s, &a]);
+        let ink = |name: &str| AlphaChannel { spot: Some((Color::BLACK, 0.0)), ..AlphaChannel::new(name, Surface::new(PixelFormat::GRAY8)) };
+        let mut back = vec![ink("S"), ink("A")];
+        apply_display_info(&bytes, true, &mut back);
+        assert_eq!(back[0].spot, s.spot);
+        assert_eq!(back[1].spot, None, "an alpha-type channel is not a spot ink");
+        assert_eq!(back[1].color.to_rgb(), [1.0, 0.0, 0.0]);
+        assert!((back[1].opacity - 0.5).abs() < 1e-6);
+        assert_eq!(back[1].indicates, ColorIndicates::MaskedAreas);
+    }
+
     #[test]
     fn decodes_other_spaces() {
         let g = decode_color(SPACE_GRAY, [10000, 0, 0, 0]);
```

pc-compose half (crates/pc-compose/src/multichannel.rs, `inks`), exactly as upstream:

```diff
-//! how Photoshop previews spot channels.
+//! how Photoshop previews spot channels. An alpha-type channel of a Multichannel document
+//! (selected / masked areas) still prints, as a transparent black ink, like Photoshop shows it.
@@ pub fn inks(doc: &Document, rect: Rect) -> Buffer {
     for ch in &doc.channels {
-        let Some((ink, solidity)) = ch.spot else { continue };
+        let (ink, solidity) = ch.spot.unwrap_or((photocraft_color::Color::BLACK, 0.0));
```

The test in multichannel.rs `mod tests` should replace "Alpha channels and other modes are not inks." with:

```rust
        // An alpha-type channel prints as a black ink.
        let mut k = Surface::new(fmt);
        k.write_region(Rect::new(0, 0, 4, 1), &[0.5, 0.0, 0.0, 0.0]);
        doc.channels.push(AlphaChannel::new("Alpha", k));
        let r = crate::flatten(&doc);
        // 0.5 is stored as 128/255 in an 8-bit channel: within one 8-bit step.
        assert!((r.px[0][0] - 0.5).abs() <= 1.0 / 255.0 && (r.px[0][1] - 0.5).abs() <= 1.0 / 255.0, "{:?}", r.px[0]);
        assert_eq!(r.px[1], [0.0, 1.0, 1.0, 1.0]);
        // Other modes have no inks.
```

`inks` is only reached for Multichannel documents (`multichannel::backdrop`), so alpha channels in
RGB/CMYK documents are unaffected.

Related upstream fix not in our tree: **78c61100** ("13-byte DisplayInfo"). Our `display_info` and
`apply_display_info` still use 14-byte records for 1077. Photoshop and upstream use 13 bytes for
1077 (14 only for the old 1007). Upstream's 6547ca0b integration test (`alpha_type_channels_import_as_alpha_and_round_trip`,
with 13-byte records) therefore can't be ported until 78c61100 is. It belongs in the same follow-up.

## Ported upstream shas (for docs/PORTS.md)

- 6f1c4481 (TySh f32 bounds remainder): crates/pc-text/src/psd.rs, src/tests.rs, tests/coverage/ds_psd_coverage.rs, crates/pc-io/tests/it/psd_type_bounds.rs.
- e2aa1934 / #200 layout (already ported; this applies its pad-inside rule to preserved odd blocks too): crates/pc-io/src/psd_export.rs.

## Needs from other owners

- pc-compose `src/multichannel.rs` + pc-io `channel_map.rs`: item 7. Apply both patches above in one change.
- (Follow-up) port upstream 78c61100 (13-byte DisplayInfo 1077) to pc-io channel_map.rs / multichannel_map.rs / psd_export.rs / psd_import.rs.

## Final checks

- `cargo fmt` on photocraft-io, photocraft-pdf, photocraft-text.
- `cargo clippy --offline -p <pkg> --all-targets -- -D warnings` clean for all three.
- Tests, each run once, all passing:
  - photocraft-text lib: 98 passed, 3 ignored
  - photocraft-text on_path: 9
  - photocraft-text coverage: 76
  - photocraft-pdf lib: 27
  - photocraft-pdf import: 64
  - photocraft-io lib: 130 passed, 1 ignored
  - photocraft-io it: 222 passed, 1 ignored
- Also run along the way: photocraft-io contracts_psd (14 passed).

## TEST-CHANGED summary

1. pc-io tests/it/tiff_layers.rs `private_layer_and_global_blocks_keep_bytes_and_source_order_over_two_tiff_saves`: big-endian raw odd block gains its pad inside the length after a save; same bytes on disk asserted (item 1).
2. pc-io tests/vector_fallback/roundtrip.rs F03: Local Image reopens the editable Shape through `liED`; other readers' pixels asserted (item 2).
3. pc-io tests/it/heif_unsupported.rs: runtime capability check instead of compile-time-only gating (item 3).
4. pc-pdf tests/import/groups.rs nested test: closed-form ISO 32000 oracle; Splash kept for the isolated case only (item 4).
5. pc-text tests/coverage/ds_psd_coverage.rs: unwarped TySh trailing bounds are [0.0; 4] (item 6).
