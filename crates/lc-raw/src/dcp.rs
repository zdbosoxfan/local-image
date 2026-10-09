//! DCP container reader (DNG 1.7 profile tags). No SDK code or Adobe data is bundled.
//! Bundled data has an explicit CC0/public-domain allowlist in data/dcp/manifest.json.
use crate::{ColorData, Result, RawError};
use lightcraft_tiff::Tiff;

#[derive(Clone, Debug)]
pub struct Dcp {
    pub model: String,
    pub name: String,
    pub copyright: String,
    pub color: ColorData,
    pub default_black_render: Option<u16>,
}
impl Dcp {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len()>32*1024*1024 { return Err(RawError::Corrupt("DCP exceeds 32 MiB".into())); }
        let mut data=bytes.to_vec();
        match data.get(..4) {
            Some(b"IIRC") => data[2..4].copy_from_slice(&42u16.to_le_bytes()),
            Some(b"MMCR") => data[2..4].copy_from_slice(&42u16.to_be_bytes()),
            _ => return Err(RawError::Corrupt("not a DCP profile".into())),
        }
        let t=Tiff::parse(&data)?;
        let i=t.ifds.first().ok_or_else(|| RawError::Corrupt("empty DCP".into()))?;
        let color=crate::dng::color_data(i,i);
        if !crate::color::has_matrix(&color) { return Err(RawError::Corrupt("DCP without a color matrix".into())); }
        Ok(Self { model:i.string(50708).unwrap_or_default(), name:i.string(50936).unwrap_or_default(), copyright:i.string(50942).unwrap_or_default(), color, default_black_render:i.u16(51110) })
    }
    pub fn bundled(make: &str, model: &str) -> Option<Self> {
        use crate::camera_matrices::normalized;
        let key=normalized(&format!("{make}{model}")); let model=normalized(model);
        crate::dcp_bundled::BUNDLED.iter().find(|(name,_)| { let n=normalized(name); n==key || n==model }).and_then(|(_,b)| Self::parse(b).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_bundled_profiles_have_explicit_rights_and_valid_tables() {
        assert_eq!(crate::dcp_bundled::BUNDLED.len(),55);
        for (model,b) in crate::dcp_bundled::BUNDLED {
            let p=Dcp::parse(b).unwrap(); assert_eq!(&p.model,model);
            assert!(["public domain","rawtherapee cc0","cc0"].contains(&p.copyright.to_lowercase().as_str()));
            assert!(p.color.profile.hue_sat_map.iter().any(Option::is_some));
        }
    }
    #[test]
    fn malformed_profiles_fail_without_panics() {
        for b in [&b""[..], &b"IIRC"[..], &b"II*\0\0\0\0\0"[..], &b"MMCR\xff\xff\xff\xff"[..]] { assert!(Dcp::parse(b).is_err()); }
    }
}
