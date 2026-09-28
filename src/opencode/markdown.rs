use std::path::Path;

use crate::claude::skill::{CommandFile, SkillFrontmatter};
use crate::error::Result;

pub struct MarkdownFile {
    pub frontmatter: Option<SkillFrontmatter>,
    pub body: String,
    pub frontmatter_unparsed: bool,
}

pub fn read_markdown(path: &Path) -> Result<MarkdownFile> {
    let parsed: CommandFile = crate::claude::skill::parse_command_file(path)?;
    let had_markers = parsed.raw.trim_start().starts_with("---");
    let frontmatter_unparsed = had_markers && parsed.frontmatter.is_none();
    Ok(MarkdownFile {
        frontmatter: parsed.frontmatter,
        body: parsed.body,
        frontmatter_unparsed,
    })
}

pub fn fit_name(raw: &str) -> String {
    let mut name = crate::validate::name::normalize_skill_name(raw);
    if name.chars().count() > 64 {
        name = name.chars().take(64).collect();
        while name.ends_with('-') {
            name.pop();
        }
        if name.is_empty() {
            name = "unnamed".to_string();
        }
    }
    name
}

pub fn map_model(model: &str, label: &str, warnings: &mut Vec<String>) -> Option<String> {
    let model = model.trim();
    if model.is_empty() || model.eq_ignore_ascii_case("inherit") {
        return None;
    }
    if model.contains('/') {
        return Some(model.to_string());
    }
    warnings.push(format!(
        "{label}: model '{model}' is a Claude alias, not an OpenCode provider/model id; omitted"
    ));
    None
}

pub fn extra_string(fm: &SkillFrontmatter, keys: &[&str]) -> Option<String> {
    extra_value(fm, keys).and_then(scalar_string)
}

pub fn extra_value<'a>(fm: &'a SkillFrontmatter, keys: &[&str]) -> Option<&'a serde_yaml::Value> {
    keys.iter().find_map(|key| fm.extra.get(*key))
}

pub fn scalar_string(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(text) => Some(text.clone()),
        serde_yaml::Value::Number(number) => Some(number.to_string()),
        serde_yaml::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

pub fn yaml_u64(value: &serde_yaml::Value) -> Option<u64> {
    match value {
        serde_yaml::Value::Number(number) => number.as_u64(),
        serde_yaml::Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

pub fn string_list(value: &serde_yaml::Value) -> Vec<String> {
    match value {
        serde_yaml::Value::String(text) => split_list(text),
        serde_yaml::Value::Sequence(items) => items
            .iter()
            .flat_map(|item| match item {
                serde_yaml::Value::String(text) => split_list(text),
                other => scalar_string(other).into_iter().collect(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn split_list(text: &str) -> Vec<String> {
    if text.contains(',') {
        text.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        text.split_whitespace().map(str::to_string).collect()
    }
}

pub fn unhandled_keys(fm: &SkillFrontmatter, handled: &[&str]) -> Vec<String> {
    let mut keys: Vec<String> = fm
        .extra
        .keys()
        .filter(|key| !handled.contains(&key.as_str()))
        .cloned()
        .collect();
    keys.sort();
    keys
}

pub fn write_markdown(path: &Path, frontmatter_yaml: &str, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let yaml = frontmatter_yaml.trim_end();
    let body = body.trim();
    let content = if yaml.is_empty() {
        format!("{body}\n")
    } else {
        format!("---\n{yaml}\n---\n\n{body}\n")
    };
    std::fs::write(path, content)?;
    Ok(())
}

pub fn truncate_chars(value: &str, max: usize) -> (String, bool) {
    if value.chars().count() <= max {
        (value.to_string(), false)
    } else {
        (value.chars().take(max).collect(), true)
    }
}
