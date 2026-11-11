use super::*;

fn json(text: &str) -> Json {
    Json::parse(text, Role::Installed).unwrap()
}

fn yaml(text: &str) -> Yaml {
    Yaml::parse(text, Role::Installed).unwrap()
}

fn toml(text: &str) -> Toml {
    Toml::parse(text, Role::Installed).unwrap()
}

#[test]
fn json_requires_an_object_at_the_root() {
    for text in ["[]", "null", "1", "not json"] {
        assert!(Json::parse(text, Role::Installed).is_err(), "{text}");
    }
}

#[test]
fn json_sets_and_gets_a_nested_value_creating_the_mappings_on_the_way() {
    let mut tree = json(r#"{"keep": 1}"#);

    tree.set(&["a", "b"], serde_json::json!(2)).unwrap();

    assert_eq!(tree.get(&["a", "b"]), Some(serde_json::json!(2)));
    assert_eq!(tree.get(&["keep"]), Some(serde_json::json!(1)));
    assert_eq!(tree.get(&["a", "missing"]), None);
}

#[test]
fn json_set_refuses_to_step_through_a_value_that_is_not_a_mapping() {
    let mut tree = json(r#"{"a": "text"}"#);

    assert!(tree.set(&["a", "b"], serde_json::json!(1)).is_err());
}

#[test]
fn json_remove_ignores_a_missing_path() {
    let mut tree = json(r#"{"a": {"b": 1, "c": 2}}"#);

    tree.remove(&["a", "b"]);
    tree.remove(&["x", "y"]);
    tree.remove(&["a", "b", "deeper"]);

    assert_eq!(
        tree.render().unwrap(),
        "{\n  \"a\": {\n    \"c\": 2\n  }\n}\n"
    );
}

#[test]
fn json_renders_pretty_with_a_trailing_newline() {
    assert_eq!(json("{}").render().unwrap(), "{}\n");
}

#[test]
fn yaml_reads_an_empty_installed_file_as_an_empty_mapping_but_not_a_generated_one() {
    assert!(Yaml::parse("", Role::Installed).is_ok());
    assert!(Yaml::parse("", Role::Generated).is_err());
    assert!(Yaml::parse("- a\n", Role::Installed).is_err());
}

#[test]
fn yaml_sets_gets_and_removes_nested_values_in_order() {
    let mut tree = yaml("first: 1\nplugins:\n  enabled:\n    - a\n");

    tree.set(&["plugins", "extra"], YamlValue::Bool(true))
        .unwrap();
    tree.set(&["last"], YamlValue::Bool(false)).unwrap();
    tree.remove(&["plugins", "enabled"]);

    assert_eq!(tree.get(&["plugins", "extra"]), Some(YamlValue::Bool(true)));
    assert_eq!(tree.get(&["plugins", "enabled"]), None);
    let keys: Vec<_> = tree.entries().into_iter().map(|(key, _)| key).collect();
    assert_eq!(keys, ["first", "plugins", "last"]);
}

#[test]
fn toml_keeps_comments_and_layout_it_does_not_touch() {
    let mut tree = toml("# note\nkeep   = 1 # trailing\n");

    tree.set(&["added"], toml_edit::value(2)).unwrap();

    let text = tree.render().unwrap();
    assert!(text.starts_with("# note\nkeep   = 1 # trailing\n"));
    assert!(text.contains("added = 2"));
}

#[test]
fn toml_steps_through_inline_tables_and_refuses_other_values() {
    let mut inline = toml("policy = { set = { CI = \"1\" } }\n");
    inline
        .set(&["policy", "set", "NEW"], toml_edit::value("x"))
        .unwrap();
    assert!(inline.get(&["policy", "set", "CI"]).is_some());
    assert!(inline.get(&["policy", "set", "NEW"]).is_some());

    let mut scalar = toml("policy = \"inherit\"\n");
    assert!(scalar.set(&["policy", "set"], toml_edit::value(1)).is_err());
}

#[test]
fn toml_remove_ignores_a_path_that_is_not_there() {
    let mut tree = toml("[a]\nb = 1\nc = 2\n");

    tree.remove(&["a", "b"]);
    tree.remove(&["x", "y"]);

    let text = tree.render().unwrap();
    assert!(!text.contains("b = 1"));
    assert!(text.contains("c = 2"));
}

#[test]
fn a_document_remembers_its_format() {
    for format in [ConfigFormat::Json, ConfigFormat::Toml, ConfigFormat::Yaml] {
        assert_eq!(ConfigDocument::blank(format).format(), format);
    }
}
