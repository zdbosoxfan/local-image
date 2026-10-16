//! Shared document-to-view presentation mapping. Camera changes never edit a document.
//!
//! The basis reflects document x, scales, then rotates clockwise in screen coordinates.
//! Upright GPU views retain the existing whole-device-pixel origin; rotated views stay
//! continuous. Consumers use the returned device mapping for both uniforms and culling.

use egui::{Color32, Mesh, Pos2, Rect, TextureId, Vec2, pos2, vec2};

/// An affine camera in either logical points or device pixels.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    origin: Pos2,
    anchor: Pos2,
    center: [f32; 2],
    scale: f32,
    direction: f32,
    sin: f32,
    cos: f32,
    device: bool,
}

/// Normalize a presentation angle to [-π, π]; invalid values display upright.
/// Command/UI boundaries should reject invalid input rather than storing it.
pub fn normalized_angle(radians: f32) -> f32 {
    if !radians.is_finite() || radians == 0.0 {
        return 0.0;
    }
    let a = radians % std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a - std::f32::consts::TAU
    } else if a < -std::f32::consts::PI {
        a + std::f32::consts::TAU
    } else {
        a
    }
}

impl Camera {
    /// Map a document center to the center of a logical canvas rectangle.
    pub fn new(rect: Rect, zoom: f32, center: [f32; 2], flip: bool, radians: f32) -> Self {
        let scale = if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 };
        let center = center.map(|v| if v.is_finite() { v } else { 0.0 });
        let angle = normalized_angle(radians);
        let (sin, cos) = if angle == 0.0 { (0.0, 1.0) } else { angle.sin_cos() };
        let mut camera = Self { origin: Pos2::ZERO, anchor: Pos2::ZERO, center, scale, direction: if flip { -1.0 } else { 1.0 }, sin, cos, device: false };
        let anchor = rect.center();
        let anchor = if anchor.x.is_finite() && anchor.y.is_finite() { anchor } else { Pos2::ZERO };
        camera.anchor = anchor;
        camera.origin = anchor - camera.screen_delta(vec2(center[0], center[1]));
        camera
    }

    /// Device-pixel counterpart of this mapping, retaining legacy upright pixel alignment.
    pub fn device_pixels(self, pixels_per_point: f32) -> Self {
        let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 { pixels_per_point } else { 1.0 };
        // Recompute at device scale in the same order as legacy GPU placement, instead
        // of multiplying an already-subtracted logical origin across a rounding boundary.
        let mut camera = Self { anchor: self.anchor * ppp, scale: self.scale * ppp, device: true, ..self };
        camera.origin = camera.anchor - camera.screen_delta(vec2(self.center[0], self.center[1]));
        if self.sin == 0.0 && self.cos == 1.0 {
            camera.origin = pos2(camera.origin.x.round(), camera.origin.y.round());
            camera.anchor = camera.origin + camera.screen_delta(vec2(self.center[0], self.center[1]));
        }
        camera
    }

    /// Screen position of document (0,0), in this mapping's units.
    pub fn origin(self) -> Pos2 {
        self.origin
    }
    /// Screen units per document pixel.
    pub fn scale(self) -> f32 {
        self.scale
    }
    /// Normalized rotation's sine and cosine, in shader uniform order.
    pub fn sin_cos(self) -> [f32; 2] {
        [self.sin, self.cos]
    }

    /// Document displacement to screen displacement (without translation).
    pub fn screen_delta(self, d: Vec2) -> Vec2 {
        let d = vec2(d.x * self.direction, d.y) * self.scale;
        vec2(self.cos * d.x - self.sin * d.y, self.sin * d.x + self.cos * d.y)
    }
    /// Screen displacement to document displacement (without translation).
    pub fn document_delta(self, d: Vec2) -> Vec2 {
        let d = d / self.scale;
        vec2((self.cos * d.x + self.sin * d.y) * self.direction, -self.sin * d.x + self.cos * d.y)
    }
    /// Document point to screen point.
    pub fn to_screen(self, p: Pos2) -> Pos2 {
        if self.device {
            // Match the shader's origin-relative arithmetic (including upright rounding).
            self.origin + self.screen_delta(p.to_vec2())
        } else {
            // Preserve the existing logical ViewXform's center-relative arithmetic.
            self.anchor + self.screen_delta(p - pos2(self.center[0], self.center[1]))
        }
    }
    /// Screen point to document point; the exact inverse basis of `to_screen`.
    pub fn to_document(self, p: Pos2) -> Pos2 {
        if self.device { self.document_delta(p - self.origin).to_pos2() } else { pos2(self.center[0], self.center[1]) + self.document_delta(p - self.anchor) }
    }

    /// True transformed corners, clockwise from document top-left (not an AABB).
    pub fn quad(self, document: Rect) -> [Pos2; 4] {
        [document.left_top(), document.right_top(), document.right_bottom(), document.left_bottom()].map(|p| self.to_screen(p))
    }
    /// Conservative screen bounds for culling only.
    pub fn screen_bounds(self, document: Rect) -> Rect {
        Rect::from_points(&self.quad(document))
    }
    /// Conservative document bounds of all four viewport corners, before filter gutters.
    pub fn document_bounds(self, viewport: Rect) -> Rect {
        Rect::from_points(&[viewport.left_top(), viewport.right_top(), viewport.right_bottom(), viewport.left_bottom()].map(|p| self.to_document(p)))
    }

    /// Image/channel/mask quad with UVs attached to document corners, including reflection.
    pub fn textured_quad(self, texture: TextureId, document: Rect, uv: Rect, tint: Color32) -> Mesh {
        mesh(texture, self.quad(document), [uv.left_top(), uv.right_top(), uv.right_bottom(), uv.left_bottom()], tint)
    }
    /// Screen-aligned tiled texture clipped to the true document quad (e.g. transparency).
    pub fn screen_textured_quad(self, texture: TextureId, document: Rect, anchor: Pos2, period: Vec2, tint: Color32) -> Mesh {
        let positions = self.quad(document);
        mesh(texture, positions, positions.map(|p| ((p - anchor) / period).to_pos2()), tint)
    }
}

fn mesh(texture: TextureId, positions: [Pos2; 4], uv: [Pos2; 4], tint: Color32) -> Mesh {
    let mut mesh = Mesh::with_texture(texture);
    mesh.vertices.extend(positions.into_iter().zip(uv).map(|(pos, uv)| egui::epaint::Vertex { pos, uv, color: tint }));
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    mesh
}

#[cfg(test)]
mod tests;
