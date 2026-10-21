use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

pub(super) fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    if !content.starts_with("---\n") {
        bail!("SKILL.md must start with YAML frontmatter");
    }

    let marker = "\n---\n";
    let Some(end) = content["---\n".len()..].find(marker) else {
        bail!("SKILL.md frontmatter must end with ---");
    };
    let end = end + "---\n".len();

    Ok((&content["---\n".len()..end], &content[end + marker.len()..]))
}

pub(super) fn frontmatter_text(entries: &[(String, String)], target: &str) -> Result<String> {
    let entry = entries
        .iter()
        .find_map(|(name, entry)| (name == target).then_some(entry))
        .with_context(|| format!("SKILL.md frontmatter must contain {target}"))?;
    let (_name, first_line) = entry
        .lines()
        .next()
        .and_then(|line| line.split_once(':'))
        .with_context(|| format!("SKILL.md frontmatter {target} is invalid"))?;
    let first_line = first_line.trim();
    let value = if matches!(first_line, ">" | ">-" | "|" | "|-") {
        entry
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        first_line.trim_matches('"').to_owned()
    };
    if value.is_empty() {
        bail!("SKILL.md frontmatter {target} must not be empty");
    }
    Ok(value)
}

pub(super) fn common_frontmatter_entries(path: &Path, frontmatter: &str) -> Result<Vec<String>> {
    let entries = split_entries(frontmatter)?;
    let unknown_keys: Vec<&str> = entries
        .iter()
        .map(|(key, _entry)| key.as_str())
        .filter(|key| !matches!(*key, "name" | "description"))
        .collect();

    if !unknown_keys.is_empty() {
        bail!(
            "{}: non-common frontmatter keys must move to Rust skill overrides: {}",
            path.display(),
            unknown_keys.join(", "),
        );
    }

    Ok(entries.into_iter().map(|(_key, entry)| entry).collect())
}

pub(super) fn split_entries(frontmatter: &str) -> Result<Vec<(String, String)>> {
    let mut entries: Vec<(String, Vec<&str>)> = Vec::new();

    for line in frontmatter.lines() {
        match (frontmatter_key(line), entries.last_mut()) {
            (Some(key), _) => entries.push((key.to_string(), vec![line])),
            (None, Some((_key, lines))) => lines.push(line),
            (None, None) => ensure!(
                line.trim().is_empty(),
                "frontmatter line is not under a key: {line}"
            ),
        }
    }

    Ok(entries
        .into_iter()
        .map(|(key, lines)| (key, lines.join("\n").trim_end().to_string()))
        .collect())
}

fn frontmatter_key(line: &str) -> Option<&str> {
    let (key, _value) = line.split_once(':')?;
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return None;
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_entries_groups_continuation_lines_under_their_key() -> Result<()> {
        let entries = split_entries("\nname: example\ndescription: >\n  first\n\n  second\n")?;

        assert_eq!(
            entries,
            [
                ("name".to_string(), "name: example".to_string()),
                (
                    "description".to_string(),
                    "description: >\n  first\n\n  second".to_string()
                ),
            ]
        );
        assert_eq!(frontmatter_text(&entries, "description")?, "first second");
        Ok(())
    }

    #[test]
    fn split_entries_rejects_a_line_before_the_first_key() {
        let error = split_entries("  orphan\nname: example\n").unwrap_err();

        assert_eq!(
            error.to_string(),
            "frontmatter line is not under a key:   orphan"
        );
    }
}
