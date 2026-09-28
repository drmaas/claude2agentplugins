use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::claude::skill::Skill;
use crate::error::Result;
use crate::opencode::markdown::{fit_name, truncate_chars, write_markdown};

#[derive(Debug, Clone)]
pub struct RegisteredSkill {
    pub id: String,
    pub description: String,
    pub content: String,
    pub relative_dir: String,
}

#[derive(Serialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compatibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<BTreeMap<String, String>>,
}

pub fn convert(
    plugin_root: &Path,
    output: &Path,
    skills: &[Skill],
) -> Result<(Vec<RegisteredSkill>, Vec<String>)> {
    let mut warnings = Vec::new();
    let mut registered = Vec::new();
    let mut seen = HashSet::new();

    for skill in skills {
        let id = fit_name(&skill.dir_name);
        if id != skill.dir_name {
            warnings.push(format!(
                "Skill '{}': renamed to '{id}' to satisfy OpenCode v2 skill naming",
                skill.dir_name
            ));
        }
        if let Some(frontmatter_name) = &skill.frontmatter.name
            && fit_name(frontmatter_name) != id
        {
            warnings.push(format!(
                "Skill '{}': frontmatter name '{frontmatter_name}' does not match directory name '{id}'; v2 skill id uses the directory name",
                skill.dir_name
            ));
        }
        if !seen.insert(id.clone()) {
            warnings.push(format!(
                "Skill '{}': duplicate skill id '{id}'; skipped",
                skill.dir_name
            ));
            continue;
        }

        let mut description = skill
            .frontmatter
            .description
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string();
        if description.is_empty() {
            description = format!("Converted Claude skill {id}");
            warnings.push(format!(
                "Skill '{id}': missing description; synthesized '{description}'"
            ));
        }
        let (description, truncated) = truncate_chars(&description, 1024);
        if truncated {
            warnings.push(format!(
                "Skill '{id}': description exceeds 1024 characters; truncated"
            ));
        }

        if skill.frontmatter.allowed_tools.is_some() {
            warnings.push(format!(
                "Skill '{id}': allowed-tools is not an OpenCode v2 skill field; omitted from the plugin registration"
            ));
        }
        let mut extra_keys: Vec<&String> = skill.frontmatter.extra.keys().collect();
        extra_keys.sort();
        for key in extra_keys {
            warnings.push(format!(
                "Skill '{id}': frontmatter field '{key}' is not an OpenCode v2 skill field; omitted from the plugin registration"
            ));
        }

        let metadata = stringify_metadata(skill, &id, &mut warnings);
        let frontmatter = SkillFrontmatter {
            name: id.clone(),
            description: description.clone(),
            license: skill.frontmatter.license.clone(),
            compatibility: skill.frontmatter.compatibility.clone(),
            metadata,
        };
        let yaml = serde_yaml::to_string(&frontmatter)?;
        let relative_dir = format!("skills/{id}");
        let skill_dir = output.join("skills").join(&id);
        std::fs::create_dir_all(&skill_dir)?;
        write_markdown(&skill_dir.join("SKILL.md"), &yaml, &skill.body)?;
        copy_aux(skill, plugin_root, &skill_dir)?;

        registered.push(RegisteredSkill {
            id,
            description,
            content: skill.body.trim().to_string(),
            relative_dir,
        });
    }

    Ok((registered, warnings))
}

fn stringify_metadata(
    skill: &Skill,
    id: &str,
    warnings: &mut Vec<String>,
) -> Option<BTreeMap<String, String>> {
    let mut metadata = BTreeMap::new();
    if let Some(values) = &skill.frontmatter.metadata {
        for (key, value) in values {
            let text = match value {
                serde_json::Value::String(text) => text.clone(),
                other => {
                    warnings.push(format!(
                        "Skill '{id}': metadata value '{key}' is not a string; converted to '{other}'"
                    ));
                    other.to_string()
                }
            };
            metadata.insert(key.clone(), text);
        }
    }
    if metadata.is_empty() {
        None
    } else {
        warnings.push(format!(
            "Skill '{id}': metadata is preserved in SKILL.md; the v2 editor.add registration passes name, description, and content"
        ));
        Some(metadata)
    }
}

fn copy_aux(skill: &Skill, plugin_root: &Path, dest_dir: &Path) -> Result<()> {
    if !skill.path.is_dir() {
        return Ok(());
    }
    let is_root = skill.path == plugin_root;
    for entry in std::fs::read_dir(&skill.path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "SKILL.md" {
            continue;
        }
        if is_root {
            const ALLOW: &[&str] = &["scripts", "references", "assets"];
            if !ALLOW.contains(&name_str.as_ref()) {
                continue;
            }
        }
        crate::convert::extensions::copy_entry(&entry.path(), &dest_dir.join(name))?;
    }
    Ok(())
}
