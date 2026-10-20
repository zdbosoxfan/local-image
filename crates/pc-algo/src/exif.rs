//! The camera settings the photography commands use, read through the shared EXIF reader (exposure, aperture,
//! ISO, focal length, camera and lens names). Input is the raw TIFF-structured EXIF payload
//! (`II*\0` / `MM\0*`, as stored in `Document.metadata.exif`); malformed data yields `None`
//! fields, never a panic. Tag numbers are from the EXIF 2.32 specification (CIPA DC-008).

/// Camera settings read from EXIF.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraInfo {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    /// Seconds.
    pub exposure_time: Option<f64>,
    pub f_number: Option<f64>,
    pub iso: Option<f64>,
    /// Millimetres.
    pub focal_length: Option<f64>,
    /// 35 mm-equivalent focal length in millimetres.
    pub focal_length_35mm: Option<f64>,
}

impl CameraInfo {
    /// Linear exposure factor `t · ISO / N²` (relative brightness of the image for one scene).
    pub fn exposure_factor(&self) -> Option<f64> {
        let t = self.exposure_time?;
        let n = self.f_number.unwrap_or(1.0).max(0.1);
        Some(t * self.iso.unwrap_or(100.0) / 100.0 / (n * n))
    }
}

/// Read camera settings through the shared Library/editor metadata backend.
pub fn read(exif: &[u8]) -> CameraInfo {
    let m = lightcraft_meta::read_exif(exif);
    CameraInfo {
        make: m.make,
        model: m.model,
        lens: m.lens_model,
        exposure_time: m.exposure_time,
        f_number: m.f_number,
        iso: m.iso.map(f64::from),
        focal_length: m.focal_length,
        focal_length_35mm: m.focal_length_35mm,
    }
}

/// Build a little-endian EXIF payload through the shared schema-aware writer.
pub fn build(info: &CameraInfo) -> Vec<u8> {
    lightcraft_meta::write_exif(&lightcraft_meta::Metadata {
        make: info.make.clone(),
        model: info.model.clone(),
        lens_model: info.lens.clone(),
        exposure_time: info.exposure_time,
        f_number: info.f_number,
        iso: info.iso.filter(|v| v.is_finite() && *v > 0.0).map(|v| v as u32),
        focal_length: info.focal_length,
        focal_length_35mm: info.focal_length_35mm,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_garbage() {
        let info = CameraInfo {
            make: Some("Generic".into()),
            model: Some("Camera 1".into()),
            lens: Some("24-70mm".into()),
            exposure_time: Some(0.004),
            f_number: Some(8.0),
            iso: Some(200.0),
            focal_length: Some(35.0),
            focal_length_35mm: Some(52.0),
        };
        let b = build(&info);
        let r = read(&b);
        assert_eq!(r, info);
        assert!((r.exposure_factor().unwrap() - 0.004 * 2.0 / 64.0).abs() < 1e-9);
        let mut prefixed = b"Exif\0\0".to_vec();
        prefixed.extend_from_slice(&b);
        assert_eq!(read(&prefixed), info);
        assert_eq!(read(b"nonsense"), CameraInfo::default());
        for cut in [5, 12, 30, b.len() - 3] {
            let _ = read(&b[..cut]);
        }
    }
    #[test]
    fn shared_camera_reader_preserves_35mm_and_extended_iso_and_apex() {
        let info = CameraInfo { exposure_time: Some(6.0), f_number: Some(6.0), iso: Some(128000.0), focal_length_35mm: Some(52.0), ..Default::default() };
        let bytes = build(&info);
        let back = read(&bytes);
        assert_eq!(back.iso, Some(128000.0));
        assert_eq!(back.focal_length_35mm, Some(52.0));
        // Change the synthetic Exif entries into the standard APEX fallback tags.
        let mut apex = bytes;
        let root = u32::from_le_bytes(apex[4..8].try_into().unwrap()) as usize;
        let count = u16::from_le_bytes(apex[root..root + 2].try_into().unwrap()) as usize;
        let child = (0..count)
            .map(|i| root + 2 + i * 12)
            .find_map(|at| {
                (u16::from_le_bytes(apex[at..at + 2].try_into().unwrap()) == 0x8769)
                    .then(|| u32::from_le_bytes(apex[at + 8..at + 12].try_into().unwrap()) as usize)
            })
            .unwrap();
        let count = u16::from_le_bytes(apex[child..child + 2].try_into().unwrap()) as usize;
        for i in 0..count {
            let at = child + 2 + i * 12;
            let tag = u16::from_le_bytes(apex[at..at + 2].try_into().unwrap());
            let replacement = match tag {
                0x829a => 0x9201u16,
                0x829d => 0x9202u16,
                _ => continue,
            };
            apex[at..at + 2].copy_from_slice(&replacement.to_le_bytes());
        }
        let back = read(&apex);
        assert_eq!(back.exposure_time, Some(1.0 / 64.0));
        assert_eq!(back.f_number, Some(8.0));
        assert_eq!(read(&[0; 8]), CameraInfo::default());
        println!("camera quality: ISO 128000, APEX shutter 1/64, aperture f/8, 35mm equivalent 52 all retained");
    }
}
