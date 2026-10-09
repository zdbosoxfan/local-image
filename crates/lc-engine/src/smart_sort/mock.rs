//! Deterministic, file-free tagger used by engine and later headless UI tests.

use super::{Tagger, store::normalized};
use lightcraft_raster::Rgba8;

#[derive(Default)]
pub struct MockTagger;
impl Tagger for MockTagger {
    fn model_id(&self) -> &str {
        "mock-tags"
    }
    fn dim(&self) -> usize {
        8
    }
    fn embed_image(&self, img: &Rgba8) -> Result<Vec<f32>, String> {
        if img.data.is_empty() {
            return Err("empty image".into());
        }
        let mut rgb = [0.0; 3];
        for p in &img.data {
            for c in 0..3 {
                rgb[c] += f32::from(p[c]) / 255.0;
            }
        }
        for c in &mut rgb {
            *c /= img.data.len() as f32;
        }
        let [r, g, b] = rgb;
        let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let saturation = r.max(g).max(b) - r.min(g).min(b);
        normalized(vec![r, g, b, l, r - g, b - g, saturation, 1.0], 8)
    }
    fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
        let mut v = vec![0.0; 8];
        for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
            match word {
                "red" => v[0] += 1.0,
                "green" => v[1] += 1.0,
                "blue" => v[2] += 1.0,
                "bright" => v[3] += 1.0,
                "dark" => v[3] -= 1.0,
                "colorful" => v[6] += 1.0,
                _ => {}
            }
        }
        if v.iter().all(|v| *v == 0.0) {
            let hash = text.bytes().fold(2166136261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
            v[(hash.wrapping_add(1) as usize) % 8] = if hash & 256 == 0 { 1.0 } else { -1.0 };
        }
        normalized(v, 8)
    }
}
