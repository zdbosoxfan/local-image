//! Adobe Swatch Exchange, adapted from VectorCraft `color/src/palette_io.rs` at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Big-endian ASE 1.x: UTF-16 names, RGB/CMYK/Lab/Gray float components, colour kinds
//! 0=global, 1=spot, 2=process. Unknown blocks/models are retained as explicit warnings.
//! Unlike the reference reader, nonfinite numbers and truncated known blocks are errors.
use crate::{Color, ColorMode};

pub const MAX_BYTES: usize = 16 << 20;
pub const MAX_ENTRIES: usize = 4096;
pub const MAX_NAME: usize = 1024;
#[derive(Clone, Debug, PartialEq)]
pub struct Swatch {
    pub name: String,
    pub color: Color,
    pub global: bool,
    pub spot: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub name: String,
    pub swatches: Vec<Swatch>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette {
    pub swatches: Vec<Swatch>,
    pub groups: Vec<Group>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ReadResult {
    pub palette: Palette,
    pub warnings: Vec<String>,
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let (head, tail) = self.0.split_at_checked(n).ok_or("truncated ASE block")?;
        self.0 = tail;
        Ok(head)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        self.bytes(N)?.try_into().map_err(|_| "truncated ASE field".into())
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    fn float(&mut self) -> Result<f32, String> {
        let n = f32::from_be_bytes(self.array()?);
        if n.is_finite() { Ok(n) } else { Err("nonfinite ASE component".into()) }
    }
    fn name(&mut self) -> Result<String, String> {
        let n = usize::from(self.u16()?);
        if n == 0 || n > MAX_NAME + 1 {
            return Err("invalid ASE name length".into());
        }
        let mut units = Vec::with_capacity(n);
        for _ in 0..n {
            units.push(self.u16()?);
        }
        if units.pop() != Some(0) || units.contains(&0) {
            return Err("invalid ASE name terminator".into());
        }
        String::from_utf16(&units).map_err(|_| "invalid ASE UTF-16".into())
    }
}
pub fn read(bytes: &[u8]) -> Result<ReadResult, String> {
    if bytes.len() > MAX_BYTES {
        return Err("ASE byte budget exceeded".into());
    }
    let mut r = Reader(bytes);
    if r.bytes(4)? != b"ASEF" || r.u16()? != 1 {
        return Err("unsupported ASE signature/version".into());
    }
    let _minor = r.u16()?;
    let blocks = r.u32()? as usize;
    if blocks > MAX_ENTRIES * 3 {
        return Err("ASE block budget exceeded".into());
    }
    let mut out = ReadResult { palette: Palette::default(), warnings: Vec::new() };
    let mut group = None;
    let mut entries = 0;
    for _ in 0..blocks {
        let kind = r.u16()?;
        let len = r.u32()? as usize;
        let mut body = Reader(r.bytes(len)?);
        if matches!(kind, 0xc001 | 1) && entries == MAX_ENTRIES {
            return Err("ASE entry budget exceeded".into());
        }
        match kind {
            0xc001 => {
                entries += 1;
                out.palette.groups.push(Group { name: body.name()?, swatches: Vec::new() });
                group = Some(out.palette.groups.len() - 1);
            }
            0xc002 => {
                group = None;
            }
            1 => {
                entries += 1;
                let name = body.name()?;
                let model = body.array::<4>()?;
                let mut color = Color::BLACK;
                match &model {
                    b"RGB " => {
                        color = Color::rgb(body.float()?, body.float()?, body.float()?);
                    }
                    b"CMYK" => {
                        color.mode = ColorMode::Cmyk;
                        for c in &mut color.c {
                            *c = body.float()?;
                        }
                    }
                    b"Gray" => {
                        color = Color::gray(body.float()?);
                    }
                    b"LAB " => {
                        let l = body.float()?;
                        color.mode = ColorMode::Lab;
                        color.c = [if l > 1.0 { l / 100.0 } else { l }, (body.float()? + 128.0) / 255.0, (body.float()? + 128.0) / 255.0, 0.0];
                    }
                    _ => {
                        out.warnings.push(format!("unsupported ASE model {}", String::from_utf8_lossy(&model)));
                        continue;
                    }
                }
                // Some external ASE writers omit the type; process is the established fallback.
                let tag = if body.0.is_empty() { 2 } else { body.u16()? };
                if tag > 2 {
                    return Err("invalid ASE colour type".into());
                }
                for c in &mut color.c {
                    *c = c.clamp(0., 1.);
                }
                let w = Swatch { name, color, global: tag < 2, spot: tag == 1 };
                if let Some(i) = group {
                    out.palette.groups[i].swatches.push(w);
                } else {
                    out.palette.swatches.push(w);
                }
            }
            _ => {
                out.warnings.push(format!("unsupported ASE block {kind:#06x}"));
                continue;
            }
        }
        if !body.0.is_empty() {
            return Err("trailing data in known ASE block".into());
        }
        if entries > MAX_ENTRIES {
            return Err("ASE entry budget exceeded".into());
        }
    }
    if !r.0.is_empty() {
        return Err("trailing ASE bytes".into());
    }
    Ok(out)
}
fn name(out: &mut Vec<u8>, value: &str) -> Result<(), String> {
    let units: Vec<_> = value.encode_utf16().collect();
    if units.len() > MAX_NAME || units.contains(&0) {
        return Err("ASE name budget or embedded zero".into());
    }
    out.extend(((units.len() + 1) as u16).to_be_bytes());
    out.extend(units.into_iter().chain([0]).flat_map(u16::to_be_bytes));
    Ok(())
}
fn block(out: &mut Vec<u8>, kind: u16, body: &[u8]) {
    out.extend(kind.to_be_bytes());
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
}
fn write_swatch(out: &mut Vec<u8>, w: &Swatch) -> Result<(), String> {
    if w.color.alpha != 1. || w.color.c.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
        return Err("ASE requires opaque bounded colours".into());
    }
    let mut body = Vec::new();
    name(&mut body, &w.name)?;
    let (model, values): (&[u8; 4], Vec<f32>) = match w.color.mode {
        ColorMode::Rgb => (b"RGB ", w.color.c[..3].to_vec()),
        ColorMode::Cmyk => (b"CMYK", w.color.c.to_vec()),
        ColorMode::Grayscale => (b"Gray", vec![w.color.c[0]]),
        ColorMode::Lab => (b"LAB ", vec![w.color.c[0], w.color.c[1] * 255. - 128., w.color.c[2] * 255. - 128.]),
        _ => return Err("unsupported ASE output colour model".into()),
    };
    body.extend(model);
    body.extend(values.iter().flat_map(|v| v.to_be_bytes()));
    let kind: u16 = if w.spot {
        1
    } else if w.global {
        0
    } else {
        2
    };
    body.extend(kind.to_be_bytes());
    block(out, 1, &body);
    Ok(())
}
pub fn write(p: &Palette) -> Result<Vec<u8>, String> {
    let entries = p.swatches.len() + p.groups.iter().map(|g| 1 + g.swatches.len()).sum::<usize>();
    if entries > MAX_ENTRIES {
        return Err("ASE entry budget exceeded".into());
    }
    let blocks = p.swatches.len() + p.groups.iter().map(|g| 2 + g.swatches.len()).sum::<usize>();
    let mut out = b"ASEF\0\x01\0\0".to_vec();
    out.extend((blocks as u32).to_be_bytes());
    for w in &p.swatches {
        write_swatch(&mut out, w)?;
    }
    for g in &p.groups {
        let mut body = Vec::new();
        name(&mut body, &g.name)?;
        block(&mut out, 0xc001, &body);
        for w in &g.swatches {
            write_swatch(&mut out, w)?;
        }
        block(&mut out, 0xc002, &[]);
    }
    if out.len() > MAX_BYTES {
        return Err("ASE byte budget exceeded".into());
    }
    Ok(out)
}
#[cfg(test)]
mod tests;
