//! Develop's pinned plain-Gaussian policy, shared by its CPU consumers and GPU planner.
//! Raster/editor callers retain their sampled default. This selects the existing box
//! engine; it does not copy kernels or change the separate fine/eigen-guided filters.
pub use lightcraft_raster::blur::GaussianPlan;
use lightcraft_raster::{Image, blur::GaussianPolicy, resample::Pixel};

pub const POLICY: GaussianPolicy = GaussianPolicy::Boxes;

pub fn plan(sigma: f32) -> GaussianPlan {
    POLICY.plan(sigma)
}

pub fn gaussian<T: Pixel>(image: &Image<T>, sigma: f32) -> Image<T> {
    lightcraft_raster::blur::gaussian_with_policy(image, sigma, POLICY)
}
