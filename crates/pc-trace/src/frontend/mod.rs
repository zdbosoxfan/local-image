mod binary;
mod color_cluster;
mod keying;
mod watershed;
use crate::{error::Error, ir::Segmentation, vc::ColorImage};
pub use binary::{BinaryFrontend, Threshold};
pub use color_cluster::ColorClusterFrontend;
pub use watershed::WatershedFrontend;
pub trait Frontend {
    fn segment(&self, image: &ColorImage) -> Result<Segmentation, Error>;
    fn segment_cancellable(&self, image: &ColorImage, cancel: &impl Fn() -> bool) -> Result<Segmentation, Error> {
        if cancel() {
            return Err(Error::Cancelled);
        }
        let result = self.segment(image)?;
        if cancel() {
            return Err(Error::Cancelled);
        }
        Ok(result)
    }
}
