#[test]
fn provider_declarations_do_not_depend_on_filesystem_capabilities() {
    let source = include_str!("../src/provider/spec.rs");
    for forbidden in ["fs_ops", "std::fs"] {
        assert!(!source.contains(forbidden), "spec depends on {forbidden}");
    }
}
