//! Pure logic for creating a Smart Sort category from example photos. Prototypes live in
//! the same L2-normalised embedding space as the store.

use super::store::normalized;

/// Mean of the exemplar embeddings, L2-normalised; None if empty or dimensions differ or the mean is ~zero.
pub fn exemplar_prototype(embeddings: &[&[f32]]) -> Option<Vec<f32>> {
    if embeddings.is_empty() {
        return None;
    }
    let dim = embeddings[0].len();
    if dim == 0 {
        return None;
    }
    let mut sum = vec![0.0; dim];
    for e in embeddings {
        if e.len() != dim {
            return None;
        }
        for (s, v) in sum.iter_mut().zip(e.iter()) {
            *s += *v;
        }
    }
    normalized(sum, dim).ok()
}

/// Combine a text prototype (from tags) with exemplars per spec §5.6 step 2: t = normalise(t + γ·e), γ = min(n,10)/5.
/// With no text prototype, return the exemplar prototype (examples alone define the folder).
pub fn combine(text: Option<&[f32]>, exemplars: &[&[f32]]) -> Option<Vec<f32>> {
    let exemplar = exemplar_prototype(exemplars);
    match (text, exemplar) {
        (Some(t), Some(e)) => {
            if t.len() != e.len() {
                return None;
            }
            let gamma = exemplars.len().min(10) as f32 / 5.0;
            let sum: Vec<f32> = t.iter().zip(e.iter()).map(|(t, e)| t + gamma * e).collect();
            normalized(sum, t.len()).ok()
        }
        (Some(t), None) => normalized(t.to_vec(), t.len()).ok(),
        (None, Some(e)) => Some(e),
        (None, None) => None,
    }
}

/// Rank candidate photos by cosine to the prototype (descending; ties by input order); returns (index, cosine).
pub fn rank(prototype: &[f32], candidates: &[&[f32]]) -> Vec<(usize, f32)> {
    let norm_p = prototype.iter().map(|v| v * v).sum::<f32>().sqrt();
    let mut scored: Vec<(usize, f32)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let norm_c = c.iter().map(|v| v * v).sum::<f32>().sqrt();
            let cosine =
                if norm_p > 0.0 && norm_c > 0.0 { prototype.iter().zip(c.iter()).map(|(a, b)| a * b).sum::<f32>() / (norm_p * norm_c) } else { 0.0 };
            (i, cosine)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    scored
}

/// Suggest a folder name from the most common existing keyword among the exemplars' keywords
/// (case-insensitive, ties → alphabetical), else "New Folder".
pub fn suggest_name(keywords_per_photo: &[Vec<String>]) -> String {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for photo in keywords_per_photo {
        for keyword in photo {
            *counts.entry(keyword.to_lowercase()).or_insert(0) += 1;
        }
    }
    let max = counts.values().copied().max().unwrap_or(0);
    counts
        .into_iter()
        .filter(|(_, count)| *count == max)
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(keyword, _)| keyword)
        .unwrap_or_else(|| "New Folder".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 1e-6, "expected {expected:?}, got {actual:?}");
        }
    }

    #[test]
    fn exemplar_prototype_averages_direction() {
        let e1 = [1.0, 0.0, 0.0];
        let e2 = [1.0, 0.0, 0.0];
        let e3 = [0.6, 0.8, 0.0];
        let proto = exemplar_prototype(&[&e1, &e2, &e3]).unwrap();
        let expected = normalized(vec![2.6, 0.8, 0.0], 3).unwrap();
        assert_close(&proto, &expected);
    }

    #[test]
    fn exemplar_prototype_rejects_empty_mismatched_zero() {
        assert!(exemplar_prototype(&[]).is_none());
        assert!(exemplar_prototype(&[&[1.0, 0.0], &[0.0, 1.0, 0.0]]).is_none());
        assert!(exemplar_prototype(&[&[0.0, 0.0]]).is_none());
    }

    #[test]
    fn combine_text_and_exemplars_uses_gamma() {
        let text = [1.0, 0.0];
        let ex = [0.0, 1.0];
        // one exemplar -> gamma = 1/5 = 0.2
        let combined = combine(Some(&text), &[&ex]).unwrap();
        let expected = normalized(vec![1.0, 0.2], 2).unwrap();
        assert_close(&combined, &expected);
    }

    #[test]
    fn combine_gamma_caps_at_ten_exemplars() {
        let text = [1.0, 0.0];
        let ex = [0.0, 1.0];
        let ten: Vec<&[f32]> = (0..10).map(|_| &ex[..]).collect();
        let fifteen: Vec<&[f32]> = (0..15).map(|_| &ex[..]).collect();
        let c10 = combine(Some(&text), &ten).unwrap();
        let c15 = combine(Some(&text), &fifteen).unwrap();
        let expected = normalized(vec![1.0, 2.0], 2).unwrap();
        assert_close(&c10, &expected);
        assert_close(&c15, &expected);
    }

    #[test]
    fn combine_without_text_returns_exemplar_prototype() {
        let ex = [0.0, 1.0];
        let proto = combine(None, &[&ex]).unwrap();
        assert_close(&proto, &ex);
        assert!(combine(None, &[]).is_none());
    }

    #[test]
    fn combine_text_only_returns_normalized_text() {
        let text = [3.0, 4.0];
        let proto = combine(Some(&text), &[]).unwrap();
        assert_close(&proto, &[0.6, 0.8]);
    }

    #[test]
    fn rank_orders_by_cosine_and_ties_stable() {
        let proto = [1.0, 0.0, 0.0];
        let c0 = [1.0, 0.0, 0.0]; // cosine 1
        let c1 = [0.0, 1.0, 0.0]; // cosine 0
        let c2 = [1.0, 0.0, 0.0]; // tie with c0, later index
        let c3 = [0.6, 0.8, 0.0]; // cosine 0.6
        let candidates = [&c0[..], &c1[..], &c2[..], &c3[..]];
        let ranked = rank(&proto, &candidates);
        let indices: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
        assert_eq!(indices, vec![0, 2, 3, 1]);
        assert_eq!(ranked[0].1, 1.0);
        assert_eq!(ranked[1].1, 1.0);
        assert!(ranked[2].1 > ranked[3].1);
    }

    #[test]
    fn suggest_name_counts_case_insensitive_and_ties_alphabetical() {
        let keywords = vec![vec!["Cat".to_string(), "dog".to_string()], vec!["cat".to_string(), "bird".to_string()]];
        assert_eq!(suggest_name(&keywords), "cat");

        let tied = vec![vec!["zebra".to_string()], vec!["apple".to_string()], vec!["zebra".to_string()], vec!["apple".to_string()]];
        assert_eq!(suggest_name(&tied), "apple");

        assert_eq!(suggest_name(&[]), "New Folder");
    }
}
