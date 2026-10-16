use super::*;
use std::sync::atomic::Ordering;
struct Pixels<'a> {
    data: Vec<bool>,
    rows: usize,
    limit: usize,
    cancel_after_row: bool,
    cancel: &'a AtomicBool,
}
impl Decoder for Pixels<'_> {
    fn push_pixel(&mut self, white: bool) -> Result<()> {
        if self.data.len() >= self.limit {
            return Err(DecodeError::OutputLimit);
        }
        self.data.push(white);
        Ok(())
    }
    fn push_pixel_chunk(&mut self, white: bool, count: u32) -> Result<()> {
        for _ in 0..count * 8 {
            self.push_pixel(white)?;
        }
        Ok(())
    }
    fn next_line(&mut self) -> Result<()> {
        self.rows += 1;
        if self.cancel_after_row {
            self.cancel.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}
fn settings() -> DecodeSettings {
    DecodeSettings {
        columns: 8,
        rows: 4,
        end_of_block: true,
        end_of_line: false,
        rows_are_byte_aligned: false,
        encoding: EncodingMode::Group4,
        invert_black: false,
    }
}
fn data() -> Vec<u8> {
    // T.6: white row V0; horizontal white0/black8; repeat black V0/V0;
    // white row Pass; EOFB. The sample expectation is independent of tables.
    let bits = b"100100110101000101110001000000000001000000000001";
    bits.chunks(8).map(|chunk| chunk.iter().enumerate().fold(0, |v, (i, b)| v | u8::from(*b == b'1') << (7 - i))).collect()
}
#[test]
fn ccitt_known_rows_fallible_output_and_mid_decode_cancel() {
    let flag = AtomicBool::new(false);
    let mut sink = Pixels { data: vec![], rows: 0, limit: 32, cancel_after_row: false, cancel: &flag };
    assert!(decode(&data(), &mut sink, &mut DecoderContext::new(settings()), &flag, 10000).is_ok());
    assert_eq!(sink.rows, 4);
    assert_eq!(sink.data, [vec![true; 8], vec![false; 16], vec![true; 8]].concat());
    sink.data.clear();
    sink.rows = 0;
    sink.limit = 7;
    assert_eq!(decode(&data(), &mut sink, &mut DecoderContext::new(settings()), &flag, 10000), Err(DecodeError::OutputLimit));
    sink.data.clear();
    sink.rows = 0;
    sink.limit = 32;
    sink.cancel_after_row = true;
    assert_eq!(decode(&data(), &mut sink, &mut DecoderContext::new(settings()), &flag, 10000), Err(DecodeError::Cancelled));
    assert_eq!(sink.rows, 1);
    flag.store(false, Ordering::Relaxed);
    sink.cancel_after_row = false;
    assert_eq!(decode(&data(), &mut sink, &mut DecoderContext::new(settings()), &flag, 1), Err(DecodeError::WorkLimit));
}

#[test]
fn ccitt_zero_length_runs_exhaust_work_without_output_growth() {
    let flag = AtomicBool::new(false);
    let bits = "001101010000110111".repeat(256); // white0, black0, repeatedly
    let data: Vec<u8> = bits.as_bytes().chunks(8).map(|chunk| chunk.iter().enumerate().fold(0, |v, (i, b)| v | u8::from(*b == b'1') << (7 - i))).collect();
    let mut settings = settings();
    settings.rows = 1;
    settings.encoding = EncodingMode::Group3_1D;
    let mut sink = Pixels { data: vec![], rows: 0, limit: 8, cancel_after_row: false, cancel: &flag };
    assert_eq!(decode(&data, &mut sink, &mut DecoderContext::new(settings), &flag, 500), Err(DecodeError::WorkLimit));
    assert!(sink.data.is_empty());
}
