//! Ported from visioncortex @ 0062088c89645aac76c00e066deb7e8f53980dd7.
// Copyright (c) 2026 TSANG, Hao Fung, visioncortex contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/visioncortex-LICENSE-MIT and -APACHE.
//! Algorithm to cluster a color image and build a tree of clusters
//!
//! The hierarchical structure resembles the human visual cortex.
//!
//! To support interactivity, components follow a state-machine model:
//!
//! + new(): creation of placeholder object
//! + init(): resource allocation
//! + tick() -> bool: computation. returning false to continue, returning true when finish
//! + result() -> T: cleanup & collect results

mod builder;
mod cluster;
mod container;
mod runner;

pub use builder::*;
pub use cluster::*;
pub use container::*;
pub use runner::*;
