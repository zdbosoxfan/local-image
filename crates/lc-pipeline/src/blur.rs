//! Develop's pinned plain-Gaussian policy, shared by its CPU consumers and GPU planner.
//! Raster/editor callers retain their sampled default. This selects the existing box
//! engine; it does not copy kernels or change the separate fine/eigen-guided filters.
use li_raster::Frame;
use li_raster::blur::{GaussianPlan, GaussianPolicy};
use li_raster::resample::Pixel;

pub const POLICY: GaussianPolicy = GaussianPolicy::Boxes;

pub fn plan(sigma: f32) -> GaussianPlan {
    POLICY.plan(sigma)
}

pub fn gaussian<T: Pixel>(image: &Frame<T>, sigma: f32) -> Frame<T> {
    image.gaussian_with_policy(sigma, POLICY)
}
