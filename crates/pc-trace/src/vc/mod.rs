//! Minimal image storage for the visioncortex port; no bit-vec dependency.
#![allow(dead_code)]
mod bound;
mod color;
pub mod color_clusters;
pub use bound::*;
pub use color::*;
#[derive(Clone, Copy, Default, Debug)]
pub struct PointI32 {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Default, Debug)]
pub struct ColorImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}
impl ColorImage {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_w_h(width: usize, height: usize) -> Self {
        Self { width, height, pixels: vec![0; width * height * 4] }
    }
    pub fn get_pixel(&self, x: usize, y: usize) -> Color {
        let i = (y * self.width + x) * 4;
        Color::new_rgba(self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3])
    }
    pub fn set_pixel(&mut self, x: usize, y: usize, c: &Color) {
        let i = (y * self.width + x) * 4;
        self.pixels[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, c.a]);
    }
    pub fn to_binary_image(&self, f: impl Fn(Color) -> bool) -> BinaryImage {
        let mut b = BinaryImage::new_w_h(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                b.set_pixel(x, y, f(self.get_pixel(x, y)));
            }
        }
        b
    }
}
#[derive(Clone, Default, Debug)]
pub struct BinaryImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<bool>,
}
impl BinaryImage {
    pub fn new_w_h(width: usize, height: usize) -> Self {
        Self { width, height, pixels: vec![false; width * height] }
    }
    pub fn get_pixel(&self, x: usize, y: usize) -> bool {
        self.pixels[y * self.width + x]
    }
    pub fn at(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height && self.get_pixel(x as usize, y as usize)
    }
    pub fn set_pixel(&mut self, x: usize, y: usize, v: bool) {
        self.pixels[y * self.width + x] = v;
    }
    pub fn to_clusters(&self, diagonal: bool) -> BinaryClusters {
        let mut seen = vec![false; self.pixels.len()];
        let mut result = Vec::new();
        for seed in 0..seen.len() {
            if seen[seed] || !self.pixels[seed] {
                continue;
            }
            seen[seed] = true;
            let mut todo = vec![seed];
            let mut indices = Vec::new();
            let mut rect = BoundingRect::default();
            while let Some(i) = todo.pop() {
                let x = (i % self.width) as i32;
                let y = (i / self.width) as i32;
                indices.push((x, y));
                rect.add_x_y(x, y);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if (dx == 0 && dy == 0) || (!diagonal && dx.abs() + dy.abs() != 1) {
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        if self.at(nx, ny) {
                            let j = ny as usize * self.width + nx as usize;
                            if !seen[j] {
                                seen[j] = true;
                                todo.push(j);
                            }
                        }
                    }
                }
            }
            result.push(BinaryCluster { rect, indices });
        }
        BinaryClusters(result)
    }
}
pub struct BinaryCluster {
    pub rect: BoundingRect,
    indices: Vec<(i32, i32)>,
}
impl BinaryCluster {
    pub fn size(&self) -> usize {
        self.indices.len()
    }
    pub fn to_binary_image(&self) -> BinaryImage {
        let mut b = BinaryImage::new_w_h(self.rect.width() as usize, self.rect.height() as usize);
        for &(x, y) in &self.indices {
            b.set_pixel((x - self.rect.left) as usize, (y - self.rect.top) as usize, true);
        }
        b
    }
}
pub struct BinaryClusters(Vec<BinaryCluster>);
impl BinaryClusters {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn get_cluster(&self, i: usize) -> &BinaryCluster {
        &self.0[i]
    }
    pub fn iter(&self) -> std::slice::Iter<'_, BinaryCluster> {
        self.0.iter()
    }
}
pub struct Shape;
impl Shape {
    pub fn image_boundary_list(b: &BinaryImage) -> Vec<PointI32> {
        let mut result = Vec::new();
        for y in 0..b.height as i32 {
            for x in 0..b.width as i32 {
                if b.at(x, y) && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| !b.at(x + dx, y + dy)) {
                    result.push(PointI32 { x, y });
                }
            }
        }
        result
    }
}
impl From<BinaryImage> for Shape {
    fn from(_: BinaryImage) -> Self {
        Self
    }
}
pub struct SummedAreaTable {
    width: usize,
    sums: Vec<u64>,
}
impl SummedAreaTable {
    pub fn from_color_image(b: &ColorImage) -> Self {
        let w = b.width + 1;
        let mut s = vec![0; (b.height + 1) * w];
        for y in 0..b.height {
            let mut row = 0;
            for x in 0..b.width {
                let c = b.get_pixel(x, y);
                row += (u64::from(c.r) + u64::from(c.g) + u64::from(c.b)) / 3;
                s[(y + 1) * w + x + 1] = s[y * w + x + 1] + row;
            }
        }
        Self { width: w, sums: s }
    }
    pub fn get_region_sum_x_y_w_h(&self, x: usize, y: usize, w: usize, h: usize) -> u64 {
        let i = |x, y| self.sums[y * self.width + x];
        i(x + w, y + h) + i(x, y) - i(x + w, y) - i(x, y + h)
    }
}
