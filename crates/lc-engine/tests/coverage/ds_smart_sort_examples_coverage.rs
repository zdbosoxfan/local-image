use lightcraft_engine::smart_sort::examples;

#[test]
fn exemplar_prototype_single_embedding_returns_normalized() {
    let e = [3.0, 4.0];
    let p = examples::exemplar_prototype(&[&e]).unwrap();
    assert_eq!(p.len(), 2);
    assert!((p[0] - 0.6).abs() < 1e-6);
    assert!((p[1] - 0.8).abs() < 1e-6);
}

#[test]
fn exemplar_prototype_averages_multiple_embeddings() {
    let e1 = [1.0, 0.0];
    let e2 = [0.0, 1.0];
    let p = examples::exemplar_prototype(&[&e1, &e2]).unwrap();
    assert_eq!(p.len(), 2);
    let norm = (p[0] * p[0] + p[1] * p[1]).sqrt();
    assert!((norm - 1.0).abs() < 1e-6);
    assert!((p[0] - p[1]).abs() < 1e-6);
}

#[test]
fn exemplar_prototype_rejects_empty_slice() {
    assert!(examples::exemplar_prototype(&[]).is_none());
}

#[test]
fn exemplar_prototype_rejects_mismatched_dimensions() {
    let e1 = [1.0, 0.0];
    let e2 = [0.0, 1.0, 0.0];
    assert!(examples::exemplar_prototype(&[&e1, &e2]).is_none());
}

#[test]
fn exemplar_prototype_rejects_zero_vector() {
    assert!(examples::exemplar_prototype(&[&[0.0, 0.0]]).is_none());
}

#[test]
fn exemplar_prototype_returns_none_for_non_finite_inputs() {
    let nan = [f32::NAN, 0.0];
    let inf = [f32::INFINITY, 1.0];
    assert!(examples::exemplar_prototype(&[&nan]).is_none());
    assert!(examples::exemplar_prototype(&[&inf]).is_none());

    let a = [1.0, f32::NAN];
    let b = [2.0, 3.0];
    assert!(examples::exemplar_prototype(&[&a, &b]).is_none());
}

#[test]
fn combine_text_and_exemplars_returns_weighted_normalized() {
    let text = [1.0, 0.0];
    let ex = [0.0, 1.0];
    let p = examples::combine(Some(&text), &[&ex]).unwrap();

    let norm = (1.0f32 + 0.04).sqrt();
    assert!((p[0] - 1.0 / norm).abs() < 1e-6);
    assert!((p[1] - 0.2 / norm).abs() < 1e-6);
}

#[test]
fn combine_without_text_returns_exemplar_prototype() {
    let ex = [0.0, 2.0];
    let p = examples::combine(None, &[&ex]).unwrap();
    assert_eq!(p.len(), 2);
    assert!((p[0] - 0.0).abs() < 1e-6);
    assert!((p[1] - 1.0).abs() < 1e-6);
}

#[test]
fn combine_text_only_returns_normalized_text() {
    let text = [3.0, 4.0];
    let p = examples::combine(Some(&text), &[]).unwrap();
    assert!((p[0] - 0.6).abs() < 1e-6);
    assert!((p[1] - 0.8).abs() < 1e-6);
}

#[test]
fn combine_mismatched_text_and_exemplar_dimensions_returns_none() {
    let text = [1.0, 0.0];
    let ex = [0.0, 1.0, 0.0];
    assert!(examples::combine(Some(&text), &[&ex]).is_none());
}

#[test]
fn combine_with_non_finite_text_returns_none() {
    let nan_text = [f32::NAN, 0.0];
    let good_ex = [1.0, 0.0];
    assert!(examples::combine(Some(&nan_text), &[&good_ex]).is_none());
}

#[test]
fn combine_with_non_finite_exemplars_falls_back_to_text() {
    let text = [3.0, 4.0];
    let nan_ex = [f32::NAN, 0.0];
    let p = examples::combine(Some(&text), &[&nan_ex]).unwrap();
    assert!((p[0] - 0.6).abs() < 1e-6);
    assert!((p[1] - 0.8).abs() < 1e-6);
}

