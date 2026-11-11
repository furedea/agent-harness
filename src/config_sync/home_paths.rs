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
