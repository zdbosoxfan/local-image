use photocraft_engine::{Session, retouch_cmds};

fn specs() -> Vec<photocraft_engine::commands::CommandSpec> {
    retouch_cmds::specs()
}

#[test]
fn run_functions_return_err_with_empty_params_without_document() {
    let mut session = Session::new();
    let params = Default::default(); // serde_json::Value::Null
    for spec in specs() {
        let result = (spec.run)(&mut session, &params);
        assert!(result.is_err(), "run for `{}` should return Err with no document and empty params", spec.id);
    }
}
