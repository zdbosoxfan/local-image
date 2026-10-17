use photocraft_io::abr_map::read_abr;

#[test]
fn empty_input_returns_err() {
    let result = read_abr(b"", "test.abr");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for empty input"),
    }
}
