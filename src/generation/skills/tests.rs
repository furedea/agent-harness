use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[test]
fn render_claude_skill_applies_frontmatter_overrides() -> Result<()> {
    let root = test_root("render_claude_skill_applies_frontmatter_overrides")?;
    write_skill_source(&root, "custom-command")?;
    write_skill_rendering(
        &root,
        r#"{
  "version": 1,
  "skills": {
    "custom-command": {
      "claude": {
        "frontmatter": {
          "argument-hint": "{direct | pr-per-feature}",
          "disable-model-invocation": true
        }
      }
    }
  }
}
"#,
    )?;
    let out = root.join("out");

    render_skills(
        &root,
        &[],
        SkillTarget {
            provider: Provider::Claude,
            path: &out,
        },
    )?;

    let content = std::fs::read_to_string(out.join("custom-command/SKILL.md"))?;
    assert!(content.contains("argument-hint: \"{direct | pr-per-feature}\""));
    assert!(content.contains("disable-model-invocation: true"));
    assert!(!out.join("custom-command/agents/openai.yaml").exists());

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn render_codex_skill_writes_explicit_only_metadata() -> Result<()> {
    let root = test_root("render_codex_skill_writes_explicit_only_metadata")?;
    write_skill_source(&root, "custom-command")?;
    write_skill_rendering(
        &root,
        r#"{
  "version": 1,
  "skills": {
    "custom-command": {
      "codex": {
        "openai": {
          "allow_implicit_invocation": false
        }
      }
    }
  }
}
"#,
    )?;
    let out = root.join("out");

    render_skills(
        &root,
        &[],
        SkillTarget {
            provider: Provider::Codex,
            path: &out,
        },
    )?;

    let content = std::fs::read_to_string(out.join("custom-command/SKILL.md"))?;
    assert!(!content.contains("disable-model-invocation"));
    assert_eq!(
        std::fs::read_to_string(out.join("custom-command/agents/openai.yaml"))?,
        "policy:\n  allow_implicit_invocation: false\n",
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn render_skills_copies_an_external_skill_directory_verbatim() -> Result<()> {
    let root = test_root("render_skills_copies_an_external_skill_directory_verbatim")?;
    write_skill_source(&root, "built-in")?;
    let external_skill_dir = root.join("external/herdr");
    let external_skill_content =
        "---\nname: herdr\ndescription: upstream skill\nlicense: Apache-2.0\n---\n\nBody\n";
    write_file(&external_skill_dir.join("SKILL.md"), external_skill_content)?;
    write_file(
        &external_skill_dir.join("references/commands.md"),
        "Upstream commands\n",
    )?;
    let external_skills = [ExternalSkill::new("herdr", external_skill_dir)?];
    let out = root.join("out");

    render_skills(
        &root,
        &external_skills,
        SkillTarget {
            provider: Provider::Codex,
            path: &out,
        },
    )?;

    assert_eq!(
        std::fs::read_to_string(out.join("herdr/SKILL.md"))?,
        external_skill_content,
    );
    assert_eq!(
        std::fs::read_to_string(out.join("herdr/references/commands.md"))?,
        "Upstream commands\n",
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn external_skill_rejects_a_name_that_escapes_the_output_directory() {
    let result = ExternalSkill::new("../herdr", PathBuf::from("/external/herdr"));

    assert!(result.is_err());
}

#[test]
fn render_skills_rejects_an_external_skill_that_shadows_a_built_in_skill() -> Result<()> {
    let root = test_root("render_skills_rejects_an_external_skill_that_shadows_a_built_in_skill")?;
    write_skill_source(&root, "herdr")?;
    let external_skill_dir = root.join("external/herdr");
    write_file(
        &external_skill_dir.join("SKILL.md"),
        "---\nname: herdr\ndescription: upstream skill\n---\n\nBody\n",
    )?;
    let external_skills = [ExternalSkill::new("herdr", external_skill_dir)?];

    let result = render_skills(
        &root,
        &external_skills,
        SkillTarget {
            provider: Provider::Codex,
            path: &root.join("out"),
        },
    );

    assert!(result.is_err());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn render_skills_rejects_duplicate_external_skill_names() -> Result<()> {
    let root = test_root("render_skills_rejects_duplicate_external_skill_names")?;
    write_skill_source(&root, "built-in")?;
    let first_source = root.join("external/first");
    let second_source = root.join("external/second");
    write_file(
        &first_source.join("SKILL.md"),
        "---\nname: shared\ndescription: first\n---\n\nBody\n",
    )?;
    write_file(
        &second_source.join("SKILL.md"),
        "---\nname: shared\ndescription: second\n---\n\nBody\n",
    )?;
    let external_skills = [
        ExternalSkill::new("shared", first_source)?,
        ExternalSkill::new("shared", second_source)?,
    ];

    let result = render_skills(
        &root,
        &external_skills,
        SkillTarget {
            provider: Provider::Codex,
            path: &root.join("out"),
        },
    );

    assert!(result.is_err());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn render_skills_rejects_an_external_directory_without_a_skill_file() -> Result<()> {
    let root = test_root("render_skills_rejects_an_external_directory_without_a_skill_file")?;
    write_skill_source(&root, "built-in")?;
    let external_skill_dir = root.join("external/incomplete");
    std::fs::create_dir_all(&external_skill_dir)?;
    let external_skills = [ExternalSkill::new("incomplete", external_skill_dir)?];

    let result = render_skills(
        &root,
        &external_skills,
        SkillTarget {
            provider: Provider::Codex,
            path: &root.join("out"),
        },
    );

    assert!(result.is_err());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn render_skills_preserves_installed_skills_when_a_source_is_invalid() -> Result<()> {
    let root = test_root("invalid-source-preserves-skills")?;
    write_file(
        &root.join("skills/broken/SKILL.md"),
        "missing frontmatter\n",
    )?;
    let out = root.join("out");
    write_file(&out.join("existing/SKILL.md"), "installed skill\n")?;

    let error = render_skills(
        &root,
        &[],
        SkillTarget {
            provider: Provider::Codex,
            path: &out,
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("frontmatter"));
    assert_eq!(
        std::fs::read_to_string(out.join("existing/SKILL.md"))?,
        "installed skill\n",
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn render_skills_rejects_an_external_skill_with_only_a_symlinked_skill_file() -> Result<()> {
    let root = test_root("external-symlinked-skill")?;
    let external = root.join("external");
    let real_skill = root.join("upstream.md");
    write_file(&real_skill, "upstream skill\n")?;
    std::fs::create_dir(&external)?;
    std::os::unix::fs::symlink(real_skill, external.join("SKILL.md"))?;
    let out = root.join("out");
    write_file(&out.join("existing/SKILL.md"), "installed skill\n")?;
    let skills = [ExternalSkill::new("example", external)?];

    let error = render_skills(
        &root,
        &skills,
        SkillTarget {
            provider: Provider::Codex,
            path: &out,
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("SKILL.md"));
    assert_eq!(
        std::fs::read_to_string(out.join("existing/SKILL.md"))?,
        "installed skill\n",
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn test_root(name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn write_skill_source(source: &Path, skill_name: &str) -> Result<()> {
    let skill_dir = source.join("skills").join(skill_name);
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: example\ndescription: test skill\n---\n\nBody\n",
    )?;
    Ok(())
}

fn write_skill_rendering(source: &Path, content: &str) -> Result<()> {
    std::fs::create_dir_all(source)?;
    std::fs::write(source.join("skill_rendering.json"), content)?;
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
    std::fs::write(path, content)?;
    Ok(())
}
