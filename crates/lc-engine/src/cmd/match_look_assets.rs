//! Portable reference previews and filesystem identities. Saved recipes never retain paths.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use lightcraft_raster::Rgba8;
use serde::{Deserialize, Serialize};

pub const PREVIEW_EDGE: usize = 256;
const MAX_PREVIEW_BYTES: usize = 48 << 10;
const MAX_EXTERNAL_BYTES: u64 = 2 << 30;
pub const EXTERNAL_ID_START: u64 = 1 << 63;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReferencePreview {
    pub name: String,
    /// Small finished sRGB JPEG, base64 encoded. Never an original RAW or a filesystem path.
    pub jpeg: String,
}
impl ReferencePreview {
    pub(super) fn capture(name: &str, image: &Rgba8) -> std::result::Result<Self, String> {
        use lightcraft_codecs::encode::{ChromaSubsampling, EncodeImage, encode_jpeg};
        let bytes = encode_jpeg(&EncodeImage::rgba8(image), 72, ChromaSubsampling::S420, &Default::default()).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_PREVIEW_BYTES {
            return Err("Reference preview exceeds its size limit".into());
        }
        Ok(Self { name: name.chars().filter(|c| !c.is_control()).take(256).collect(), jpeg: STANDARD.encode(bytes) })
    }
    pub fn image(&self) -> Option<Rgba8> {
        if !self.bounded() {
            return None;
        }
        let bytes = STANDARD.decode(&self.jpeg).ok()?;
        if bytes.len() > MAX_PREVIEW_BYTES || lightcraft_codecs::sniff(&bytes) != Some(lightcraft_codecs::Format::Jpeg) {
            return None;
        }
        let decoded =
            lightcraft_codecs::decode(&bytes, lightcraft_codecs::DecodeOptions { max_size: None, max_pixels: (PREVIEW_EDGE * PREVIEW_EDGE) as u64 })
                .ok()?;
        if decoded.image.width > PREVIEW_EDGE || decoded.image.height > PREVIEW_EDGE {
            return None;
        }
        Some(decoded.to_srgb8())
    }
    pub(super) fn bounded(&self) -> bool {
        self.name.len() <= 1024
            && !self.name.chars().any(char::is_control)
            && !self.jpeg.is_empty()
            && self.jpeg.len() <= MAX_PREVIEW_BYTES.div_ceil(3) * 4
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalReference {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub key: String,
    #[serde(default = "importance")]
    pub importance: f64,
}
fn importance() -> f64 {
    1.0
}
impl ExternalReference {
    pub(super) fn unchanged(&self) -> bool {
        external_key(&self.path).is_ok_and(|k| k == self.key)
    }
}
pub(super) fn external_key(path: &str) -> std::result::Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if !metadata.is_file() {
        return Err("Choose a reference image file".into());
    }
    if metadata.len() > MAX_EXTERNAL_BYTES {
        return Err("Reference file exceeds 2 GiB".into());
    }
    let time = metadata.modified().map_err(|e| e.to_string())?.duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?;
    Ok(format!("{}:{}:{}", metadata.len(), time.as_secs(), time.subsec_nanos()))
}
