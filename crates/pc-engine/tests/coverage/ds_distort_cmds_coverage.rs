use photocraft_engine::distort_cmds;
use serde_json::json;

// ---------------------------------------------------------------------------
// command registry
// ---------------------------------------------------------------------------

#[test]
fn specs_has_five_commands() {
    let specs = distort_cmds::specs();
    assert_eq!(specs.len(), 5);
}

#[test]
fn specs_ids_match_constants() {
    let specs = distort_cmds::specs();
    let ids: Vec<&str> = specs.iter().map(|s| s.id).collect();
    assert!(ids.contains(&distort_cmds::LIQUIFY));
    assert!(ids.contains(&distort_cmds::PUPPET));
    assert!(ids.contains(&distort_cmds::PUPPET_SMART));
    assert!(ids.contains(&distort_cmds::PERSPECTIVE));
    assert!(ids.contains(&distort_cmds::PERSPECTIVE_SMART));
}

// ---------------------------------------------------------------------------
// puppet_params
// ---------------------------------------------------------------------------

#[test]
fn puppet_params_empty_pins_ok() {
    let p = json!({});
    assert!(distort_cmds::puppet_params("test", &p).is_ok());
}

#[test]
fn puppet_params_single_pin_ok() {
    let p = json!({
        "pins": [
            {"src": [10.0, 20.0], "dst": [15.0, 25.0]}
        ]
    });
    assert!(distort_cmds::puppet_params("test", &p).is_ok());
}

#[test]
fn puppet_params_pins_malformed_err() {
    let p = json!({"pins": "not an array"});
    assert!(distort_cmds::puppet_params("test", &p).is_err());
}

#[test]
fn puppet_params_invalid_mode_err() {
    let p = json!({"mode": "stretchy"});
    assert!(distort_cmds::puppet_params("test", &p).is_err());
}

#[test]
fn puppet_params_invalid_density_err() {
    let p = json!({"density": "ultra"});
    assert!(distort_cmds::puppet_params("test", &p).is_err());
}

#[test]
fn puppet_params_expansion_too_large_err() {
    let p = json!({"expansion": 201.0});
    assert!(distort_cmds::puppet_params("test", &p).is_err());
}

#[test]
fn puppet_params_expansion_negative_too_large_err() {
    let p = json!({"expansion": -201.0});
    assert!(distort_cmds::puppet_params("test", &p).is_err());
}

#[test]
fn puppet_params_expansion_boundary_positive_ok() {
    let p = json!({"expansion": 200.0});
    assert!(distort_cmds::puppet_params("test", &p).is_ok());
}

#[test]
fn puppet_params_expansion_boundary_negative_ok() {
    let p = json!({"expansion": -200.0});
    assert!(distort_cmds::puppet_params("test", &p).is_ok());
}

// ---------------------------------------------------------------------------
// perspective_params
// ---------------------------------------------------------------------------

#[test]
fn perspective_params_missing_planes_err() {
    let p = json!({});
    assert!(distort_cmds::perspective_params("test", &p).is_err());
}

#[test]
fn perspective_params_empty_planes_err() {
    let p = json!({"planes": []});
    assert!(distort_cmds::perspective_params("test", &p).is_err());
}

#[test]
fn perspective_params_valid_single_plane_ok() {
    let p = json!({
        "planes": [
            {
                "src": [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
                "dst": [[0.0, 0.0], [110.0, 0.0], [110.0, 110.0], [0.0, 110.0]]
            }
        ]
    });
    let result = distort_cmds::perspective_params("test", &p);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn perspective_params_plane_wrong_arity_err() {
    let p = json!({
        "planes": [
            {
                "src": [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]],
                "dst": [[0.0, 0.0], [110.0, 0.0], [110.0, 110.0]]
            }
        ]
    });
    assert!(distort_cmds::perspective_params("test", &p).is_err());
}

#[test]
fn perspective_params_unknown_straighten_err() {
    let p = json!({
        "planes": [
            {
                "src": [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
                "dst": [[0.0, 0.0], [110.0, 0.0], [110.0, 110.0], [0.0, 110.0]]
            }
        ],
        "straighten": "diagonal"
    });
    assert!(distort_cmds::perspective_params("test", &p).is_err());
}

#[test]
fn perspective_params_straighten_horizontal_ok() {
    let p = json!({
        "planes": [
            {
                "src": [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
                "dst": [[0.0, 0.0], [110.0, 0.0], [110.0, 110.0], [0.0, 110.0]]
            }
        ],
        "straighten": "horizontal"
    });
    assert!(distort_cmds::perspective_params("test", &p).is_ok());
}

#[test]
fn perspective_params_degenerate_plane_err() {
    // All four corners collinear -> degenerate quad
    let p = json!({
        "planes": [
            {
                "src": [[0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]],
                "dst": [[0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]]
            }
        ]
    });
    assert!(distort_cmds::perspective_params("test", &p).is_err());
}

#[test]
fn perspective_params_multiple_planes_ok() {
    let p = json!({
        "planes": [
            {
                "src": [[0.0, 0.0], [50.0, 0.0], [50.0, 50.0], [0.0, 50.0]],
                "dst": [[0.0, 0.0], [55.0, 0.0], [55.0, 55.0], [0.0, 55.0]]
            },
            {
                "src": [[60.0, 0.0], [100.0, 0.0], [100.0, 40.0], [60.0, 40.0]],
                "dst": [[60.0, 0.0], [105.0, 0.0], [105.0, 45.0], [60.0, 45.0]]
            }
        ]
    });
    let result = distort_cmds::perspective_params("test", &p);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 2);
}
