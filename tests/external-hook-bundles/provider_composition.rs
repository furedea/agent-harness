use super::{
    complete_source_root, hook_command_exists, read_json, remove_dir, run_harness, test_root,
};

#[test]
fn external_devin_hooks_are_composed_with_built_in_hooks() {
    let root = test_root("external-devin-hooks");
    let bundle = root.join("moshi");
    let config_path = root.join("config.json");
    std::fs::create_dir_all(bundle.join(".devin/hooks")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(bundle.join(".devin/hooks/moshi-devin.sh"), "#!/bin/bash\n").unwrap();
    let command = format!(
        "bash '{}/.devin/hooks/moshi-devin.sh' notify",
        bundle.display(),
    );
    std::fs::write(
        bundle.join(".devin/hooks.v1.json"),
        format!(
            r#"{{"SessionStart":[{{"hooks":[{{"type":"command","command":"{command}"}}]}}]}}"#,
        ),
    )
    .unwrap();

    run_harness([
        "generate-devin-hooks",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--extra-hook",
        &format!("moshi={}", bundle.display()),
        "--output",
        config_path.to_str().unwrap(),
    ]);

    let config = read_json(&config_path);
    assert!(hook_command_exists(
        &config,
        "bash \"$HOME/.devin/hooks/external/moshi/moshi-devin.sh\" notify",
    ));
    assert!(hook_command_exists(
        &config,
        "$HOME/.devin/hooks/hook_adapter.py shell forbidden",
    ));

    remove_dir(root);
}

#[test]
fn external_hermes_hooks_are_composed_with_built_in_hooks() {
    let root = test_root("external-hermes-hooks");
    let bundle = root.join("moshi");
    let manifest_path = root.join("hooks.json");
    std::fs::create_dir_all(bundle.join(".hermes/hooks")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(
        bundle.join(".hermes/hooks/moshi-hermes.sh"),
        "#!/bin/bash\n",
    )
    .unwrap();
    let command = format!(
        "bash '{}/.hermes/hooks/moshi-hermes.sh' notify",
        bundle.display(),
    );
    std::fs::write(
        bundle.join(".hermes/hooks.json"),
        format!(
            r#"{{"on_session_start":[{{"hooks":[{{"type":"command","command":"{command}"}}]}}]}}"#,
        ),
    )
    .unwrap();

    run_harness([
        "generate-hermes-hooks",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--extra-hook",
        &format!("moshi={}", bundle.display()),
        "--output",
        manifest_path.to_str().unwrap(),
    ]);

    let manifest = read_json(&manifest_path);
    assert!(hook_command_exists(
        &manifest,
        "bash \"$HOME/.hermes/hooks/external/moshi/moshi-hermes.sh\" notify",
    ));
    assert!(hook_command_exists(
        &manifest,
        "$HOME/.hermes/hooks/hook_adapter.py shell forbidden",
    ));

    remove_dir(root);
}

#[test]
fn external_pi_hooks_are_composed_with_built_in_hooks() {
    let root = test_root("external-pi-hooks");
    let bundle = root.join("moshi");
    let manifest_path = root.join("hooks.json");
    std::fs::create_dir_all(bundle.join(".pi/hooks")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(bundle.join(".pi/hooks/moshi-pi.sh"), "#!/bin/bash\n").unwrap();
    let command = format!("bash '{}/.pi/hooks/moshi-pi.sh' notify", bundle.display());
    std::fs::write(
        bundle.join(".pi/hooks.json"),
        format!(
            r#"{{"session_start":[{{"hooks":[{{"type":"command","command":"{command}"}}]}}]}}"#,
        ),
    )
    .unwrap();

    run_harness([
        "generate-pi-hooks",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--extra-hook",
        &format!("moshi={}", bundle.display()),
        "--output",
        manifest_path.to_str().unwrap(),
    ]);

    let manifest = read_json(&manifest_path);
    assert!(hook_command_exists(
        &manifest,
        "bash \"$HOME/.pi/hooks/external/moshi/moshi-pi.sh\" notify",
    ));
    assert!(hook_command_exists(
        &manifest,
        "$HOME/.pi/hooks/hook_adapter.py shell forbidden",
    ));

    remove_dir(root);
}
