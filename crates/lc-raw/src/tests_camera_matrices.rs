//! Independent spot checks against six maker TOMLs in the read-only rawler 0.8.0 archive.
use crate::{ColorData, Mat3, camera_matrices};
#[test]
fn camera_database_matches_toml_spots_and_does_not_replace_file_matrices() {
    for row in include_str!("../tests/fixtures/camera-matrices.csv").lines() {
        let v: Vec<&str> = if let Some(quoted) = row.strip_prefix('"') {
            let (make, rest) = quoted.split_once("\",").unwrap();
            std::iter::once(make).chain(rest.split(',')).collect()
        } else {
            row.split(',').collect()
        };
        let code: u16 = v[2].parse().unwrap();
        let (m, illuminants) = camera_matrices::get(v[0], v[1]).unwrap();
        let index = illuminants.iter().position(|i| *i == code).unwrap();
        let matrix = m[index].unwrap();
        for i in 0..3 {
            for j in 0..3 {
                assert_eq!(matrix.0[i][j], v[3 + 3 * i + j].parse::<f64>().unwrap());
            }
        }
        let mut color = ColorData { color_matrix: [Some(Mat3::IDENTITY), None], ..Default::default() };
        camera_matrices::fill(&mut color, v[0], v[1]);
        assert_eq!(color.color_matrix, [Some(Mat3::IDENTITY), None]);
    }
    assert!(camera_matrices::DATA.windows(2).all(|r| r[0].0 < r[1].0));
    assert!(camera_matrices::get("Unknown", "Camera").is_none());
}
