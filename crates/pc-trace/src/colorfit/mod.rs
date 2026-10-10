pub mod oklab;
mod palette;
mod quantize;
pub use palette::FixedPalette;
pub use quantize::AutoQuantize;
pub trait ColorFitter {
    fn fit(&self, seg: &mut crate::ir::Segmentation);
}
