//! Colour accumulators for hierarchical image tracing.
//!
//! Ported from visioncortex @ 0062088c89645aac76c00e066deb7e8f53980dd7.
// Copyright (c) 2026 TSANG, Hao Fung, visioncortex contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/visioncortex-LICENSE-MIT and -APACHE.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self::new_rgba(r, g, b, 255)
    }
    pub fn new_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub fn rgb_u8(&self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
}
#[derive(Copy, Clone, Default, Debug)]
pub struct ColorI32 {
    pub r: i32,
    pub g: i32,
    pub b: i32,
}
impl ColorI32 {
    pub fn new(c: &Color) -> Self {
        Self { r: c.r.into(), g: c.g.into(), b: c.b.into() }
    }
    pub fn diff(&self, b: &Self) -> Self {
        Self { r: self.r - b.r, g: self.g - b.g, b: self.b - b.b }
    }
}
#[derive(Copy, Clone, Default, Debug)]
pub struct ColorSum {
    pub r: u64,
    pub g: u64,
    pub b: u64,
    pub a: u64,
    pub counter: u64,
}
impl ColorSum {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add(&mut self, color: &Color) {
        self.r += color.r as u64;
        self.g += color.g as u64;
        self.b += color.b as u64;
        self.a += color.a as u64;
        self.counter += 1;
    }

    /// Caution: might underflow
    pub fn sub(&mut self, color: &Color) {
        self.r -= color.r as u64;
        self.g -= color.g as u64;
        self.b -= color.b as u64;
        self.a -= color.a as u64;
        self.counter -= 1;
    }

    pub fn merge(&mut self, other: &ColorSum) {
        self.r += other.r;
        self.g += other.g;
        self.b += other.b;
        self.a += other.a;
        self.counter += other.counter;
    }

    /// Caution: might underflow
    pub fn unmerge(&mut self, other: &ColorSum) {
        self.r -= other.r;
        self.g -= other.g;
        self.b -= other.b;
        self.a -= other.a;
        self.counter -= other.counter;
    }

    pub fn average(&self) -> Color {
        Color::new_rgba(
            (self.r / self.counter.max(1)) as u8,
            (self.g / self.counter.max(1)) as u8,
            (self.b / self.counter.max(1)) as u8,
            (self.a / self.counter.max(1)) as u8,
        )
    }

    pub fn clear(&mut self) {
        self.r = 0;
        self.g = 0;
        self.b = 0;
        self.a = 0;
        self.counter = 0;
    }
}
