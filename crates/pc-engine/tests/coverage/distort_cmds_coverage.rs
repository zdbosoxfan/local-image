use photocraft_engine::distort_cmds;
use serde_json::json;

// puppet_params

#[test]
fn puppet_params_pins_malformed_err() {
    let p = json!({"pins": "not an array"});
    assert!(distort_cmds::puppet_params("test", &p).is_err());
}

#[test]
fn puppet_params_expansion_too_large_err() {
    let p = json!({"expansion": 201.0});
    assert!(distort_cmds::puppet_params("test", &p).is_err());
}

#[test]
fn puppet_params_expansion_boundary_positive_ok() {
    let p = json!({"expansion": 200.0});
    assert!(distort_cmds::puppet_params("test", &p).is_ok());
}

// perspective_params

#[test]
fn perspective_params_empty_planes_err() {
    let p = json!({"planes": []});
    assert!(distort_cmds::perspective_params("test", &p).is_err());
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
