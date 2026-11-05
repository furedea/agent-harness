#[test]
fn provider_declarations_do_not_depend_on_filesystem_capabilities() {
    for source in [
        include_str!("../src/provider/spec.rs"),
        include_str!("../src/install_plan.rs"),
        include_str!("../src/generation/hooks.rs"),
        include_str!("../src/generation/bridges.rs"),
    ] {
        assert_no_filesystem_imports(source);
    }
}

fn assert_no_filesystem_imports(source: &str) {
    for forbidden in ["fs_ops", "std::fs"] {
        assert!(
            !source.contains(forbidden),
            "Data/Functions module depends on {forbidden}"
        );
    }
}
