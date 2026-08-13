use std::collections::HashSet;
use std::path::Path;

use crate::agent_plugins::skill::AgentSkill;
use crate::claude::manifest::{ClaudeManifest, StringOrArray};
use crate::claude::skill::Skill;
use crate::error::Result;
use crate::validate::name;
use crate::validate::skill::validate_skill;

pub fn convert_commands(
    input_dir: &Path,
    output_dir: &Path,
    manifest: &ClaudeManifest,
) -> Result<(usize, Vec<String>)> {
    let mut warnings = Vec::new();
    let mut count = 0;
    let mut seen: HashSet<String> = HashSet::new();

    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    let default_dir = input_dir.join("commands");
    if default_dir.is_dir() {
        dirs.push(default_dir);
    }
    if let Some(paths) = manifest.commands.as_ref() {
        match paths {
            StringOrArray::Single(p) => add_command_dir(input_dir, p, &mut dirs),
            StringOrArray::Multiple(ps) => {
                for p in ps {
                    add_command_dir(input_dir, p, &mut dirs);
                }
            }
        }
    }

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("command");
            let cmd = crate::claude::skill::parse_command_file(&path)?;
            let raw_name = cmd
                .frontmatter
                .as_ref()
                .and_then(|fm| fm.name.clone())
                .unwrap_or_else(|| stem.to_string());
            let normalized = name::normalize_skill_name(&raw_name);
            if !seen.insert(normalized.clone()) {
                warnings.push(format!(
                    "Command '{}': name '{}' already used by another command; skipped",
                    path.display(),
                    normalized
                ));
                continue;
            }
            let description = crate::claude::skill::command_description(&cmd, &normalized);
            let agent_skill = AgentSkill {
                name: normalized.clone(),
                description,
                license: cmd.frontmatter.as_ref().and_then(|fm| fm.license.clone()),
                compatibility: cmd
                    .frontmatter
                    .as_ref()
                    .and_then(|fm| fm.compatibility.clone()),
                metadata: None,
                allowed_tools: cmd
                    .frontmatter
                    .as_ref()
                    .and_then(|fm| fm.allowed_tools.clone())
                    .map(|tools| tools.to_agent_string()),
            };
            crate::agent_plugins::skill::write_skill(
                &agent_skill,
                &cmd.body,
                output_dir,
                &normalized,
            )?;
            count += 1;
        }
    }

    Ok((count, warnings))
}

fn add_command_dir(input_dir: &Path, raw: &str, dirs: &mut Vec<std::path::PathBuf>) {
    let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
    if cleaned.is_empty() {
        return;
    }
    let resolved = input_dir.join(cleaned);
    if resolved.is_dir() {
        dirs.push(resolved);
    }
}

pub fn convert(
    _input_dir: &Path,
    output_dir: &Path,
    claude_skills: &[Skill],
) -> Result<Vec<crate::convert::ConvertedSkill>> {
    let mut converted = Vec::new();

    for skill in claude_skills {
        let mut warnings = validate_skill(skill);

        let normalized_name = name::normalize_skill_name(&skill.dir_name);
        if normalized_name != skill.dir_name {
            warnings.push(format!(
                "Skill '{}': renamed to '{}' to satisfy Agent Skills naming rules",
                skill.dir_name, normalized_name
            ));
        }

        let mut metadata = skill.frontmatter.metadata.clone().unwrap_or_default();
        for (key, value) in &skill.frontmatter.extra {
            if let Ok(json_val) = serde_json::to_value(value) {
                metadata.insert(key.clone(), json_val);
            }
        }

        let mut metadata_warnings = Vec::new();
        let metadata = if metadata.is_empty() {
            None
        } else {
            let stringified: std::collections::HashMap<String, String> = metadata
                .into_iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s,
                        other => {
                            metadata_warnings.push(format!(
                                "Skill '{}': metadata value '{}' is not a string; converted to '{}'",
                                skill.dir_name, k, other
                            ));
                            other.to_string()
                        }
                    };
                    (k, s)
                })
                .collect();
            Some(stringified)
        };

        let allowed_tools = skill
            .frontmatter
            .allowed_tools
            .as_ref()
            .map(|tools| tools.to_agent_string());

        let description = truncate_with_warning(
            skill.frontmatter.description.clone().unwrap_or_default(),
            1024,
            &format!("Skill '{}': description", skill.dir_name),
            &mut warnings,
        );
        let compatibility = skill.frontmatter.compatibility.clone().map(|c| {
            truncate_with_warning(
                c,
                500,
                &format!("Skill '{}': compatibility", skill.dir_name),
                &mut warnings,
            )
        });

        let agent_skill = AgentSkill {
            name: normalized_name.clone(),
            description,
            license: skill.frontmatter.license.clone(),
            compatibility,
            metadata,
            allowed_tools,
        };

        crate::agent_plugins::skill::write_skill(
            &agent_skill,
            &skill.body,
            output_dir,
            &normalized_name,
        )?;

        copy_skill_aux_files(skill, output_dir, &normalized_name)?;

        warnings.extend(metadata_warnings);

        converted.push(crate::convert::ConvertedSkill {
            path: output_dir.join("skills").join(&normalized_name),
            warnings,
        });
    }

    Ok(converted)
}

