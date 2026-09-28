use std::path::Path;

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct MarkdownDoc {
    pub frontmatter: Option<serde_yaml::Value>,
    pub body: String,
}

pub fn parse(raw: &str) -> MarkdownDoc {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return MarkdownDoc {
            frontmatter: None,
            body: raw.trim().to_string(),
        };
    }
    let rest = &trimmed[3..];
    let Some(end) = rest.find("---") else {
        return MarkdownDoc {
            frontmatter: None,
            body: raw.trim().to_string(),
        };
    };
    let yaml = &rest[..end];
    let body = rest[end + 3..].trim().to_string();
    match serde_yaml::from_str::<serde_yaml::Value>(yaml) {
        Ok(value) => MarkdownDoc {
            frontmatter: Some(value),
            body,
        },
        Err(_) => MarkdownDoc {
            frontmatter: None,
            body: raw.trim().to_string(),
        },
    }
}

pub fn write(path: &Path, frontmatter: &serde_yaml::Value, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let yaml = serde_yaml::to_string(frontmatter).map_err(Error::Yaml)?;
    let content = format!("---\n{yaml}---\n\n{body}\n");
    std::fs::write(path, content)?;
    Ok(())
}

pub fn mapping_get<'a>(value: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Value> {
    value
        .as_mapping()?
        .get(serde_yaml::Value::String(key.to_string()))
}

pub fn mapping_string(value: &serde_yaml::Value, key: &str) -> Option<String> {
    match mapping_get(value, key)? {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub fn yaml_bool(value: &serde_yaml::Value) -> Option<bool> {
    match value {
        serde_yaml::Value::Bool(b) => Some(*b),
        serde_yaml::Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        },
        serde_yaml::Value::Number(n) => n.as_i64().and_then(|n| match n {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        }),
        _ => None,
    }
}

pub fn insert_string(map: &mut serde_yaml::Mapping, key: &str, value: impl Into<String>) {
    map.insert(
        serde_yaml::Value::String(key.to_string()),
        serde_yaml::Value::String(value.into()),
    );
}

pub fn first_prose_line(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.chars().take(1024).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter() {
        let doc = parse("---\nname: demo\ndescription: Hi\n---\n\nBody.");
        assert_eq!(
            mapping_string(doc.frontmatter.as_ref().unwrap(), "name").as_deref(),
            Some("demo")
        );
        assert_eq!(doc.body, "Body.");
    }

    #[test]
    fn parses_plain_markdown() {
        let doc = parse("# Title\n\nHello.");
        assert!(doc.frontmatter.is_none());
        assert!(doc.body.contains("Hello."));
    }

    #[test]
    fn yaml_bool_accepts_claude_spellings() {
        assert_eq!(
            yaml_bool(&serde_yaml::Value::String("yes".into())),
            Some(true)
        );
        assert_eq!(
            yaml_bool(&serde_yaml::Value::String("off".into())),
            Some(false)
        );
    }
}