#[test]
fn rank_orders_by_cosine_descending() {
    let proto = [1.0, 0.0, 0.0];
    let c0 = [1.0, 0.0, 0.0];
    let c1 = [0.0, 1.0, 0.0];
    let c2 = [0.6, 0.8, 0.0];
    let candidates = [&c0[..], &c1[..], &c2[..]];

    let ranked = examples::rank(&proto, &candidates);
    let indices: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();

    assert_eq!(indices, vec![0, 2, 1]);
    assert!((ranked[0].1 - 1.0).abs() < 1e-6);
    assert!((ranked[1].1 - 0.6).abs() < 1e-6);
    assert!((ranked[2].1 - 0.0).abs() < 1e-6);
}

#[test]
fn rank_ties_are_stable_by_index() {
    let proto = [1.0, 0.0];
    let c0 = [1.0, 0.0];
    let c1 = [2.0, 0.0];
    let candidates = [&c0[..], &c1[..], &c0[..]];

    let ranked = examples::rank(&proto, &candidates);
    let indices: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();

    assert_eq!(indices, vec![0, 1, 2]);
}

#[test]
fn rank_with_zero_prototype_assigns_zero_cosine() {
    let proto = [0.0, 0.0];
    let c0 = [1.0, 0.0];
    let c1 = [0.0, 1.0];

    let ranked = examples::rank(&proto, &[&c0, &c1]);
    assert_eq!(ranked.len(), 2);

    for (_, cosine) in ranked.iter() {
        assert_eq!(*cosine, 0.0);
    }

    let indices: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
    assert_eq!(indices, vec![0, 1]);
}

#[test]
fn rank_empty_candidates_returns_empty() {
    let proto = [1.0, 0.0];
    let candidates: Vec<&[f32]> = vec![];
    assert!(examples::rank(&proto, &candidates).is_empty());
}

#[test]
fn rank_outputs_finite_and_bounded_for_finite_inputs() {
    let proto = [1.0, 1.0, 1.0];
    let c0 = [2.0, -3.0, 4.0];
    let c1 = [0.5, 0.25, -1.0];
    let c2 = [10.0, 20.0, 30.0];

    let ranked = examples::rank(&proto, &[&c0, &c1, &c2]);
    assert_eq!(ranked.len(), 3);

    for (_, cosine) in ranked {
        assert!(cosine.is_finite());
        assert!((-1.0..=1.0).contains(&cosine));
    }
}

#[test]
fn rank_does_not_panic_with_nan_candidate() {
    let proto = [1.0, 0.0];
    let c0 = [f32::NAN, 0.0];
    let c1 = [0.0, 1.0];

    let ranked = examples::rank(&proto, &[&c0, &c1]);
    assert_eq!(ranked.len(), 2);

    let mut indices: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
    indices.sort_unstable();
    assert_eq!(indices, vec![0, 1]);
}

#[test]
fn rank_does_not_panic_with_infinite_candidate() {
    let proto = [1.0, 0.0];
    let c0 = [f32::INFINITY, 0.0];
    let c1 = [0.0, f32::INFINITY];

    let ranked = examples::rank(&proto, &[&c0, &c1]);
    assert_eq!(ranked.len(), 2);

    let mut indices: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
    indices.sort_unstable();
    assert_eq!(indices, vec![0, 1]);
}

#[test]
fn suggest_name_empty_returns_new_folder() {
    assert_eq!(examples::suggest_name(&[]), "New Folder");
}

#[test]
fn suggest_name_ignores_unsorted_and_empty_keywords() {
    let keywords = vec![vec!["Unsorted".to_string(), "".to_string(), " ".to_string()], vec!["unsorted".to_string()]];
    assert_eq!(examples::suggest_name(&keywords), "New Folder");
}

#[test]
fn suggest_name_case_insensitive_counts() {
    let keywords = vec![vec!["Cat".to_string(), "dog".to_string()], vec!["cat".to_string(), "bird".to_string()], vec!["CAT".to_string()]];
    assert_eq!(examples::suggest_name(&keywords), "cat");
}

#[test]
fn suggest_name_ties_break_alphabetically() {
    let keywords = vec![vec!["zebra".to_string()], vec!["apple".to_string()], vec!["zebra".to_string()], vec!["apple".to_string()]];
    assert_eq!(examples::suggest_name(&keywords), "apple");
}

#[test]
fn suggest_name_uses_last_hierarchical_component() {
    let keywords = vec![vec!["Topics|Speakers".to_string(), "speakers".to_string()], vec!["Topics|Speakers".to_string()], vec!["Other".to_string()]];
    assert_eq!(examples::suggest_name(&keywords), "speakers");
}
