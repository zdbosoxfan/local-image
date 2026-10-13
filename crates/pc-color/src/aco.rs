//! Adobe ACO v1/v2. Layout follows Adobe's Photoshop File Formats specification.
use crate::{
    Color, ColorMode,
    ase::{MAX_BYTES, MAX_ENTRIES, MAX_NAME, Palette, ReadResult, Swatch},
};
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn word(&mut self) -> Result<u16, String> {
        let (b, tail) = self.0.split_at_checked(2).ok_or("truncated ACO")?;
        self.0 = tail;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn section(&mut self) -> Result<ReadResult, String> {
        let version = self.word()?;
        if !matches!(version, 1 | 2) {
            return Err("unsupported ACO version".into());
        }
        let count = usize::from(self.word()?);
        if count > MAX_ENTRIES {
            return Err("ACO entry budget exceeded".into());
        }
        let mut out = ReadResult { palette: Palette::default(), warnings: vec![] };
        for i in 0..count {
            let model = self.word()?;
            let v = [self.word()?, self.word()?, self.word()?, self.word()?];
            let mut name = format!("Colour {}", i + 1);
            if version == 2 {
                let n = (u32::from(self.word()?) << 16) | u32::from(self.word()?);
                if n == 0 || n as usize > MAX_NAME + 1 {
                    return Err("ACO name budget exceeded".into());
                }
                let mut units = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    units.push(self.word()?);
                }
                if units.pop() != Some(0) || units.contains(&0) {
                    return Err("invalid ACO name terminator".into());
                }
                name = String::from_utf16(&units).map_err(|_| "invalid ACO UTF-16")?;
            }
            let norm = v.map(|x| f32::from(x) / 65535.);
            let color = match model {
                0 => Color::rgb(norm[0], norm[1], norm[2]),
                1 => {
                    // HSB, as specified by Adobe (hue range 0..65535).
                    let h = norm[0] * 6.;
                    let s = norm[1];
                    let b = norm[2];
                    let x = b * (1. - s * (h % 2. - 1.).abs());
                    let m = b * (1. - s);
                    let rgb = match h as u32 % 6 {
                        0 => [b, x, m],
                        1 => [x, b, m],
                        2 => [m, b, x],
                        3 => [m, x, b],
                        4 => [x, m, b],
                        _ => [b, m, x],
                    };
                    Color::rgb(rgb[0], rgb[1], rgb[2])
                }
                2 => Color { mode: ColorMode::Cmyk, c: norm.map(|x| 1. - x), alpha: 1. },
                7 => Color {
                    mode: ColorMode::Lab,
                    c: [f32::from(v[0]) / 10000., (f32::from(v[1] as i16) / 100. + 128.) / 255., (f32::from(v[2] as i16) / 100. + 128.) / 255., 0.],
                    alpha: 1.,
                },
                8 => Color::gray(1. - f32::from(v[0]) / 10000.),
                9 => Color { mode: ColorMode::Cmyk, c: v.map(|x| 1. - f32::from(x) / 10000.), alpha: 1. },
                _ => {
                    out.warnings.push(format!("Unsupported ACO colour model {model}: {name}"));
                    continue;
                }
            };
            if color.c.iter().any(|x| !(0.0..=1.0).contains(x)) {
                return Err("ACO component outside colour range".into());
            }
            out.palette.swatches.push(Swatch { name, color, global: false, spot: false });
        }
        Ok(out)
    }
}
pub fn read(bytes: &[u8]) -> Result<ReadResult, String> {
    if bytes.len() > MAX_BYTES {
        return Err("ACO byte budget exceeded".into());
    }
    let mut r = Reader(bytes);
    let first_version = bytes.get(..2);
    let mut out = r.section()?;
    if !r.0.is_empty() && first_version == Some(&[0, 1]) {
        if r.0.get(..2) != Some(&[0, 2]) {
            return Err("expected ACO v2 section".into());
        }
        out = r.section()?;
    }
    if !r.0.is_empty() {
        return Err("trailing ACO bytes".into());
    }
    Ok(out)
}
/// Write the Photoshop convention: a v1 section followed by v2 names.
pub fn write(p: &Palette) -> Result<Vec<u8>, String> {
    let entries: Vec<_> = p.swatches.iter().chain(p.groups.iter().flat_map(|g| &g.swatches)).collect();
    if entries.len() > MAX_ENTRIES {
        return Err("ACO entry budget exceeded".into());
    }
    let mut out = Vec::new();
    for version in [1u16, 2] {
        out.extend(version.to_be_bytes());
        out.extend((entries.len() as u16).to_be_bytes());
        for w in &entries {
            let c = w.color;
            if c.alpha != 1. || c.c.iter().any(|x| !x.is_finite() || !(0.0..=1.0).contains(x)) {
                return Err("ACO requires opaque bounded colours".into());
            }
            let q = |x: f32| (x * 65535.).round() as u16;
            let (model, values) = match c.mode {
                ColorMode::Rgb => (0u16, [q(c.c[0]), q(c.c[1]), q(c.c[2]), 0]),
                ColorMode::Cmyk => (2, c.c.map(|x| q(1. - x))),
                ColorMode::Lab => (
                    7,
                    [
                        (c.c[0] * 10000.).round() as u16,
                        ((c.c[1] * 255. - 128.) * 100.).round() as i16 as u16,
                        ((c.c[2] * 255. - 128.) * 100.).round() as i16 as u16,
                        0,
                    ],
                ),
                ColorMode::Grayscale => (8, [((1. - c.c[0]) * 10000.).round() as u16, 0, 0, 0]),
                _ => return Err("unsupported ACO output model".into()),
            };
            out.extend(model.to_be_bytes());
            out.extend(values.into_iter().flat_map(u16::to_be_bytes));
            if version == 2 {
                let units: Vec<_> = w.name.encode_utf16().collect();
                if units.len() > MAX_NAME || units.contains(&0) {
                    return Err("invalid ACO name".into());
                }
                out.extend(((units.len() + 1) as u32).to_be_bytes());
                out.extend(units.into_iter().chain([0]).flat_map(u16::to_be_bytes));
            }
        }
    }
    if out.len() > MAX_BYTES {
        return Err("ACO byte budget exceeded".into());
    }
    Ok(out)
}
