//! Read-only Adobe Lens Correction Profile (LCP) import. Adobe profiles are user-owned
//! data and must never be bundled. Perspective-model equations follow Adobe Camera
//! Model (2010, `lensprofile_creator_cameramodel.pdf`), as implemented by
//! RawTherapee `rtengine/lcp.cc` and lensfun's
//! `apps/lensfun-convert-lcp`. No XML entities, external resources or executable content.
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum LcpError {
    #[error("Invalid or oversized LCP XML")]
    Xml,
    #[error("Unsupported LCP model (fisheye or piecewise vignette)")]
    Unsupported,
    #[error("Invalid LCP calibration")]
    Calibration,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LcpModel {
    /// Focal lengths as fractions of the calibration image's long edge.
    pub focal: [f64; 2],
    /// Optical centre as fractions of image width and height respectively.
    pub center: [f64; 2],
    pub radial: [f64; 3],
    pub tangential: [f64; 2],
    pub scale: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LcpCorrection {
    pub distortion: Option<LcpModel>,
    /// Red/green, green (normalisation reference), blue/green models.
    pub chromatic: Option<[LcpModel; 3]>,
    pub vignette: Option<LcpModel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LcpCalibration {
    pub focal_mm: f64,
    pub focus_m: f64,
    /// Adobe ApertureValue uses APEX units, not f-number.
    pub aperture_value: Option<f64>,
    pub correction: LcpCorrection,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LcpProfile {
    pub name: String,
    pub camera_make: String,
    pub camera_model: String,
    pub lens_name: String,
    pub is_raw: bool,
    pub sensor_format_factor: f64,
    pub calibrations: Vec<LcpCalibration>,
}

impl LcpProfile {
    pub fn parse(bytes: &[u8]) -> Result<Self, LcpError> {
        let tree = Xml::parse(bytes)?;
        if tree.find("FisheyeModel").is_some() || tree.find("VignetteModelPiecewiseParam").is_some() {
            return Err(LcpError::Unsupported);
        }
        let profiles = tree.find("CameraProfiles").ok_or(LcpError::Calibration)?;
        let crop = tree.field("SensorFormatFactor").map(number).transpose()?.unwrap_or(1.0);
        if !(0.1..=20.0).contains(&crop) {
            return Err(LcpError::Calibration);
        }
        let mut calibrations = Vec::new();
        for entry in profiles.descendants("li") {
            let entry = entry.children.iter().find(|c| local(&c.name) == "Description").unwrap_or(entry);
            let Some(perspective) = entry.find("PerspectiveModel") else { continue };
            let focal_mm = number(entry.field("FocalLength").ok_or(LcpError::Calibration)?)?;
            if !(0.1..=5000.0).contains(&focal_mm) {
                return Err(LcpError::Calibration);
            }
            let focus_m = entry.field("FocusDistance").map(number).transpose()?.unwrap_or(10000.0);
            if focus_m < 0.0 {
                return Err(LcpError::Calibration);
            }
            let aperture_value = entry.field("ApertureValue").map(number).transpose()?;
            let fallback = focal_mm * crop / 35.0;
            let distortion = Some(model(perspective, fallback, false)?);
            let chromatic = match (
                perspective.find("ChromaticRedGreenModel"),
                perspective.find("ChromaticGreenModel"),
                perspective.find("ChromaticBlueGreenModel"),
            ) {
                (Some(r), Some(g), Some(b)) => Some([model(r, fallback, false)?, model(g, fallback, false)?, model(b, fallback, false)?]),
                (None, None, None) => None,
                _ => return Err(LcpError::Calibration),
            };
            let vignette = perspective.find("VignetteModel").map(|n| model(n, fallback, true)).transpose()?;
            calibrations.push(LcpCalibration {
                focal_mm,
                focus_m: focus_m.min(10000.0),
                aperture_value,
                correction: LcpCorrection { distortion, chromatic, vignette },
            });
            if calibrations.len() > 4096 {
                return Err(LcpError::Calibration);
            }
        }
        if calibrations.is_empty() {
            return Err(LcpError::Calibration);
        }
        let text = |key| tree.field(key).unwrap_or_default().to_owned();
        Ok(Self {
            name: text("ProfileName"),
            camera_make: text("Make"),
            camera_model: tree.field("Model").or_else(|| tree.field("UniqueCameraModel")).unwrap_or_default().to_owned(),
            lens_name: tree.field("LensPrettyName").or_else(|| tree.field("Lens")).unwrap_or_default().to_owned(),
            is_raw: tree.field("CameraRawProfile").is_none_or(|v| v.eq_ignore_ascii_case("true")),
            sensor_format_factor: crop,
            calibrations,
        })
    }

    /// Clamp outside measured ranges; interpolate focal length and focus in log space,
    /// vignetting in APEX aperture space. Missing focus uses the longest measured distance.
    /// Missing aperture disables vignetting instead of guessing the capture aperture.
    pub fn correction(&self, focal_mm: f64, focus_m: Option<f64>, f_number: Option<f64>) -> Option<LcpCorrection> {
        if !focal_mm.is_finite() || focal_mm <= 0.0 {
            return None;
        }
        let focus = focus_m.filter(|x| x.is_finite() && *x > 0.0).map(f64::ln);
        let aperture = f_number.filter(|x| x.is_finite() && *x > 0.0).map(|f| 2.0 * f.log2());
        let distortion = self.interpolate(focal_mm, focus, false, |c| c.distortion.map(|m| [m; 3])).map(|m| m[0]);
        let chromatic = self.interpolate(focal_mm, focus, false, |c| c.chromatic);
        let vignette = aperture.and_then(|a| self.interpolate(focal_mm, Some(a), true, |c| c.vignette.map(|m| [m; 3]))).map(|m| m[0]);
        (distortion.is_some() || chromatic.is_some() || vignette.is_some()).then_some(LcpCorrection { distortion, chromatic, vignette })
    }

    fn interpolate(
        &self,
        focal: f64,
        secondary: Option<f64>,
        vignette: bool,
        get: impl Fn(LcpCorrection) -> Option<[LcpModel; 3]>,
    ) -> Option<[LcpModel; 3]> {
        let mut rows: Vec<_> =
            self.calibrations.iter().filter(|c| get(c.correction).is_some() && (!vignette || c.aperture_value.is_some())).collect();
        rows.sort_by(|a, b| a.focal_mm.total_cmp(&b.focal_mm));
        let lo = rows.iter().rev().find(|c| c.focal_mm <= focal).or(rows.first())?.focal_mm;
        let hi = rows.iter().find(|c| c.focal_mm >= focal).or(rows.last())?.focal_mm;
        let at_focal = |f: f64| {
            let axis = |c: &LcpCalibration| if vignette { c.aperture_value.unwrap_or(0.0) } else { c.focus_m.max(1e-6).ln() };
            let mut same: Vec<_> = rows.iter().copied().filter(|c| c.focal_mm == f).collect();
            same.sort_by(|a, b| axis(a).total_cmp(&axis(b)));
            // A profile may repeat a focus distance at multiple apertures (or an
            // aperture at multiple focus distances). Keep the first calibration
            // at each active-axis coordinate, matching upstream tie retention.
            // Otherwise reverse lower-bracket search and forward upper-bracket
            // search select different models at the exact same coordinate.
            same.dedup_by(|a, b| axis(a) == axis(b));
            let first = same.first()?;
            let last = same.last()?;
            let target = secondary.unwrap_or_else(|| axis(last));
            let a = same.iter().rev().find(|c| axis(c) <= target).unwrap_or(first);
            let b = same.iter().find(|c| axis(c) >= target).unwrap_or(last);
            let t = weight(axis(a), axis(b), target);
            Some(merge(get(a.correction)?, get(b.correction)?, t))
        };
        Some(merge(at_focal(lo)?, at_focal(hi)?, weight(lo.ln(), hi.ln(), focal.ln())))
    }
}

fn weight(a: f64, b: f64, x: f64) -> f64 {
    if b > a { ((x - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 }
}
fn merge(a: [LcpModel; 3], b: [LcpModel; 3], t: f64) -> [LcpModel; 3] {
    let mix = |x, y| x + (y - x) * t;
    std::array::from_fn(|i| LcpModel {
        focal: std::array::from_fn(|j| mix(a[i].focal[j], b[i].focal[j])),
        center: std::array::from_fn(|j| mix(a[i].center[j], b[i].center[j])),
        radial: std::array::from_fn(|j| mix(a[i].radial[j], b[i].radial[j])),
        tangential: std::array::from_fn(|j| mix(a[i].tangential[j], b[i].tangential[j])),
        scale: mix(a[i].scale, b[i].scale),
    })
}
fn number(s: &str) -> Result<f64, LcpError> {
    s.trim().parse::<f64>().ok().filter(|x| x.is_finite() && x.abs() < 1e9).ok_or(LcpError::Calibration)
}
fn model(n: &Xml, fallback: f64, vignette: bool) -> Result<LcpModel, LcpError> {
    let n = n.children.iter().find(|c| local(&c.name) == "Description").unwrap_or(n);
    let value = |key: &str, default| n.direct_field(key).map(number).transpose().map(|v| v.unwrap_or(default));
    let focal = [value("FocalLengthX", fallback)?, value("FocalLengthY", fallback)?];
    let center = [value("ImageXCenter", 0.5)?, value("ImageYCenter", 0.5)?];
    if focal.iter().any(|x| *x < 0.001 || *x > 1000.0) || center.iter().any(|x| !(0.0..=1.0).contains(x)) {
        return Err(LcpError::Calibration);
    }
    let prefix = if vignette { "VignetteModelParam" } else { "RadialDistortParam" };
    let radial = [value(&format!("{prefix}1"), 0.0)?, value(&format!("{prefix}2"), 0.0)?, value(&format!("{prefix}3"), 0.0)?];
    let tangential = [
        value("TangentialDistortParam1", value("RadialDistortParam4", 0.0)?)?,
        value("TangentialDistortParam2", value("RadialDistortParam5", 0.0)?)?,
    ];
    if vignette && (value("VignetteModelParam4", 0.0)? != 0.0 || value("VignetteModelParam5", 0.0)? != 0.0) {
        return Err(LcpError::Unsupported);
    }
    let scale = value("ScaleFactor", 1.0)?;
    if !(0.1..=10.0).contains(&scale) || radial.iter().chain(&tangential).any(|x| x.abs() > 100.0) {
        return Err(LcpError::Calibration);
    }
    Ok(LcpModel { focal, center, radial, tangential, scale })
}

/// Small bounded XML subset for Adobe's RDF. Qualified names are retained for
/// well-formedness checks; field lookup uses the local name. No DTD/entity expansion.
#[derive(Default)]
struct Xml {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Xml>,
}
fn local(s: &str) -> &str {
    s.rsplit(':').next().unwrap_or(s)
}
impl Xml {
    fn direct_field(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| local(k) == key)
            .map(|(_, v)| v.as_str())
            .or_else(|| self.children.iter().find(|n| local(&n.name) == key).map(|n| n.text.trim()))
    }
    fn field(&self, key: &str) -> Option<&str> {
        self.direct_field(key).or_else(|| self.children.iter().find_map(|n| n.field(key)))
    }
    fn find(&self, key: &str) -> Option<&Self> {
        if local(&self.name) == key { Some(self) } else { self.children.iter().find_map(|n| n.find(key)) }
    }
    fn descendants<'a>(&'a self, key: &str) -> Vec<&'a Self> {
        let mut out = Vec::new();
        for n in &self.children {
            if local(&n.name) == key {
                out.push(n);
            } else {
                out.extend(n.descendants(key));
            }
        }
        out
    }
    fn parse(bytes: &[u8]) -> Result<Self, LcpError> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(LcpError::Xml);
        }
        let mut input = std::str::from_utf8(bytes).map_err(|_| LcpError::Xml)?.trim_start_matches('\u{feff}');
        let mut stack = vec![Self::default()];
        let mut nodes = 0;
        while !input.is_empty() {
            if let Some(rest) = input.strip_prefix("<!--") {
                let end = rest.find("-->").ok_or(LcpError::Xml)?;
                input = &rest[end + 3..];
                continue;
            }
            if let Some(rest) = input.strip_prefix("<?") {
                let end = rest.find("?>").ok_or(LcpError::Xml)?;
                input = &rest[end + 2..];
                continue;
            }
            if input.starts_with("<!") {
                return Err(LcpError::Xml);
            }
            if let Some(rest) = input.strip_prefix("</") {
                let end = rest.find('>').ok_or(LcpError::Xml)?;
                let n = stack.pop().ok_or(LcpError::Xml)?;
                if n.name != rest[..end].trim() || stack.is_empty() {
                    return Err(LcpError::Xml);
                }
                stack.last_mut().ok_or(LcpError::Xml)?.children.push(n);
                input = &rest[end + 1..];
                continue;
            }
            if let Some(mut rest) = input.strip_prefix('<') {
                nodes += 1;
                if nodes > 100_000 || stack.len() > 64 {
                    return Err(LcpError::Xml);
                }
                let end = rest.find(|c: char| c.is_whitespace() || c == '/' || c == '>').ok_or(LcpError::Xml)?;
                let mut n = Self { name: rest[..end].to_owned(), ..Self::default() };
                if n.name.is_empty() {
                    return Err(LcpError::Xml);
                }
                rest = &rest[end..];
                loop {
                    rest = rest.trim_start();
                    if let Some(tail) = rest.strip_prefix("/>") {
                        stack.last_mut().ok_or(LcpError::Xml)?.children.push(n);
                        input = tail;
                        break;
                    }
                    if let Some(tail) = rest.strip_prefix('>') {
                        stack.push(n);
                        input = tail;
                        break;
                    }
                    let end = rest.find(|c: char| c.is_whitespace() || c == '=').ok_or(LcpError::Xml)?;
                    let key = &rest[..end];
                    rest = rest[end..].trim_start().strip_prefix('=').ok_or(LcpError::Xml)?.trim_start();
                    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"').ok_or(LcpError::Xml)?;
                    rest = &rest[1..];
                    let end = rest.find(quote).ok_or(LcpError::Xml)?;
                    if n.attrs.iter().any(|(k, _)| k == key) {
                        return Err(LcpError::Xml);
                    }
                    n.attrs.push((key.to_owned(), unescape(&rest[..end])?));
                    rest = &rest[end + 1..];
                }
            } else {
                let end = input.find('<').unwrap_or(input.len());
                let text = &input[..end];
                if stack.len() == 1 && !text.trim().is_empty() {
                    return Err(LcpError::Xml);
                }
                stack.last_mut().ok_or(LcpError::Xml)?.text.push_str(&unescape(text)?);
                input = &input[end..];
            }
        }
        if stack.len() != 1 {
            return Err(LcpError::Xml);
        }
        let root = stack.pop().ok_or(LcpError::Xml)?;
        if root.children.len() != 1 {
            return Err(LcpError::Xml);
        }
        Ok(root)
    }
}
fn unescape(mut s: &str) -> Result<String, LcpError> {
    let mut out = String::new();
    while let Some(start) = s.find('&') {
        out.push_str(&s[..start]);
        s = &s[start + 1..];
        let end = s.find(';').ok_or(LcpError::Xml)?;
        let entity = &s[..end];
        let c = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(h) = entity.strip_prefix("#x") {
                    u32::from_str_radix(h, 16).ok()
                } else {
                    entity.strip_prefix('#').and_then(|d| d.parse::<u32>().ok())
                };
                code.and_then(char::from_u32).filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r')).ok_or(LcpError::Xml)?
            }
        };
        out.push(c);
        s = &s[end + 1..];
    }
    out.push_str(s);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(entries: &str) -> Vec<u8> {
        format!("<rdf:RDF><rdf:Description stCamera:ProfileName='Adobe &amp; custom' stCamera:Make='Sony' stCamera:Model='A1' stCamera:Lens='Zoom' stCamera:SensorFormatFactor='1'><stCamera:CameraProfiles><rdf:Seq>{entries}</rdf:Seq></stCamera:CameraProfiles></rdf:Description></rdf:RDF>").into_bytes()
    }
    fn entry(focal: f64, aperture: f64, radial: f64) -> String {
        format!(
            "<rdf:li><rdf:Description stCamera:FocalLength='{focal}' stCamera:FocusDistance='10' stCamera:ApertureValue='{aperture}'><stCamera:PerspectiveModel stCamera:FocalLengthX='1' stCamera:FocalLengthY='0.8' stCamera:RadialDistortParam1='{radial}'><stCamera:VignetteModel stCamera:VignetteModelParam1='{radial}' /></stCamera:PerspectiveModel></rdf:Description></rdf:li>"
        )
    }
    #[test]
    fn lcp_import_interpolates_log_focal_apex_aperture_and_preserves_axes() -> Result<(), LcpError> {
        let bytes = profile(&format!("{}{}{}{}", entry(20.0, 2.0, 0.1), entry(20.0, 4.0, 0.3), entry(80.0, 2.0, 0.5), entry(80.0, 4.0, 0.7)));
        let p = LcpProfile::parse(&bytes)?;
        assert_eq!(p.name, "Adobe & custom");
        assert_eq!(p.camera_model, "A1");
        assert_eq!(p.correction(1.0, None, None).ok_or(LcpError::Calibration)?.distortion.ok_or(LcpError::Calibration)?.radial[0], 0.1);
        assert_eq!(p.correction(1000.0, None, None).ok_or(LcpError::Calibration)?.distortion.ok_or(LcpError::Calibration)?.radial[0], 0.5);
        let c = p.correction(40.0, None, Some(8.0_f64.sqrt())).ok_or(LcpError::Calibration)?;
        assert_eq!(c.distortion.ok_or(LcpError::Calibration)?.focal, [1.0, 0.8]);
        assert!((c.vignette.ok_or(LcpError::Calibration)?.radial[0] - 0.4).abs() < 1e-12);
        assert!(p.correction(40.0, None, None).ok_or(LcpError::Calibration)?.vignette.is_none());
        assert!(p.correction(f64::NAN, None, None).is_none());
        Ok(())
    }
    #[test]
    fn lcp_import_rejects_entities_malformed_nonfinite_and_unsupported_models() {
        for b in [b"<!DOCTYPE x><x/>".as_slice(), b"<x>&evil;</x>", b"<x><y></x>", b"<x/><y/>"] {
            assert_eq!(LcpProfile::parse(b), Err(LcpError::Xml));
        }
        assert_eq!(LcpProfile::parse(&profile(&entry(20.0, 2.0, f64::NAN))), Err(LcpError::Calibration));
        assert_eq!(LcpProfile::parse(&profile("<FisheyeModel/>")), Err(LcpError::Unsupported));
        assert_eq!(LcpProfile::parse(&profile(&entry(20.0, 2.0, 0.0).replace("FocalLengthY='0.8'", "FocalLengthY='0'"))), Err(LcpError::Calibration));
    }
    #[test]
    fn lcp_import_reads_element_and_description_forms_and_unknown_focus() -> Result<(), LcpError> {
        let e = "<rdf:li><stCamera:FocalLength>50</stCamera:FocalLength><stCamera:FocusDistance>2</stCamera:FocusDistance><stCamera:PerspectiveModel><rdf:Description><stCamera:RadialDistortParam1>0.1</stCamera:RadialDistortParam1><stCamera:ChromaticRedGreenModel><rdf:Description stCamera:ScaleFactor='1.002'/></stCamera:ChromaticRedGreenModel><stCamera:ChromaticGreenModel/><stCamera:ChromaticBlueGreenModel stCamera:ScaleFactor='0.999'/></rdf:Description></stCamera:PerspectiveModel></rdf:li>";
        let far = e
            .replace(">2</stCamera:FocusDistance>", ">20</stCamera:FocusDistance>")
            .replace(">0.1</stCamera:RadialDistortParam1>", ">0.3</stCamera:RadialDistortParam1>");
        let p = LcpProfile::parse(&profile(&format!("{e}{far}")))?;
        let c = p.correction(50.0, None, None).ok_or(LcpError::Calibration)?;
        assert_eq!(c.distortion.ok_or(LcpError::Calibration)?.radial[0], 0.3);
        assert!((c.distortion.ok_or(LcpError::Calibration)?.focal[0] - 50.0 / 35.0).abs() < 1e-12);
        assert_eq!(c.chromatic.ok_or(LcpError::Calibration)?[0].scale, 1.002);
        let c = p.correction(50.0, Some(40.0_f64.sqrt()), None).ok_or(LcpError::Calibration)?;
        assert!((c.distortion.ok_or(LcpError::Calibration)?.radial[0] - 0.2).abs() < 1e-12);
        Ok(())
    }
}
