//! Pure deterministic grouping. Keys are independent of slice position so reordering worker
//! results cannot change labels, tie breaks, centroids or the numbering of suggestions.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};

use super::{ASSIGN_COSINE, ASSIGN_MARGIN, CLUSTER_COSINE, CLUSTER_PASSES};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeKey {
    /// Capture time in a single caller-defined epoch/unit; use the same sentinel for missing dates.
    pub photo_date: i64,
    /// Stable photo/content key (virtual copies should be deduplicated by the caller).
    pub key: String,
    pub face_index: usize,
}

#[derive(Clone, Debug)]
pub struct FaceNode {
    pub key: NodeKey,
    pub embedding: [f32; 128],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    /// Stable final label, the key of the node which originally owned the winning label.
    pub label: NodeKey,
    /// Ordered by NodeKey. Singletons are excluded from returned suggestions.
    pub members: Vec<NodeKey>,
    pub centroid: [f32; 128],
}

/// Reject zero/nonfinite vectors; double precision prevents overflow for finite f32 inputs.
pub fn normalise_embedding(embedding: &[f32; 128]) -> Result<[f32; 128]> {
    ensure!(embedding.iter().all(|v| v.is_finite()), "Nonfinite face embedding");
    let norm = embedding.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
    ensure!(norm > 0.0, "Zero face embedding");
    Ok(embedding.map(|v| (f64::from(v) / norm) as f32))
}

/// Actual cosine, also for callers supplying non-unit embeddings.
pub fn cosine(a: &[f32; 128], b: &[f32; 128]) -> Result<f32> {
    ensure!(a.iter().chain(b).all(|v| v.is_finite()), "Nonfinite face embedding");
    let aa = a.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
    let bb = b.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
    ensure!(aa > 0.0 && bb > 0.0, "Zero face embedding");
    let ab = a.iter().zip(b).map(|(a, b)| f64::from(*a) * f64::from(*b)).sum::<f64>();
    Ok((ab / (aa.sqrt() * bb.sqrt())).clamp(-1.0, 1.0) as f32)
}

/// Chinese Whispers: edges cosine ≥ .50, labels initially sorted node indices, sequential
/// in-place updates in (date, key, face index) order. Largest summed neighbour weight wins;
/// exact ties choose the smallest label. Stop at stability or after 30 passes.
/// Suggestions sort by descending size, then the smallest member key for equal sizes.
pub fn chinese_whispers(nodes: &[FaceNode]) -> Result<Vec<Cluster>> {
    let mut nodes: Vec<_> = nodes.iter().collect();
    nodes.sort_by(|a, b| a.key.cmp(&b.key));
    ensure!(nodes.windows(2).all(|p| p[0].key != p[1].key), "Duplicate face node key");
    let embeddings = nodes.iter().map(|n| normalise_embedding(&n.embedding)).collect::<Result<Vec<_>>>()?;
    let mut edges = vec![Vec::<(usize, f64)>::new(); nodes.len()];
    for i in 0..nodes.len() {
        for j in i + 1..nodes.len() {
            let weight = cosine(&embeddings[i], &embeddings[j])?;
            if weight >= CLUSTER_COSINE {
                edges[i].push((j, f64::from(weight)));
                edges[j].push((i, f64::from(weight)));
            }
        }
    }
    let labels = whisper_labels(&edges);
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, label) in labels.into_iter().enumerate() {
        groups.entry(label).or_default().push(index);
    }
    let mut clusters = Vec::new();
    for (label, indices) in groups {
        if indices.len() < 2 {
            continue;
        }
        let sum: [f32; 128] = std::array::from_fn(|c| (indices.iter().map(|i| f64::from(embeddings[*i][c])).sum::<f64>() / indices.len() as f64) as f32);
        clusters.push(Cluster {
            label: nodes[label].key.clone(),
            members: indices.iter().map(|i| nodes[*i].key.clone()).collect(),
            centroid: normalise_embedding(&sum)?,
        });
    }
    clusters.sort_by(|a, b| b.members.len().cmp(&a.members.len()).then_with(|| a.members[0].cmp(&b.members[0])));
    Ok(clusters)
}

fn whisper_labels(edges: &[Vec<(usize, f64)>]) -> Vec<usize> {
    let mut labels: Vec<_> = (0..edges.len()).collect();
    for _ in 0..CLUSTER_PASSES {
        let mut changed = false;
        for (i, neighbours) in edges.iter().enumerate() {
            let mut weights = BTreeMap::<usize, f64>::new();
            for &(other, weight) in neighbours {
                *weights.entry(labels[other]).or_default() += weight;
            }
            let mut best = None;
            for (label, weight) in weights {
                // Iteration is ascending label; equality deliberately preserves the first.
                if best.is_none_or(|(_, current)| weight > current) {
                    best = Some((label, weight));
                }
            }
            if let Some((label, _)) = best {
                changed |= labels[i] != label;
                labels[i] = label;
            }
        }
        if !changed {
            break;
        }
    }
    labels
}

#[derive(Clone, Debug)]
pub struct NamedPerson {
    /// Stable person id, not the display name.
    pub id: String,
    pub centroid: [f32; 128],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    pub person: String,
    pub cosine: f32,
}

/// The best candidate must pass .45 and lead the runner-up by .05. Rejected pairs remain in
/// the ranking: rejection of the best match leaves the face unassigned, never forces a second
/// choice. With only one named person there is no runner-up/margin constraint.
pub fn assign_person(key: &NodeKey, embedding: &[f32; 128], people: &[NamedPerson], rejected: &BTreeSet<(NodeKey, String)>) -> Result<Option<Assignment>> {
    assign(embedding, people, |id| rejected.contains(&(key.clone(), id.to_owned())))
}

/// A rejection for any member vetoes assigning the entire cluster to that person. Callers may
/// then try individual members to preserve already confirmed exclusions.
pub fn assign_cluster(cluster: &Cluster, people: &[NamedPerson], rejected: &BTreeSet<(NodeKey, String)>) -> Result<Option<Assignment>> {
    assign(&cluster.centroid, people, |id| cluster.members.iter().any(|key| rejected.contains(&(key.clone(), id.to_owned()))))
}

fn assign(embedding: &[f32; 128], people: &[NamedPerson], rejected: impl Fn(&str) -> bool) -> Result<Option<Assignment>> {
    let embedding = normalise_embedding(embedding)?;
    let mut ranked = people.iter().map(|p| Ok((p.id.as_str(), cosine(&embedding, &p.centroid)?))).collect::<Result<Vec<_>>>()?;
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let unique: BTreeSet<_> = people.iter().map(|p| &p.id).collect();
    ensure!(unique.len() == people.len(), "Duplicate named person id");
    let Some(&(id, score)) = ranked.first() else { return Ok(None) };
    let margin = ranked.get(1).is_none_or(|p| score - p.1 >= ASSIGN_MARGIN);
    Ok((score >= ASSIGN_COSINE && margin && !rejected(id)).then(|| Assignment { person: id.to_owned(), cosine: score }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whispers_sum_weights_choose_smallest_label_and_update_in_place() {
        // Node 0 sees an exact tie between labels 1 and 2 -> 1. Node 1 then sees the
        // *updated* label 1 at node 0 and label 2 -> 1. Node 2's summed label 1 wins.
        let edges = vec![vec![(1, 0.5), (2, 0.5)], vec![(0, 0.5), (2, 0.5)], vec![(0, 0.5), (1, 0.5)], vec![]];
        assert_eq!(whisper_labels(&edges), vec![1, 1, 1, 3]);
        assert_eq!(CLUSTER_PASSES, 30);
    }
}
