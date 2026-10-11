//! Keep photo-bound edits attached to content through a quarter turn.
use crate::{DevelopSettings, Geometry, MaskShape, SegMask};
use lightcraft_geom::{Homography, Orientation, Point, Rect};

fn point(p: &mut Point, turn: Orientation) {
    let (x, y) = turn.map(p.x, p.y, 1., 1.);
    *p = Point::new(x, y);
}
fn segmentation(seg: &mut SegMask, turn: Orientation) {
    let Some(values) = seg.logits() else { return };
    let side = seg.side as usize;
    let mut rotated = vec![0.; values.len()];
    for y in 0..side {
        for x in 0..side {
            let (nx, ny) = turn.map(x as f64 + 0.5, y as f64 + 0.5, side as f64, side as f64);
            rotated[ny.floor() as usize * side + nx.floor() as usize] = values[y * side + x];
        }
    }
    let bounds = seg.rect.map(|[x0, y0, x1, y1]| turn.map_norm_rect(Rect::new(x0, y0, x1, y1)));
    *seg = match bounds {
        Some(r) => SegMask::from_logits_in(side, &rotated, [r.x0, r.y0, r.x1, r.y1]),
        None => SegMask::from_logits(side, &rotated),
    };
}

// Conjugate the manual virtual-camera correction by the display quarter turn. The
// keystone sliders compose Y then X rotations, so swapping them alone changes the
// result when both are nonzero; decompose the rotated camera into that same order.
fn geometry(g: &mut Geometry, clockwise: bool) {
    let direction = if clockwise { -1. } else { 1. };
    let (sa, ca) = (direction * g.horizontal / 200.).sin_cos();
    let (sb, cb) = (direction * g.vertical / 200.).sin_cos();
    g.horizontal = (ca * sb).clamp(-1., 1.).asin() * 200.;
    g.vertical = -sa.atan2(ca * cb) * 200.;
    g.rotate += (sa * sb).atan2(cb).to_degrees();
    g.aspect = -g.aspect;
    let (x, y) = (g.offset_x, g.offset_y);
    (g.offset_x, g.offset_y) = if clockwise { (y, -x) } else { (-y, x) };
    if let Some(h) = &mut g.upright_transform {
        let sign = if clockwise { 1. } else { -1. };
        let turn = Homography([0., -sign, 0., sign, 0., 0., 0., 0., 1.]);
        let inverse = Homography([0., sign, 0., -sign, 0., 0., 0., 0., 1.]);
        *h = turn.mul(h).mul(&inverse);
    }
}

impl DevelopSettings {
    pub fn rotate_content(&mut self, clockwise: bool) {
        let turn = if clockwise { Orientation::Rotate90 } else { Orientation::Rotate270 };
        self.orientation = self.orientation.rotated(clockwise);
        geometry(&mut self.geometry, clockwise);
        self.crop.geometry.rect = turn.map_norm_rect(self.crop.geometry.rect);
        std::mem::swap(&mut self.crop.flip_h, &mut self.crop.flip_v);
        if let Some((w, h)) = &mut self.crop.aspect {
            std::mem::swap(w, h);
        }
        for (a, b) in &mut self.geometry.guides {
            point(a, turn);
            point(b, turn);
        }
        for mask in &mut self.masks {
            for component in &mut mask.components {
                match &mut component.shape {
                    MaskShape::Linear { start, end } => {
                        point(start, turn);
                        point(end, turn);
                    }
                    MaskShape::Radial { center, angle, .. } => {
                        point(center, turn);
                        *angle = (*angle + if clockwise { 90. } else { -90. }).rem_euclid(360.);
                    }
                    MaskShape::Brush { strokes } => {
                        for stroke in strokes {
                            for p in &mut stroke.points {
                                point(p, turn);
                            }
                        }
                    }
                    MaskShape::Object { hint, exclude, seg, detail, .. } => {
                        for p in hint.iter_mut().chain(exclude) {
                            point(p, turn);
                        }
                        if let Some(seg) = seg {
                            segmentation(seg, turn);
                        }
                        for seg in detail {
                            segmentation(seg, turn);
                        }
                    }
                    MaskShape::Prompt { seg, detail, .. } => {
                        if let Some(seg) = seg {
                            segmentation(seg, turn);
                        }
                        for seg in detail {
                            segmentation(seg, turn);
                        }
                    }
                    MaskShape::Subject { seg } | MaskShape::Sky { seg } | MaskShape::Background { seg } | MaskShape::DepthRange { seg, .. } => {
                        if let Some(seg) = seg {
                            segmentation(seg, turn);
                        }
                    }
                    _ => {}
                }
            }
        }
        for spot in &mut self.spots {
            for p in spot.points.iter_mut().chain(&mut spot.polygon) {
                point(p, turn);
            }
            if let Some(offset) = &mut spot.source_offset {
                *offset = if clockwise { Point::new(-offset.y, offset.x) } else { Point::new(offset.y, -offset.x) };
            }
            if let Some(patch) = &mut spot.patch {
                let [x0, y0, x1, y1] = patch.rect;
                let r = turn.map_norm_rect(Rect::new(x0, y0, x1, y1));
                patch.rect = [r.x0, r.y0, r.x1, r.y1];
                patch.orientation = patch.orientation.rotated(clockwise);
            }
        }
        for eye in &mut self.red_eye {
            point(&mut eye.center, turn);
            std::mem::swap(&mut eye.rx, &mut eye.ry);
            if let Some(p) = &mut eye.catchlight {
                *p = if clockwise { Point::new(-p.y, p.x) } else { Point::new(p.y, -p.x) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BrushStroke, Mask, MaskComponent, MaskOp, RedEye};
    #[test]
    fn bugfix_develop_rotation_maps_selections_and_red_eye() {
        let mut settings = DevelopSettings::default();
        let seg = SegMask::from_logits_in(2, &[1., 2., 3., 4.], [0.1, 0.2, 0.7, 0.8]);
        let shapes = vec![
            MaskShape::Subject { seg: Some(seg) },
            MaskShape::Linear { start: Point::new(0.1, 0.2), end: Point::new(0.3, 0.4) },
            MaskShape::Brush { strokes: vec![BrushStroke { points: vec![Point::new(0.2, 0.3)], ..Default::default() }] },
        ];
        settings.masks = vec![Mask {
            components: shapes.into_iter().map(|shape| MaskComponent { name: None, op: MaskOp::Add, invert: false, shape }).collect(),
            ..Default::default()
        }];
        settings.red_eye.push(RedEye { center: Point::new(0.2, 0.3), rx: 0.1, ry: 0.2, ..Default::default() });
        settings.rotate_content(true);
        let MaskShape::Subject { seg: Some(seg) } = &settings.masks[0].components[0].shape else { panic!() };
        assert_eq!(seg.logits().unwrap(), vec![3., 1., 4., 2.]);
        let r = seg.rect.unwrap();
        assert!((r[0] - 0.2).abs() < 1e-9 && (r[1] - 0.1).abs() < 1e-9);
        let MaskShape::Brush { strokes } = &settings.masks[0].components[2].shape else { panic!() };
        assert_eq!(strokes[0].points[0], Point::new(0.7, 0.2));
        assert_eq!(settings.red_eye[0].center, Point::new(0.7, 0.2));
        assert_eq!((settings.red_eye[0].rx, settings.red_eye[0].ry), (0.2, 0.1));
    }
}
