/// `rule` of the form `tool(~)` or `tool(~/path)` of a path tool with the home directory in place of `~`; any other rule is
/// returned as it is.
pub(super) fn expand_rule_home_paths(rule: &str, home: &str) -> String {
    match rule.split_once("(~") {
        Some((tool, path)) if is_path_tool(tool) && is_home_relative(path) => {
            format!("{tool}({home}{path}")
        }
        _ => rule.to_owned(),
    }
}

/// The tools whose target is a path; the target of any other, such as `command`, is text.
fn is_path_tool(tool: &str) -> bool {
    matches!(tool, "read_file" | "write_file")
}

/// What follows `~` in `tool(~...)`: nothing but the closing parenthesis, or a path below it.
fn is_home_relative(rest: &str) -> bool {
    rest == ")" || rest.starts_with('/')
}

/// `command` with every `~/` that starts a word outside quotes, or opens a quoted word, replaced
/// by `home/`.
pub(super) fn expand_command_home_paths(command: &str, home: &str) -> String {
    let mut quote_count = 0;
    let mut escaped = false;
    let mut expand_next = true;
    let mut expanded = String::new();
    for (index, character) in command.char_indices() {
        let expand_home = expand_next && command[index..].starts_with("~/");
        push_command_character(&mut expanded, character, expand_home.then_some(home));
        quote_count += usize::from(character == '"' && !escaped);
        expand_next = character == '"' && !escaped && quote_count % 2 == 1;
        escaped = character == '\\' && !escaped;
    }
    expanded
}

fn push_command_character(expanded: &mut String, character: char, home: Option<&str>) {
    if let Some(home) = home {
        expanded.push_str(home);
    } else {
        expanded.push(character);
    }
}
