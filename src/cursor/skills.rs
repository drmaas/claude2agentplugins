use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use crate::claude::skill::Skill;
use crate::cursor::markdown::{self, insert_string};
use crate::cursor::placeholders;
use crate::error::Result;
use crate::validate::name;

const PASSTHROUGH: &[&str] = &[
    "license",
    "compatibility",
    "icon",
    "color",
    "metadata",
    "allowed-tools",
    "paths",
    "disable-model-invocation",
];

pub struct WrittenSkill {
    pub name: String,
    pub warnings: Vec<String>,
    pub extras: BTreeMap<String, serde_json::Value>,
}

pub fn write_skills(output_dir: &Path, skills: &[Skill]) -> Result<Vec<WrittenSkill>> {
    let mut written = Vec::new();
    let mut used = HashSet::new();
    for skill in skills {
        written.push(write_one(output_dir, skill, &mut used)?);
    }
    Ok(written)
}

fn write_one(output_dir: &Path, skill: &Skill, used: &mut HashSet<String>) -> Result<WrittenSkill> {
    let mut warnings = Vec::new();
    let mut extras = BTreeMap::new();

    let source_name = skill
        .frontmatter
        .name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| skill.dir_name.clone());
    let mut normalized = name::normalize_skill_name(&source_name);
    if !name::is_valid_skill_name(&normalized) {
        normalized = name::normalize_skill_name(&skill.dir_name);
    }
    normalized = unique_name(normalized, used);
    if normalized != skill.dir_name
        || skill.frontmatter.name.as_deref() != Some(normalized.as_str())
    {
        warnings.push(format!(
            "Skill '{}': written as skills/{normalized}/SKILL.md so the name matches the folder (Cursor requires this)",
            skill.dir_name
        ));
    }

    let description = match skill.frontmatter.description.clone() {
        Some(desc) if !desc.trim().is_empty() => {
            truncate_description(desc, &skill.dir_name, &mut warnings)
        }
        _ => {
            warnings.push(format!(
                "Skill '{}': missing description; synthesized one so Cursor will load the skill",
                skill.dir_name
            ));
            format!("Converted Claude skill '{normalized}'.")
        }
    };

    let mut map = serde_yaml::Mapping::new();
    insert_string(&mut map, "name", normalized.clone());
    insert_string(&mut map, "description", description);

    if let Some(license) = &skill.frontmatter.license {
        insert_string(&mut map, "license", license);
    }
    if let Some(compatibility) = &skill.frontmatter.compatibility {
        insert_string(&mut map, "compatibility", compatibility);
    }
    if let Some(tools) = &skill.frontmatter.allowed_tools {
        insert_string(&mut map, "allowed-tools", tools.to_agent_string());
    }

    if let Some(paths) = skill.frontmatter.extra.get("paths").cloned() {
        map.insert(serde_yaml::Value::String("paths".to_string()), paths);
    } else if let Some(globs) = skill.frontmatter.extra.get("globs").cloned() {
        warnings.push(format!(
            "Skill '{}': frontmatter globs mapped to paths (Cursor's skill field; globs remains a legacy alias)",
            skill.dir_name
        ));
        map.insert(serde_yaml::Value::String("paths".to_string()), globs);
    }

    if let Some(flag) = skill.frontmatter.extra.get("disable-model-invocation") {
        match markdown::yaml_bool(flag) {
            Some(value) => {
                map.insert(
                    serde_yaml::Value::String("disable-model-invocation".to_string()),
                    serde_yaml::Value::Bool(value),
                );
            }
            None => warnings.push(format!(
                "Skill '{}': disable-model-invocation is not a boolean; dropped",
                skill.dir_name
            )),
        }
    }
    for key in ["icon", "color"] {
        if let Some(value) = skill.frontmatter.extra.get(key).and_then(|v| v.as_str()) {
            insert_string(&mut map, key, value);
        }
    }

    if let Some(metadata) = stringify_metadata(skill, &mut warnings) {
        map.insert(serde_yaml::Value::String("metadata".to_string()), metadata);
    }

    for (key, value) in &skill.frontmatter.extra {
        if PASSTHROUGH.contains(&key.as_str()) || key == "globs" {
            continue;
        }
        warnings.push(format!(
            "Skill '{}': frontmatter field '{key}' has no Cursor skill equivalent; preserved in the sidecar",
            skill.dir_name
        ));
        if let Ok(json) = serde_json::to_value(value) {
            extras.insert(key.clone(), json);
        }
    }

    let body_rewrite =
        placeholders::rewrite_placeholders(&skill.body, &format!("Skill '{}'", skill.dir_name));
    warnings.extend(body_rewrite.warnings);

    let skill_dir = output_dir.join("skills").join(&normalized);
    markdown::write(
        &skill_dir.join("SKILL.md"),
        &serde_yaml::Value::Mapping(map),
        &body_rewrite.value,
    )?;
    copy_aux(skill, &skill_dir)?;

    Ok(WrittenSkill {
        name: normalized,
        warnings,
        extras,
    })
}

