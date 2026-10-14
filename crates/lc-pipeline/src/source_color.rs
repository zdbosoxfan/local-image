//! Fitted camera colour look evaluated after camera-plane lens resampling.
use lightcraft_color::profile::HsvTable;
use lightcraft_color::{D50, D65, Mat3, PROPHOTO, REC2020, bradford, luminance_2020};

fn to_prophoto() -> Mat3 {
    PROPHOTO.from_xyz().mul(&bradford(D65, D50)).mul(&REC2020.to_xyz())
}

/// A fitted camera hue/saturation table, ready to apply to linear Rec.2020 pixels. It changes
/// hue and saturation only: luminance is restored, the camera tone curve owns it.
pub struct HueSat<'a> {
    table: &'a HsvTable,
    to: [[f32; 3]; 3],
    from: [[f32; 3]; 3],
}

impl<'a> HueSat<'a> {
    pub fn new(table: &'a HsvTable) -> Option<HueSat<'a>> {
        let to = to_prophoto();
        Some(HueSat { table, to: to.to_f32(), from: to.inverse()?.to_f32() })
    }

    #[inline]
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let mul = |m: &[[f32; 3]; 3], v: [f32; 3]| -> [f32; 3] { std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2]) };
        let out = mul(&self.from, self.table.apply(mul(&self.to, rgb)));
        let (before, after) = (luminance_2020(rgb), luminance_2020(out));
        if before > 0.0 && after > 0.0 && out.iter().all(|v| v.is_finite()) { out.map(|v| v * before / after) } else { rgb }
    }
}
