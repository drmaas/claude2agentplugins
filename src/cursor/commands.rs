use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArray};
use crate::claude::skill::{self, CommandFile};
use crate::cursor::markdown::{self, insert_string};
use crate::cursor::placeholders;
use crate::error::Result;
use crate::validate::name;

pub struct WrittenCommand {
    pub name: String,
    pub warnings: Vec<String>,
    pub extras: BTreeMap<String, serde_json::Value>,
    pub as_skill: bool,
}

pub fn convert(
    input: &Path,
    manifest: &ClaudeManifest,
    output: &Path,
    as_skills: bool,
    existing_skills: &HashSet<String>,
) -> Result<Vec<WrittenCommand>> {
    let mut written = Vec::new();
    let mut used = existing_skills.clone();
    for path in command_files(input, manifest) {
        if let Some(command) = write_command(&path, output, as_skills, &mut used)? {
            written.push(command);
        }
    }
    Ok(written)
}

fn command_files(input: &Path, manifest: &ClaudeManifest) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(paths) = &manifest.commands {
        match paths {
            StringOrArray::Single(p) => push_dir(input, p, &mut dirs),
            StringOrArray::Multiple(list) => {
                for p in list {
                    push_dir(input, p, &mut dirs);
                }
            }
        }
    }
    let default = input.join("commands");
    if default.is_dir() && !dirs.iter().any(|d| d == &default) {
        dirs.push(default);
    }
    let mut files = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_file()
                && matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("md" | "mdc" | "markdown" | "txt")
                )
            {
                files.push(path);
            }
        }
    }
    files
}

fn push_dir(input: &Path, raw: &str, dirs: &mut Vec<PathBuf>) {
    let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
    if cleaned.is_empty() {
        return;
    }
    let resolved = input.join(cleaned);
    if resolved.is_dir() {
        dirs.push(resolved);
    }
}

fn write_command(
    path: &Path,
    output: &Path,
    as_skills: bool,
    used: &mut HashSet<String>,
) -> Result<Option<WrittenCommand>> {
    let cmd = skill::parse_command_file(path)?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("command");
    let raw_name = cmd
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.name.clone())
        .unwrap_or_else(|| stem.to_string());
    let normalized = name::normalize_skill_name(&raw_name);
    let mut warnings = Vec::new();
    let mut extras = BTreeMap::new();
    if normalized != raw_name {
        warnings.push(format!(
            "Command '{raw_name}': renamed to '{normalized}' (Cursor command names are lowercase kebab-case)"
        ));
    }
    if !used.insert(normalized.clone()) {
        warnings.push(format!(
            "Command '{normalized}': name already used by a skill or command; skipped"
        ));
        return Ok(Some(WrittenCommand {
            name: normalized,
            warnings,
            extras,
            as_skill: as_skills,
        }));
    }

    let description = skill::command_description(&cmd, &normalized);
    if cmd
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.description.as_ref())
        .is_none()
    {
        warnings.push(format!(
            "Command '{stem}': missing description; synthesized one from the command body"
        ));
    }
    collect_extras(&cmd, stem, &mut warnings, &mut extras);

    let body = placeholders::rewrite_placeholders(&cmd.body, &format!("Command '{stem}'"));
    warnings.extend(body.warnings);

    if as_skills {
        warnings.push(format!(
            "Command '{normalized}': converted to skills/{normalized}/SKILL.md with disable-model-invocation: true (Cursor's explicit slash-command style)"
        ));
        let mut front = serde_yaml::Mapping::new();
        insert_string(&mut front, "name", normalized.clone());
        insert_string(&mut front, "description", description);
        front.insert(
            serde_yaml::Value::String("disable-model-invocation".to_string()),
            serde_yaml::Value::Bool(true),
        );
        markdown::write(
            &output.join("skills").join(&normalized).join("SKILL.md"),
            &serde_yaml::Value::Mapping(front),
            &body.value,
        )?;
    } else {
        let mut front = serde_yaml::Mapping::new();
        insert_string(&mut front, "name", normalized.clone());
        insert_string(&mut front, "description", description);
        markdown::write(
            &output.join("commands").join(format!("{normalized}.md")),
            &serde_yaml::Value::Mapping(front),
            &body.value,
        )?;
    }

    Ok(Some(WrittenCommand {
        name: normalized,
        warnings,
        extras,
        as_skill: as_skills,
    }))
}

fn collect_extras(
    cmd: &CommandFile,
    stem: &str,
    warnings: &mut Vec<String>,
    extras: &mut BTreeMap<String, serde_json::Value>,
) {
    let Some(fm) = &cmd.frontmatter else {
        return;
    };
    if fm.allowed_tools.is_some() {
        warnings.push(format!(
            "Command '{stem}': allowed-tools has no Cursor command field; preserved in the sidecar"
        ));
        if let Some(tools) = &fm.allowed_tools {
            extras.insert(
                "allowed-tools".to_string(),
                serde_json::Value::String(tools.to_agent_string()),
            );
        }
    }
    if let Some(license) = &fm.license {
        extras.insert("license".to_string(), serde_json::json!(license));
    }
    for (key, value) in &fm.extra {
        if key == "name" || key == "description" {
            continue;
        }
        warnings.push(format!(
            "Command '{stem}': frontmatter '{key}' has no Cursor command equivalent; preserved in the sidecar"
        ));
        if let Ok(json) = serde_json::to_value(value) {
            extras.insert(key.clone(), json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_cursor_command() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("commands")).unwrap();
        std::fs::write(
            temp.path().join("commands/deploy.md"),
            "---\ndescription: Deploy the app\nargument-hint: \"[env]\"\n---\n\nRun deploy.\n",
        )
        .unwrap();
        let output = temp.path().join("out");
        let written = convert(
            temp.path(),
            &ClaudeManifest::synthesize("p"),
            &output,
            false,
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(written.len(), 1);
        let content = std::fs::read_to_string(output.join("commands/deploy.md")).unwrap();
        assert!(content.contains("name: deploy"));
        assert!(content.contains("description: Deploy the app"));
        assert!(!content.contains("argument-hint"));
        assert!(
            written[0]
                .warnings
                .iter()
                .any(|w| w.contains("argument-hint"))
        );
    }

    #[test]
    fn convert_commands_flag_writes_skill() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("commands")).unwrap();
        std::fs::write(
            temp.path().join("commands/status.md"),
            "# Status\n\nShow status.\n",
        )
        .unwrap();
        let output = temp.path().join("out");
        let written = convert(
            temp.path(),
            &ClaudeManifest::synthesize("p"),
            &output,
            true,
            &HashSet::new(),
        )
        .unwrap();
        assert!(written[0].as_skill);
        let content = std::fs::read_to_string(output.join("skills/status/SKILL.md")).unwrap();
        assert!(content.contains("disable-model-invocation: true"));
        assert!(!output.join("commands/status.md").exists());
    }
}