fn stringify_metadata(skill: &Skill, warnings: &mut Vec<String>) -> Option<serde_yaml::Value> {
    let mut metadata = skill.frontmatter.metadata.clone().unwrap_or_default();
    if metadata.is_empty() {
        return None;
    }
    let mut map = serde_yaml::Mapping::new();
    let mut keys: Vec<_> = metadata.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let value = metadata.remove(&key).unwrap_or(serde_json::Value::Null);
        let text = match value {
            serde_json::Value::String(s) => s,
            other => {
                warnings.push(format!(
                    "Skill '{}': metadata value '{key}' is not a string; converted to '{other}'",
                    skill.dir_name
                ));
                other.to_string()
            }
        };
        map.insert(
            serde_yaml::Value::String(key),
            serde_yaml::Value::String(text),
        );
    }
    Some(serde_yaml::Value::Mapping(map))
}

fn truncate_description(value: String, skill: &str, warnings: &mut Vec<String>) -> String {
    if value.chars().count() <= 1024 {
        return value;
    }
    warnings.push(format!(
        "Skill '{skill}': description exceeds 1024 characters; truncated to satisfy Agent Skills (Cursor skills use that standard)"
    ));
    value.chars().take(1024).collect()
}

fn unique_name(mut name: String, used: &mut HashSet<String>) -> String {
    if used.insert(name.clone()) {
        return name;
    }
    let base = name.clone();
    let mut n = 2;
    loop {
        name = format!("{base}-{n}");
        if used.insert(name.clone()) {
            return name;
        }
        n += 1;
    }
}

fn copy_aux(skill: &Skill, dest_dir: &Path) -> Result<()> {
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

    fn skill(dir: &Path, name: &str, description: Option<&str>) -> Skill {
        Skill {
            dir_name: name.to_string(),
            path: dir.to_path_buf(),
            frontmatter: SkillFrontmatter {
                name: Some(name.to_string()),
                description: description.map(str::to_string),
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                extra: HashMap::new(),
            },
            body: "Do the thing.".to_string(),
            raw: String::new(),
        }
    }

    #[test]
    fn writes_skill_with_matching_name() {
        let temp = tempfile::tempdir().unwrap();
        let written = write_skills(
            temp.path(),
            &[skill(
                Path::new("skills/review"),
                "review",
                Some("Review code."),
            )],
        )
        .unwrap();
        assert_eq!(written[0].name, "review");
        let content = std::fs::read_to_string(temp.path().join("skills/review/SKILL.md")).unwrap();
        assert!(content.contains("name: review"));
        assert!(content.contains("description: Review code."));
        assert!(content.contains("Do the thing."));
    }

    #[test]
    fn renames_to_kebab_case() {
        let temp = tempfile::tempdir().unwrap();
        let mut source = skill(&PathBuf::from("skills/My_Skill"), "My_Skill", Some("Desc"));
        source.dir_name = "My_Skill".to_string();
        let written = write_skills(temp.path(), &[source]).unwrap();
        assert_eq!(written[0].name, "my-skill");
        assert!(temp.path().join("skills/my-skill/SKILL.md").is_file());
        assert!(
            written[0]
                .warnings
                .iter()
                .any(|w| w.contains("matches the folder"))
        );
    }
}
