use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillFrontmatter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "allowed-tools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub dir_name: String,
    pub path: PathBuf,
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    pub raw: String,
}

fn parse_frontmatter(content: &str) -> Result<(SkillFrontmatter, String)> {
    let content = content.trim();

    if !content.starts_with("---") {
        return Err(Error::Conversion(
            "SKILL.md does not start with YAML frontmatter".to_string(),
        ));
    }

    let rest = &content[3..];
    let end_idx = rest
        .find("---")
        .ok_or_else(|| Error::Conversion("SKILL.md has unclosed YAML frontmatter".to_string()))?;

    let frontmatter_yaml = &rest[..end_idx];
    let body = rest[end_idx + 3..].trim().to_string();

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(frontmatter_yaml)
        .map_err(|e| Error::Conversion(format!("Failed to parse SKILL.md frontmatter: {e}")))?;

    Ok((frontmatter, body))
}

pub fn parse_skill_dir(skill_dir: &Path) -> Result<Skill> {
    let skill_md = skill_dir.join("SKILL.md");
    if !skill_md.exists() {
        return Err(Error::NotFound {
            path: skill_md.display().to_string(),
            reason: "No SKILL.md found in skill directory".to_string(),
        });
    }
    let raw = std::fs::read_to_string(&skill_md)?;
    let (frontmatter, body) = parse_frontmatter(&raw)?;

    let dir_name = skill_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(Skill {
        dir_name,
        path: skill_dir.to_path_buf(),
        frontmatter,
        body,
        raw,
    })
}

pub fn parse_all(plugin_dir: &Path) -> Result<Vec<Skill>> {
    let skills_dir = plugin_dir.join("skills");
    if !skills_dir.exists() {
        return Ok(vec![]);
    }

    let mut skills = Vec::new();
    for entry in std::fs::read_dir(&skills_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            match parse_skill_dir(&path) {
                Ok(skill) => skills.push(skill),
                Err(e) => {
                    eprintln!("Warning: skipping skill directory {}: {e}", path.display());
                }
            }
        }
    }
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_frontmatter() {
        let input = "---\nname: test-skill\ndescription: A test skill\n---\n\nBody content.";
        let (fm, body) = parse_frontmatter(input).unwrap();
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.description.unwrap(), "A test skill");
        assert_eq!(body, "Body content.");
    }

    #[test]
    fn parse_frontmatter_with_allowed_tools() {
        let input = "---\nname: test-skill\ndescription: desc\nallowed-tools:\n  - bash\n  - read\n---\n\nBody.";
        let (fm, _body) = parse_frontmatter(input).unwrap();
        assert_eq!(
            fm.allowed_tools.unwrap(),
            vec!["bash".to_string(), "read".to_string()]
        );
    }

    #[test]
    fn parse_missing_frontmatter() {
        let input = "No frontmatter here";
        assert!(parse_frontmatter(input).is_err());
    }

    #[test]
    fn parse_unclosed_frontmatter() {
        let input = "---\nname: test-skill\n";
        assert!(parse_frontmatter(input).is_err());
    }
}
