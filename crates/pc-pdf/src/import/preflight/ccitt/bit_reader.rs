//! Adapted from hayro-ccitt 0.3.0, commit 4ad1532e1cd719170c4f4baf669f13ec0f406abb.
//! Copyright (c) The Hayro Authors. MIT; see licenses/hayro-ccitt-MIT.txt.
//! Local changes: fallible output, strict rows, cancellation and bounded work.
//! Bit-level reader for CCITT encoded data streams.

use super::{DecodeError, Result};
use core::fmt::Debug;
use std::{
    cell::Cell,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone)]
pub(crate) struct BitReader<'a> {
    data: &'a [u8],
    bit_offset: usize,
    cancel: &'a AtomicBool,
    remaining: &'a Cell<usize>,
}

impl<'a> BitReader<'a> {
    #[inline(always)]
    pub(crate) fn new(data: &'a [u8], cancel: &'a AtomicBool, remaining: &'a Cell<usize>) -> Self {
        Self { data, bit_offset: 0, cancel, remaining }
    }

    pub(crate) fn step(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(DecodeError::Cancelled);
        }
        let next = self.remaining.get().checked_sub(1).ok_or(DecodeError::WorkLimit)?;
        self.remaining.set(next);
        Ok(())
    }
    #[inline(always)]
    pub(crate) fn read_bit(&mut self) -> Result<u32> {
        self.step()?;
        let byte_pos = self.byte_pos();
        let byte = *self.data.get(byte_pos).ok_or(DecodeError::UnexpectedEof)? as u32;
        let shift = 7 - self.bit_pos();
        self.bit_offset += 1;
        Ok((byte >> shift) & 1)
    }

    #[inline(always)]
    pub(crate) fn read_bits(&mut self, num_bits: usize) -> Result<u32> {
        let mut result = 0_u32;

        for i in (0..num_bits).rev() {
            result |= (self.read_bit()?) << i;
        }

        Ok(result)
    }

    #[inline(always)]
    pub(crate) fn peak_bits(&mut self, num_bits: usize) -> Result<u32> {
        self.clone().read_bits(num_bits)
    }

    #[inline(always)]
    pub(crate) fn align(&mut self) {
        let bit_pos = self.bit_pos();

        if !bit_pos.is_multiple_of(8) {
            self.bit_offset += 8 - bit_pos;
        }
    }

    #[inline(always)]
    pub(crate) fn at_end(&self) -> bool {
        self.byte_pos() >= self.data.len()
    }

    pub(crate) fn bit_offset(&self) -> usize {
        self.bit_offset
    }

    #[inline(always)]
    pub(crate) fn byte_pos(&self) -> usize {
        self.bit_offset >> 3
    }

    #[inline(always)]
    fn bit_pos(&self) -> usize {
        self.bit_offset & 7
    }
}
