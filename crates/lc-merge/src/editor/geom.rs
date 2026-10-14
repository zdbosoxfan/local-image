//! Integer-centred homography for editor layer placement, over `lightcraft_geom::Homography`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homography(pub [f64; 9]);
impl Homography {
    pub const IDENTITY: Self = Self(lightcraft_geom::Homography::IDENTITY.0);
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let p = lightcraft_geom::Homography(self.0).apply(lightcraft_geom::Point::new(x, y));
        (p.x, p.y)
    }
    pub fn mul(&self, other: &Self) -> Self {
        Self(lightcraft_geom::Homography(self.0).mul(&lightcraft_geom::Homography(other.0)).0)
    }
    pub fn inverse(&self) -> Option<Self> {
        lightcraft_geom::Homography(self.0).inverse().map(|h| Self(h.0))
    }
}
