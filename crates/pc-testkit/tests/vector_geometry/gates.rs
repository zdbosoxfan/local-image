use super::*;
mod svg_reference;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn geometry31_ratchet_shrinks_and_missing_goldens_never_pass() -> TestResult {
    let cases = vec![("good".into(), Outcome::Passed), ("bad".into(), Outcome::Failed("dropped hole".into()))];
    let ratchet = vec![("bad".into(), "known dropped hole".into())];
    assert_eq!(check_ratchet(&cases, &ratchet, 0.5)?, CorpusSummary { total: 2, passed: 1, ratcheted: 1 });
    assert!(check_ratchet(&cases, &[], 0.5).is_err());
    assert!(check_ratchet(&cases, &ratchet, 0.95).is_err());
    let repaired = vec![("good".into(), Outcome::Passed), ("bad".into(), Outcome::Passed)];
    assert!(check_ratchet(&repaired, &ratchet, 0.95).is_err());
    assert_eq!(check_ratchet(&repaired, &[], 0.95)?.passed, 2);
    let missing = vec![("bad".into(), Outcome::MissingReference("Inkscape SVG absent".into()))];
    assert!(check_ratchet(&missing, &ratchet, 0.0).is_err());
    assert!(check_ratchet(&cases, &[("bad".into(), " ".into())], 0.5).is_err());
    assert!(check_ratchet(&cases, &[ratchet[0].clone(), ratchet[0].clone()], 0.5).is_err());
    assert!(check_ratchet(&cases, &[("unknown".into(), "reason".into())], 0.5).is_err());
    assert!(check_ratchet(&[cases[0].clone(), cases[0].clone()], &[], 0.5).is_err());
    assert!(check_ratchet(&[], &[], 0.95).is_err());
    assert!(check_ratchet(&cases, &ratchet, f64::NAN).is_err());
    Ok(())
}

#[test]
fn geometry31_golden_scores_detect_holes_displacement_and_empty_results() -> TestResult {
    let square = photocraft_vector::shapes::rect(4.0, 4.0, 16.0, 16.0);
    let view = Affine::scale(8.0);
    let rect = Rect::new(0, 0, 256, 256);
    let same = compare_regions(std::slice::from_ref(&square), std::slice::from_ref(&square), view, rect)?;
    assert!(same.passes(Thresholds::default()));
    let shifted = square.transform(&Affine::translate(0.25, 0.0));
    let scores = compare_regions(&[shifted], std::slice::from_ref(&square), view, rect)?;
    assert!((scores.hausdorff - 0.25).abs() < 1e-5);
    assert!(!scores.passes(Thresholds::default()));
    let mut hole = square.clone();
    let mut inner = photocraft_vector::shapes::rect(8.0, 8.0, 8.0, 8.0).subpaths.remove(0);
    inner.knots.reverse();
    hole.subpaths.push(inner.with_op(PathOp::Join));
    let scores = compare_regions(std::slice::from_ref(&square), &[hole], view, rect)?;
    assert!((scores.relative_area - 1.0 / 3.0).abs() < 1e-8);
    assert!(!scores.passes(Thresholds::default()));
    assert!(compare_regions(&[], &[], view, rect)?.passes(Thresholds::default()));
    assert!(!compare_regions(&[], std::slice::from_ref(&square), view, rect)?.passes(Thresholds::default()));
    assert!(!compare_regions(&[square], &[], view, rect)?.passes(Thresholds::default()));
    assert!(compare_regions(&[], &[], view, Rect::new(0, 0, 0, 10)).is_err());
    Ok(())
}

#[test]
fn geometry31_gate_rejects_nonfinite_and_enforces_node_budget() -> TestResult {
    let reference = photocraft_vector::shapes::rect(4.0, 4.0, 16.0, 16.0);
    let extra_nodes =
        Path::new(vec![photocraft_doc::Subpath::polygon(&[(4., 4.), (12., 4.), (20., 4.), (20., 12.), (20., 20.), (12., 20.), (4., 20.), (4., 12.)])]);
    let view = Affine::scale(8.0);
    let rect = Rect::new(0, 0, 256, 256);
    let scores = compare_regions(&[extra_nodes], std::slice::from_ref(&reference), view, rect)?;
    assert_eq!(scores.reference_nodes, 4);
    assert_eq!(scores.actual_nodes, 8);
    assert!(!scores.passes(Thresholds::default()));
    assert!(!scores.passes(Thresholds { iou: f64::NAN, ..Default::default() }));
    let mut invalid = reference.clone();
    invalid.subpaths[0].knots[0].anchor.x = f64::INFINITY;
    assert!(compare_regions(&[invalid], &[reference], view, rect).is_err());
    Ok(())
}
