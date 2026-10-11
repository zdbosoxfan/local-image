#[derive(Debug,PartialEq)] enum Module {Library,Develop,Compositing}
fn module_preference(value: Option<&str>) -> Module {
    match value.map(str::trim) {
        Some("library" | "develop") => Module::Library,
        _ => Module::Compositing,
    }
}
#[test] fn bugfix_develop_startup_module() {
assert_eq!(module_preference(None),Module::Library);
assert_eq!(module_preference(Some("develop")),Module::Develop);
assert_eq!(module_preference(Some("editor")),Module::Compositing);
}
