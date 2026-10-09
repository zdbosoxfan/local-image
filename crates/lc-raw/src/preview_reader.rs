//! Bounded, seek-based extraction for classic TIFF/DNG/Sony previews. Unsupported layouts
//! return None so callers can use the complete buffer extractor without changing the image.
use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use lightcraft_tiff::{ByteOrder, tags as t};
use crate::Orientation;

/// An embedded JPEG and the enclosing raw's display orientation. The JPEG's own EXIF
/// orientation takes precedence when it is greater than one, as in the buffer path.
pub struct EmbeddedPreview {
    pub jpeg: Vec<u8>,
    pub orientation: Orientation,
}
struct Reader<'a, R> {
    stream: &'a mut R,
    len: u64,
    metadata_left: usize,
    visited: HashSet<u64>,
    best: Option<Vec<u8>>,
    orientation: Option<Orientation>,
    xmp_orientation: Option<Orientation>,
}
impl<R: Read + Seek> Reader<'_, R> {
    fn read(&mut self, at: u64, n: usize) -> Option<Vec<u8>> {
        if at.checked_add(n as u64)? > self.len || n > self.metadata_left { return None; }
        self.metadata_left -= n;
        self.stream.seek(SeekFrom::Start(at)).ok()?;
        let mut bytes=vec![0;n]; self.stream.read_exact(&mut bytes).ok()?; Some(bytes)
    }
    fn jpeg(&mut self, at: u64, n: u64) -> Option<()> {
        if n < 4 {return Some(());}
        if at.checked_add(n)? > self.len {return None;}
        let header=self.read(at,4)?;
        if !header.starts_with(&[0xff,0xd8]) {return Some(());}
        // Never allocate according to an unchecked raw-file tag. Large or unusual previews
        // are handled by the existing buffer extractor instead.
        if n > 64 << 20 {return None;}
        self.stream.seek(SeekFrom::Start(at)).ok()?;
        let mut jpeg=vec![0;n as usize]; self.stream.read_exact(&mut jpeg).ok()?;
        if super::preview::is_dct_jpeg(&jpeg) {
            // The buffer extractor's tie order also includes maker-note previews after all
            // normal IFDs. Fall back for distinct equal-size candidates rather than guess.
            if self.best.as_ref().is_some_and(|b| b.len()==jpeg.len() && b!=&jpeg) {return None;}
            if self.best.as_ref().is_none_or(|b| b.len()<jpeg.len()) {self.best=Some(jpeg);}
        }
        Some(())
    }
    fn numbers(&mut self, e: &[u8], order: ByteOrder, base: u64) -> Option<Vec<u64>> {
        let ty=order.read_u16(e,2)?; let count=order.read_u32(e,4)? as usize;
        let size=match ty {3=>2,4|13=>4,16|18=>8,_=>return None};
        if count>64 {return None;}
        let total=count.checked_mul(size)?;
        let bytes=if total<=4 {e[8..8+total].to_vec()} else {self.read(base.checked_add(order.read_u32(e,8)? as u64)?,total)?};
        (0..count).map(|i|match size {2=>order.read_u16(&bytes,i*size).map(u64::from),4=>order.read_u32(&bytes,i*size).map(u64::from),_=>order.read_u64(&bytes,i*size)}).collect()
    }
    fn ifd(&mut self, at: u64, base: u64, order: ByteOrder, root: bool, maker: bool) -> Option<()> {
        if at==0 {return Some(());}
        if self.visited.len()>=64 || !self.visited.insert(at) {return None;}
        let head=self.read(at,2)?; let n=order.read_u16(&head,0)? as usize;
        if n==0 || n>8192 {return None;}
        let table=self.read(at.checked_add(2)?,n.checked_mul(12)?.checked_add(4)?)?;
        let mut off=None; let mut len=None; let mut strip=None; let mut strip_len=None; let mut compression=0; let mut tiled=false;
        let mut children=Vec::new(); let mut notes=Vec::new(); let mut tags=HashSet::new();
        for e in table[..n*12].chunks_exact(12) {
            let tag=order.read_u16(e,0)?; if !tags.insert(tag) {return None;} let ty=order.read_u16(e,2)?; let count=order.read_u32(e,4)? as u64;
            if matches!(tag,t::JPEG_INTERCHANGE_FORMAT|t::JPEG_INTERCHANGE_FORMAT_LENGTH|t::COMPRESSION|t::ORIENTATION|t::SUB_IFDS|t::EXIF_IFD|t::GPS_IFD|t::INTEROP_IFD)  {
                let values=self.numbers(e,order,base)?; let value=values.first().copied();
                match tag {
                    t::JPEG_INTERCHANGE_FORMAT=>off=value,
                    t::JPEG_INTERCHANGE_FORMAT_LENGTH=>len=value,
                    t::COMPRESSION=>compression=value.unwrap_or(0),
                    t::ORIENTATION if root=>self.orientation=value.filter(|v|(1..=8).contains(v)).map(|v|Orientation::from_exif(v as u16)),
                    t::SUB_IFDS|t::EXIF_IFD|t::GPS_IFD|t::INTEROP_IFD=>{if !maker {children.extend(values);}},
                    
                    _=>{}
                }
            }
            if matches!(tag,t::TILE_OFFSETS|t::TILE_BYTE_COUNTS) && count==1 {tiled=true;}
            if maker && tag==0x0011 {
                let value=if matches!(ty,1|7) {
                    if count<=4 {e[8] as u64} else {self.read(base.checked_add(order.read_u32(e,8)? as u64)?,1)?[0] as u64}
                } else {self.numbers(e,order,base)?.first().copied()?};
                if value!=0 {children.push(value);}
            }
            if matches!(tag,t::STRIP_OFFSETS|t::STRIP_BYTE_COUNTS) && count==1 {
                let value=self.numbers(e,order,base)?.first().copied();
                if tag==t::STRIP_OFFSETS {strip=value;} else {strip_len=value;}
            }
            if ty==7 && count>1024 {
                let at=base.checked_add(order.read_u32(e,8)? as u64)?;
                self.jpeg(at,count)?;
            }
            if tag==t::MAKER_NOTE {
                if ty!=7 || count<2 {return None;}
                notes.push((base.checked_add(order.read_u32(e,8)? as u64)?,count));
            }
            if root && tag==t::XMP {
                if !matches!(ty,1|7) || count>1<<20 {return None;}
                let bytes=if count<=4 {e[8..8+count as usize].to_vec()} else {self.read(base.checked_add(order.read_u32(e,8)? as u64)?,count as usize)?};
                self.xmp_orientation=std::str::from_utf8(&bytes).ok().and_then(|s|lightcraft_meta::parse_xmp(s).ok()).and_then(|x|x.metadata.orientation);
            }
        }
        if let (Some(off),Some(len))=(off,len) {self.jpeg(base.checked_add(off)?,len)?;}
        if matches!(compression,6|7|34892) && tiled {return None;}
        if matches!(compression,6|7|34892) && let (Some(off),Some(len))=(strip,strip_len) {self.jpeg(base.checked_add(off)?,len)?;}
        // Sony notes use TIFF-relative offsets; plain TIFF maker notes do too. Other vendor
        // dialects deliberately fall back rather than guessing their offset bases.
        for (note,count) in notes {
            let header=self.read(note,count.min(16) as usize)?;
            let start=if header.starts_with(b"SONY") || header.starts_with(b"VHAB     \0") {note.checked_add(12)?}
                else if order.read_u16(&header,0).is_some_and(|n|n>0 && n<=8192) {note}
                else {return None;};
            self.ifd(start,0,order,false,true)?;
        }
        for off in children {self.ifd(base.checked_add(off)?,base,order,false,maker)?;}
        let next=order.read_u32(&table,n*12)? as u64;
        if next!=0 && !maker {self.ifd(base.checked_add(next)?,base,order,false,maker)?;}
        Some(())
    }
}
/// Extract without reading sensor payloads. Uses at most 1 MiB of metadata and two JPEG buffers
/// (up to 64 MiB each), independent of raw-file size. Returns None for unsupported layouts,
/// malformed/cyclic directories, or no DCT preview; fall back to [`crate::embedded_preview`].
pub fn embedded_preview_reader<R: Read + Seek>(stream: &mut R) -> Option<EmbeddedPreview> {
    let len=stream.seek(SeekFrom::End(0)).ok()?;
    let mut r=Reader {stream,len,metadata_left:1<<20,visited:HashSet::new(),best:None,orientation:None,xmp_orientation:None};
    let header=r.read(0,8)?;
    let order=match &header[..2] {b"II"=>ByteOrder::Little,b"MM"=>ByteOrder::Big,_=>return None};
    if order.read_u16(&header,2)?!=42 {return None;}
    r.ifd(order.read_u32(&header,4)? as u64,0,order,true,false)?;
    let mut jpeg=r.best?;
    jpeg.truncate(super::preview::trim_eoi(&jpeg).len());
    Some(EmbeddedPreview {jpeg,orientation:r.orientation.or(r.xmp_orientation).unwrap_or_default()})
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lightcraft_tiff::{IfdBuilder, ImageData, TiffWriter, Value};
    use std::io::Cursor;
    fn jpeg(n: usize) -> Vec<u8> {
        let mut j=vec![0xff,0xd8,0xff,0xc0,0,11,8,0,1,0,1,1,1,0x11,0];
        j.extend(std::iter::repeat_n(0x55,n)); j.extend([0xff,0xd9]); j
    }
    #[test]
    fn stream_and_buffer_select_the_same_largest_preview_and_orientation() {
        for order in [ByteOrder::Little,ByteOrder::Big] {
            let mut root=IfdBuilder::new(); root.set(t::ORIENTATION,Value::Short(vec![6]));
            for n in [30,100,60] {
                let mut sub=IfdBuilder::new(); sub.set(t::COMPRESSION,Value::Short(vec![6]));
                sub.set(t::IMAGE_WIDTH,Value::Long(vec![1])); sub.set(t::IMAGE_LENGTH,Value::Long(vec![1]));
                sub.set_image(ImageData::Strips {rows_per_strip:1,strips:vec![jpeg(n)]}); root.add_sub_ifd(sub);
            }
            let bytes=TiffWriter {order,bigtiff:false}.write(&[root]).unwrap();
            let actual=embedded_preview_reader(&mut Cursor::new(&bytes)).unwrap();
            assert_eq!(actual.jpeg,crate::embedded_preview(&bytes).unwrap()); assert_eq!(actual.orientation,Orientation::Rotate90);
            for end in 0..bytes.len() {let _=embedded_preview_reader(&mut Cursor::new(&bytes[..end]));}
        }
    }
    #[test]
    fn distinct_equal_size_previews_use_the_buffer_tie_order() {
        let mut root=IfdBuilder::new();
        for value in [0x33,0x44] {
            let mut j=jpeg(100);j[20]=value;
            let mut sub=IfdBuilder::new();sub.set(t::COMPRESSION,Value::Short(vec![6]));
            sub.set(t::IMAGE_WIDTH,Value::Long(vec![1]));sub.set(t::IMAGE_LENGTH,Value::Long(vec![1]));
            sub.set_image(ImageData::Strips {rows_per_strip:1,strips:vec![j]});root.add_sub_ifd(sub);
        }
        let bytes=TiffWriter::default().write(&[root]).unwrap();
        assert!(crate::embedded_preview(&bytes).is_some());
        assert!(embedded_preview_reader(&mut Cursor::new(bytes)).is_none());
    }
    #[test]
    fn cyclic_and_oversized_directories_fall_back_without_allocating_payloads() {
        let mut bytes=b"II\x2a\0\x08\0\0\0".to_vec();
        bytes.extend(1u16.to_le_bytes()); bytes.extend(t::ORIENTATION.to_le_bytes());
        bytes.extend(3u16.to_le_bytes()); bytes.extend(1u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes()); bytes.extend(8u32.to_le_bytes());
        assert!(embedded_preview_reader(&mut Cursor::new(&bytes)).is_none());
        bytes[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(embedded_preview_reader(&mut Cursor::new(&bytes)).is_none());
        assert!(embedded_preview_reader(&mut Cursor::new(b"not TIFF")).is_none());
    }
    struct CountReads {file:Cursor<Vec<u8>>,read:usize}
    impl Read for CountReads {
        fn read(&mut self,b:&mut [u8])->std::io::Result<usize> {let n=self.file.read(b)?;self.read+=n;Ok(n)}
    }
    impl Seek for CountReads {fn seek(&mut self,p:SeekFrom)->std::io::Result<u64> {self.file.seek(p)}}
    #[test]
    fn sensor_payload_is_not_read_even_when_the_jpeg_is_at_the_end() {
        let j=jpeg(100); let at=16<<20;
        let mut bytes=b"II\x2a\0\x08\0\0\0".to_vec(); bytes.extend(2u16.to_le_bytes());
        for (tag,val) in [(t::JPEG_INTERCHANGE_FORMAT,at as u32),(t::JPEG_INTERCHANGE_FORMAT_LENGTH,j.len() as u32)] {
            bytes.extend(tag.to_le_bytes());bytes.extend(4u16.to_le_bytes());bytes.extend(1u32.to_le_bytes());bytes.extend(val.to_le_bytes());
        }
        bytes.extend(0u32.to_le_bytes());bytes.resize(at,0);bytes.extend(&j);
        let mut reader=CountReads {file:Cursor::new(bytes),read:0};
        assert_eq!(embedded_preview_reader(&mut reader).unwrap().jpeg,j);
        assert!(reader.read<1024,"sensor payload was read: {}",reader.read);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod sony_tests {
    use super::*;
    use lightcraft_tiff::{IfdBuilder, ImageData, TiffWriter, Value};
    use std::io::Cursor;
    #[test]
    fn sony_maker_note_and_xmp_orientation_match_the_buffer_path() {
        let j=vec![0xff,0xd8,0xff,0xc0,0,11,8,0,1,0,1,1,1,0x11,0,0xff,0xd9];
        let mut note=b"SONY DSC\0\0\0\0".to_vec();
        assert_eq!(note.len(),12);
        note.extend(1u16.to_le_bytes());note.extend(0x0011u16.to_le_bytes());note.extend(7u16.to_le_bytes());
        note.extend(4u32.to_le_bytes());note.extend(0u32.to_le_bytes());note.extend(0u32.to_le_bytes());
        let mut exif=IfdBuilder::new();exif.set(t::MAKER_NOTE,Value::Undefined(note));
        let mut root=IfdBuilder::new();root.set(t::COMPRESSION,Value::Short(vec![6]));root.set(t::IMAGE_WIDTH,Value::Long(vec![1]));root.set(t::IMAGE_LENGTH,Value::Long(vec![1]));
        root.set_image(ImageData::Strips {rows_per_strip:1,strips:vec![j]});root.set_child(t::EXIF_IFD,exif);
        let xmp=lightcraft_meta::write_xmp(&lightcraft_meta::Metadata {orientation:Some(Orientation::Rotate270),..Default::default()},None);
        root.set(t::XMP,Value::Byte(xmp.into_bytes()));
        let bytes=TiffWriter::default().write(&[root]).unwrap();
        let actual=embedded_preview_reader(&mut Cursor::new(&bytes)).unwrap();
        assert_eq!(actual.jpeg,crate::embedded_preview(&bytes).unwrap());
        assert_eq!(Some(actual.orientation),lightcraft_meta::extract(&bytes).orientation);
    }
}
