//! Compact region storage for image tracing.
//!
//! Ported from visioncortex @ 0062088c89645aac76c00e066deb7e8f53980dd7.
// Copyright (c) 2026 TSANG, Hao Fung, visioncortex contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/visioncortex-LICENSE-MIT and -APACHE.
use super::Cluster;

pub struct Clusters {
    pub width: u32,
    pub height: u32,
    pub(crate) clusters: Vec<Cluster>,
    pub(crate) cluster_indices: Vec<ClusterIndex>,
    pub(crate) clusters_output: Vec<ClusterIndex>, // valid outputs. Valid outputs are clusters with at least one pixel.
}

#[derive(Copy, Clone, Default, Eq, Ord, Hash, PartialEq, PartialOrd)]
pub struct ClusterIndex(pub ClusterIndexElem);

pub type ClusterIndexElem = u32;

impl Clusters {
    pub fn view(&self) -> ClustersView<'_> {
        ClustersView {
            width: self.width,
            height: self.height,
            clusters: &self.clusters,
            cluster_indices: &self.cluster_indices,
            clusters_output: &self.clusters_output,
        }
    }
}

pub struct ClustersView<'a> {
    pub width: u32,
    pub height: u32,
    pub clusters: &'a [Cluster],
    pub cluster_indices: &'a [ClusterIndex],
    pub clusters_output: &'a [ClusterIndex],
}

impl ClustersView<'_> {
    pub fn get_cluster(&self, index: ClusterIndex) -> &Cluster {
        &self.clusters[index.0 as usize]
    }

    pub fn get_cluster_at(&self, index: u32) -> ClusterIndex {
        self.cluster_indices[index as usize]
    }
}
