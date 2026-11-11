use std::path::Path;

use serde_json::Value;

pub(super) fn expand_excluded_command_home_paths(settings: &mut Value, home: &Path) {
    let Some(commands) = settings
        .get_mut("sandbox")
        .and_then(|sandbox| sandbox.get_mut("excludedCommands"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    let home = home.to_string_lossy();
    for command in commands {
        expand_command_home_path(command, &home);
    }
}

fn expand_command_home_path(command: &mut Value, home: &str) {
    let Value::String(command) = command else {
        return;
    };
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
    *command = expanded;
}

fn push_command_character(expanded: &mut String, character: char, home: Option<&str>) {
    if let Some(home) = home {
        expanded.push_str(home);
    } else {
        expanded.push(character);
    }
}