fn truncate_with_warning(
    value: String,
    max: usize,
    label: &str,
    warnings: &mut Vec<String>,
) -> String {
    if value.chars().count() <= max {
        return value;
    }
    warnings.push(format!(
        "{} exceeds {} characters; truncated to satisfy Agent Skills constraints",
        label, max
    ));
    value.chars().take(max).collect()
}

fn copy_skill_aux_files(skill: &Skill, output_dir: &Path, normalized_name: &str) -> Result<()> {
    let dest_dir = output_dir.join("skills").join(normalized_name);
    if !skill.path.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&skill.path)? {
        let entry = entry?;
        if entry.file_name() == "SKILL.md" {
            continue;
        }
        let dest = dest_dir.join(entry.file_name());
        crate::convert::extensions::copy_entry(&entry.path(), &dest)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::skill::{Skill, SkillFrontmatter};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_skill(dir_name: &str, fm_name: &str, desc: Option<&str>) -> Skill {
        Skill {
            dir_name: dir_name.to_string(),
            path: PathBuf::from("skills").join(dir_name),
            frontmatter: SkillFrontmatter {
                name: Some(fm_name.to_string()),
                description: desc.map(|s| s.to_string()),
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                extra: HashMap::new(),
            },
            body: "Body content".to_string(),
            raw: String::new(),
        }
    }

    #[test]
    fn convert_single_skill() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        let skill = make_skill("my-skill", "my-skill", Some("A test skill"));
        let result = convert(temp.path(), &output, &[skill]).unwrap();
        assert_eq!(result.len(), 1);
        let skill_md = output.join("skills/my-skill/SKILL.md");
        assert!(skill_md.exists());
        let content = std::fs::read_to_string(skill_md).unwrap();
        assert!(content.contains("name: my-skill"));
        assert!(content.contains("Body content"));
    }

    #[test]
    fn copies_auxiliary_files() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        let skill_dir = temp.path().join("skills").join("pdf");
        std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        std::fs::create_dir_all(skill_dir.join("references")).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: pdf\ndescription: d\n---\nbody",
        )
        .unwrap();
        std::fs::write(skill_dir.join("scripts").join("extract.py"), "print('hi')").unwrap();
        std::fs::write(skill_dir.join("references").join("guide.md"), "# Guide").unwrap();
        std::fs::write(skill_dir.join("template.txt"), "template").unwrap();

        let skill = crate::claude::skill::parse_skill_dir(&skill_dir).unwrap();
        let result = convert(temp.path(), &output, &[skill]).unwrap();
        assert_eq!(result.len(), 1);

        assert!(output.join("skills/pdf/scripts/extract.py").exists());
        assert!(output.join("skills/pdf/references/guide.md").exists());
        assert!(output.join("skills/pdf/template.txt").exists());
        assert_eq!(
            std::fs::read_to_string(output.join("skills/pdf/scripts/extract.py")).unwrap(),
            "print('hi')"
        );
    }

    #[test]
    fn normalizes_skill_name() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        let skill = make_skill("My_Skill", "My_Skill", Some("desc"));
        let result = convert(temp.path(), &output, &[skill]).unwrap();
        assert!(output.join("skills/my-skill/SKILL.md").exists());
        assert!(result[0].warnings.iter().any(|w| w.contains("renamed")));
    }

    #[test]
    fn joins_allowed_tools_as_string() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        let mut skill = make_skill("tools-skill", "tools-skill", Some("desc"));
        skill.frontmatter.allowed_tools = Some(crate::claude::skill::AllowedTools::Multiple(vec![
            "bash".to_string(),
            "read".to_string(),
        ]));
        convert(temp.path(), &output, &[skill]).unwrap();
        let content = std::fs::read_to_string(output.join("skills/tools-skill/SKILL.md")).unwrap();
        assert!(content.contains("allowed-tools: bash read"));
    }

    #[test]
    fn stringifies_metadata_values() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        let mut skill = make_skill("meta-skill", "meta-skill", Some("desc"));
        let mut metadata = HashMap::new();
        metadata.insert("version".to_string(), serde_json::json!(3));
        metadata.insert("author".to_string(), serde_json::json!("org"));
        skill.frontmatter.metadata = Some(metadata);

        let result = convert(temp.path(), &output, &[skill]).unwrap();
        assert!(
            result[0]
                .warnings
                .iter()
                .any(|w| w.contains("not a string"))
        );
        let content = std::fs::read_to_string(output.join("skills/meta-skill/SKILL.md")).unwrap();
        assert!(content.contains("version: \"3\"") || content.contains("version: '3'"));
        assert!(content.contains("author: org"));
    }

    #[test]
    fn converts_commands_to_skills() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        let commands_dir = temp.path().join("commands");
        std::fs::create_dir_all(&commands_dir).unwrap();
        std::fs::write(
            commands_dir.join("deploy.md"),
            "---\nname: deploy\ndescription: Deploy it\n---\n\nRun deploy.",
        )
        .unwrap();
        std::fs::write(commands_dir.join("status.md"), "# Status\n\nShow status.").unwrap();
        std::fs::write(commands_dir.join("notes.txt"), "not a command").unwrap();

        let manifest = ClaudeManifest::synthesize("test");
        let (count, warnings) = convert_commands(temp.path(), &output, &manifest).unwrap();
        assert_eq!(count, 2);
        assert!(warnings.is_empty());

        let deploy = std::fs::read_to_string(output.join("skills/deploy/SKILL.md")).unwrap();
        assert!(deploy.contains("name: deploy"));
        assert!(deploy.contains("description: Deploy it"));
        let status = std::fs::read_to_string(output.join("skills/status/SKILL.md")).unwrap();
        assert!(status.contains("name: status"));
        assert!(status.contains("Show status."));
    }

    #[test]
    fn convert_commands_uses_custom_paths() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        std::fs::create_dir_all(temp.path().join("custom")).unwrap();
        std::fs::write(
            temp.path().join("custom").join("check.md"),
            "---\ndescription: Check it\n---\n\nCheck.",
        )
        .unwrap();

        let mut manifest = ClaudeManifest::synthesize("test");
        manifest.commands = Some(StringOrArray::Single("./custom".to_string()));
        let (count, _) = convert_commands(temp.path(), &output, &manifest).unwrap();
        assert_eq!(count, 1);
        assert!(output.join("skills/check/SKILL.md").exists());
    }

    #[test]
    fn truncates_overlong_description() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        let long_desc = "x".repeat(1500);
        let skill = make_skill("long-desc", "long-desc", Some(&long_desc));
        let result = convert(temp.path(), &output, &[skill]).unwrap();
        assert!(result[0].warnings.iter().any(|w| w.contains("truncated")));
        let content = std::fs::read_to_string(output.join("skills/long-desc/SKILL.md")).unwrap();
        assert!(content.contains(&"x".repeat(1024)));
        assert!(!content.contains(&"x".repeat(1025)));
    }

    #[test]
    fn handles_missing_description() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out2");

        let skill = make_skill("no-desc", "no-desc", None);
        let result = convert(temp.path(), &output, &[skill]).unwrap();
        assert!(!result[0].warnings.is_empty());
    }
}
