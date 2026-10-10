//! VectorCraft centreline Grid and topology-preserving Zhang–Suen / Yokoi thinning.
// Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
use super::Control;
use crate::Error;
use kurbo::Point;

pub(super) fn neighbours(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((i % w) as i64, (i / w) as i64);
    const AROUND: [(i64, i64); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
    AROUND.into_iter().filter_map(move |(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        (nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64).then(|| ny as usize * w + nx as usize)
    })
}

pub(super) struct Grid {
    pub on: Vec<bool>,
    pub w: usize,
    pub h: usize,
    x0: i64,
    y0: i64,
}
impl Grid {
    pub fn new(comp: &[usize], w: usize, c: &mut Control<'_, impl Fn() -> bool>) -> Result<Self, Error> {
        let mut bounds = (w, 0, usize::MAX, 0);
        for (k, &i) in comp.iter().enumerate() {
            if k.is_multiple_of(1024) {
                c.work(1024)?;
            }
            bounds.0 = bounds.0.min(i % w);
            bounds.1 = bounds.1.max(i % w);
            bounds.2 = bounds.2.min(i / w);
            bounds.3 = bounds.3.max(i / w);
        }
        let (x0, x1, y0, y1) = bounds;
        let (gw, gh) = (x1 - x0 + 3, y1 - y0 + 3);
        let mut g = Self { on: vec![false; gw * gh], w: gw, h: gh, x0: x0 as i64 - 1, y0: y0 as i64 - 1 };
        for (k, &i) in comp.iter().enumerate() {
            if k.is_multiple_of(1024) {
                c.work(1024)?;
            }
            g.on[(i / w - y0 + 1) * gw + i % w - x0 + 1] = true;
        }
        Ok(g)
    }
    pub fn ring(&self, i: usize) -> [bool; 8] {
        let w = self.w;
        let g = |j: usize| self.on[j];
        [g(i - w), g(i - w + 1), g(i + 1), g(i + w + 1), g(i + w), g(i + w - 1), g(i - 1), g(i - w - 1)]
    }
    pub fn interior(&self) -> impl Iterator<Item = usize> + use<> {
        let (w, h) = (self.w, self.h);
        (1..h - 1).flat_map(move |y| (1..w - 1).map(move |x| y * w + x))
    }
    pub fn neighbours(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let w = self.w;
        [i - w, i - w + 1, i + 1, i + w + 1, i + w, i + w - 1, i - 1, i - w - 1].into_iter().filter(|&j| self.on[j])
    }
    pub fn degree(&self, i: usize) -> usize {
        self.neighbours(i).count()
    }
    pub fn centre(&self, i: usize) -> Point {
        Point::new((self.x0 + (i % self.w) as i64) as f64 + 0.5, (self.y0 + (i / self.w) as i64) as f64 + 0.5)
    }
    pub fn image_index(&self, i: usize, width: usize) -> usize {
        (self.y0 + (i / self.w) as i64) as usize * width + (self.x0 + (i % self.w) as i64) as usize
    }
    pub fn thin(&mut self, c: &mut Control<'_, impl Fn() -> bool>) -> Result<(), Error> {
        let removable = |p: &[bool; 8]| {
            let b = p.iter().filter(|v| **v).count();
            let a = (0..8).filter(|&k| !p[k] && p[(k + 1) % 8]).count();
            (2..=6).contains(&b) && a == 1
        };
        let mut picked = Vec::new();
        loop {
            let mut changed = false;
            for step in 0..2 {
                picked.clear();
                for (k, i) in self.interior().enumerate() {
                    if k.is_multiple_of(1024) {
                        c.work(1024)?;
                    }
                    if !self.on[i] {
                        continue;
                    }
                    let p = self.ring(i);
                    let (n, e, s, w) = (p[0], p[2], p[4], p[6]);
                    let side = if step == 0 { !(e && s && (n || w)) } else { !(n && w && (e || s)) };
                    if side && removable(&p) {
                        picked.push(i);
                    }
                }
                for (k, &i) in picked.iter().enumerate() {
                    if k.is_multiple_of(1024) {
                        c.work(1024)?;
                    }
                    if removable(&self.ring(i)) {
                        self.on[i] = false;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        loop {
            let mut changed = false;
            for (k, i) in self.interior().enumerate() {
                if k.is_multiple_of(1024) {
                    c.work(1024)?;
                }
                if !self.on[i] || self.degree(i) < 2 {
                    continue;
                }
                let p = self.ring(i);
                // A real orthogonal T/cross is not staircase noise. Preserve
                // its meeting pixel even when diagonal neighbours stay connected.
                if [p[0], p[2], p[4], p[6]].iter().filter(|&&v| v).count() >= 3 {
                    continue;
                }
                let x = [p[2], p[1], p[0], p[7], p[6], p[5], p[4], p[3]];
                let off = |k: usize| !x[k % 8];
                let c8: usize = [0, 2, 4, 6].iter().map(|&k| usize::from(off(k)) - usize::from(off(k) && off(k + 1) && off(k + 2))).sum();
                if c8 == 1 {
                    self.on[i] = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(())
    }
}
