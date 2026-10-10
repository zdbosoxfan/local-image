//! Rejoin near-colour gradient faces introduced by stacked clustering.
// Adapted from visioncortex/vtracer crates/vtracer/src/mosaic.rs::LabelMap::merge_similar
// at 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and licenses/vtracer-LICENSE-APACHE.
use super::{LabelMap, OUTSIDE};
use crate::Error;
use std::collections::BTreeSet;
fn root(parent: &mut [usize], mut id: usize) -> usize {
    while parent[id] != id {
        parent[id] = parent[parent[id]];
        id = parent[id];
    }
    id
}
fn difference(a: [u64; 3], na: u64, b: [u64; 3], nb: u64) -> i32 {
    (0..3).map(|k| ((a[k] / na.max(1)) as i64 - (b[k] / nb.max(1)) as i64).abs() as i32).sum()
}
pub(super) fn similar(map: &mut LabelMap, max_diff: i32, cancel: &dyn Fn() -> bool) -> Result<(), Error> {
    let n = map.colors.len();
    if max_diff <= 0 || n < 2 {
        return Ok(());
    }
    let mut areas = vec![0u64; n];
    let mut pairs = BTreeSet::new();
    for (i, &id) in map.labels.iter().enumerate() {
        if i % 1024 == 0 && cancel() {
            return Err(Error::Cancelled);
        }
        if id == OUTSIDE {
            continue;
        }
        areas[id as usize] += 1;
        let x = (i % map.width as usize) as i32;
        let y = (i / map.width as usize) as i32;
        for b in [map.label(x + 1, y), map.label(x, y + 1)] {
            if b != OUTSIDE && b != id {
                pairs.insert((id.min(b) as usize, id.max(b) as usize));
            }
        }
    }
    let mut sums: Vec<[u64; 3]> = map.colors.iter().zip(&areas).map(|(c, n)| [u64::from(c[0]) * n, u64::from(c[1]) * n, u64::from(c[2]) * n]).collect();
    let mut pairs: Vec<_> = pairs.into_iter().collect();
    pairs.sort_by_key(|&(a, b)| (difference(sums[a], areas[a], sums[b], areas[b]), a, b));
    let mut parent: Vec<_> = (0..n).collect();
    loop {
        if cancel() {
            return Err(Error::Cancelled);
        }
        let mut changed = false;
        for &(a, b) in &pairs {
            let a = root(&mut parent, a);
            let b = root(&mut parent, b);
            if a == b || difference(sums[a], areas[a], sums[b], areas[b]) > max_diff {
                continue;
            }
            // Highest original paint id is the final occurrence: preserve T1
            // deterministic back-to-front paint order after gradient merging.
            let (keep, drop) = (a.max(b), a.min(b));
            parent[drop] = keep;
            for k in 0..3 {
                sums[keep][k] += sums[drop][k];
            }
            areas[keep] += areas[drop];
            changed = true;
        }
        if !changed {
            break;
        }
    }
    let mut remap = vec![OUTSIDE; n];
    let mut colors = Vec::new();
    for id in 0..n {
        if root(&mut parent, id) == id && areas[id] > 0 {
            remap[id] = colors.len() as u32;
            colors.push([(sums[id][0] / areas[id]) as u8, (sums[id][1] / areas[id]) as u8, (sums[id][2] / areas[id]) as u8, 255]);
        }
    }
    for (i, id) in map.labels.iter_mut().enumerate() {
        if i % 1024 == 0 && cancel() {
            return Err(Error::Cancelled);
        }
        if *id != OUTSIDE {
            *id = remap[root(&mut parent, *id as usize)];
        }
    }
    map.colors = colors;
    Ok(())
}
