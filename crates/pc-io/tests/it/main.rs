//! Single integration-test binary for this crate (one link instead of one per file).
mod adjust_blend_roundtrip;
mod annotations;
mod artboard_guide_order;
mod banded;
mod cancel;
mod channels;
mod color;
mod common;
mod composite;
mod comps_artboards;
#[cfg(feature = "corpus")]
mod corpus;
mod corpus_regressions;
mod develop_layer_psd;
mod effects_multi;
mod flat;
#[cfg(all(feature = "corpus", feature = "heif"))]
mod heif;
#[cfg(not(feature = "heif"))]
mod heif_unsupported;
mod lab16;
mod large_psb;
mod modes;
mod orientation;
mod psd32_export;
mod psd_block_layout;
mod psd_kerning;
mod psd_structure;
mod psd_type_bounds;
mod psd_vertical_type;
mod raw;
mod roundtrip;
#[cfg(feature = "corpus")]
mod smart_corpus;
mod text_corpus;
mod text_styles;
#[cfg(feature = "corpus")]
mod tiff_corpus;
mod tiff_layers;
mod tiff_pages;
mod vector;
mod xmp_export;
