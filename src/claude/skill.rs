use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum AllowedTools {
    Single(String),
    Multiple(Vec<String>),
}

impl AllowedTools {
    /// Agent Skills defines allowed-tools as a space-separated string.
    pub fn to_agent_string(&self) -> String {
        match self {
            AllowedTools::Single(s) => s.clone(),
            AllowedTools::Multiple(v) => v.join(" "),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillFrontmatter {
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "allowed-tools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<AllowedTools>,
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
    parse_all_with_dirs(plugin_dir, &[])
}

pub fn parse_all_with_dirs(plugin_dir: &Path, extra_dirs: &[PathBuf]) -> Result<Vec<Skill>> {
    let mut skills = Vec::new();

    let mut dirs = Vec::new();
    let skills_dir = plugin_dir.join("skills");
    if skills_dir.is_dir() {
        dirs.push(skills_dir);
    }
    dirs.extend(extra_dirs.iter().map(|d| {
        if d.is_absolute() {
            d.clone()
        } else {
            plugin_dir.join(d)
        }
    }));

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
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
    }
    Ok(skills)
}

pub struct CommandFile {
    pub frontmatter: Option<SkillFrontmatter>,
    pub body: String,
    pub raw: String,
}

pub fn parse_command_file(path: &Path) -> Result<CommandFile> {
    let raw = std::fs::read_to_string(path)?;
    if raw.trim_start().starts_with("---")
        && let Ok((frontmatter, body)) = parse_frontmatter(&raw)
    {
        return Ok(CommandFile {
            frontmatter: Some(frontmatter),
            body,
            raw,
        });
    }
    Ok(CommandFile {
        frontmatter: None,
        body: raw.trim().to_string(),
        raw,
    })
}

pub fn command_description(cmd: &CommandFile, fallback_name: &str) -> String {
    if let Some(fm) = &cmd.frontmatter
        && let Some(desc) = &fm.description
    {
        return desc.clone();
    }
    let first_line = cmd
        .body
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or(fallback_name);
    let truncated: String = first_line.chars().take(1024).collect();
    truncated
}

pub fn parse_root_skill(plugin_dir: &Path, fallback_name: &str) -> Result<Option<Skill>> {
    let skill_md = plugin_dir.join("SKILL.md");
    if !skill_md.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&skill_md)?;
    let (frontmatter, body) = parse_frontmatter(&raw)?;

    let dir_name = match &frontmatter.name {
        Some(n) if crate::validate::name::is_valid_skill_name(n) => n.clone(),
        _ => fallback_name.to_string(),
    };

    Ok(Some(Skill {
        dir_name,
        path: plugin_dir.to_path_buf(),
        frontmatter,
        body,
        raw,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_frontmatter() {
        let input = "---\nname: test-skill\ndescription: A test skill\n---\n\nBody content.";
        let (fm, body) = parse_frontmatter(input).unwrap();
        assert_eq!(fm.name.as_deref(), Some("test-skill"));
        assert_eq!(fm.description.unwrap(), "A test skill");
        assert_eq!(body, "Body content.");
    }

    #[test]
    fn parse_frontmatter_without_name() {
        let input = "---\ndescription: A test skill\n---\n\nBody content.";
        let (fm, _body) = parse_frontmatter(input).unwrap();
        assert!(fm.name.is_none());
    }

    #[test]
    fn parse_frontmatter_with_allowed_tools_array() {
        let input = "---\nname: test-skill\ndescription: desc\nallowed-tools:\n  - bash\n  - read\n---\n\nBody.";
        let (fm, _body) = parse_frontmatter(input).unwrap();
        assert_eq!(fm.allowed_tools.unwrap().to_agent_string(), "bash read");
    }

    #[test]
    fn parse_frontmatter_with_allowed_tools_string() {
        let input = "---\nname: test-skill\ndescription: desc\nallowed-tools: Bash Read Write Edit\n---\n\nBody.";
        let (fm, _body) = parse_frontmatter(input).unwrap();
        assert_eq!(
            fm.allowed_tools.unwrap().to_agent_string(),
            "Bash Read Write Edit"
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

    #[test]
    fn parse_root_skill_uses_frontmatter_name() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: root-skill\ndescription: desc\n---\n\nBody.",
        )
        .unwrap();
        let skill = parse_root_skill(temp.path(), "fallback").unwrap().unwrap();
        assert_eq!(skill.dir_name, "root-skill");
        assert_eq!(skill.body, "Body.");
    }

    #[test]
    fn parse_root_skill_falls_back_to_dir_name() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\ndescription: desc\n---\n\nBody.",
        )
        .unwrap();
        let skill = parse_root_skill(temp.path(), "my-plugin").unwrap().unwrap();
        assert_eq!(skill.dir_name, "my-plugin");
    }

    #[test]
    fn parse_root_skill_missing_returns_none() {
        let temp = tempfile::tempdir().unwrap();
        assert!(parse_root_skill(temp.path(), "x").unwrap().is_none());
    }

    #[test]
    fn parse_command_with_frontmatter() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("deploy.md");
        std::fs::write(
            &path,
            "---\nname: deploy\ndescription: Deploy to prod\n---\n\nRun the deploy.",
        )
        .unwrap();
        let cmd = parse_command_file(&path).unwrap();
        assert_eq!(
            cmd.frontmatter.as_ref().unwrap().name.as_deref(),
            Some("deploy")
        );
        assert_eq!(cmd.body, "Run the deploy.");
        assert_eq!(command_description(&cmd, "deploy"), "Deploy to prod");
    }

    #[test]
    fn parse_command_without_frontmatter() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("status.md");
        std::fs::write(&path, "# Status\n\nShow the current status.").unwrap();
        let cmd = parse_command_file(&path).unwrap();
        assert!(cmd.frontmatter.is_none());
        assert_eq!(
            command_description(&cmd, "status"),
            "Show the current status."
        );
    }

    #[test]
    fn parse_all_with_custom_dirs() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("skills").join("default")).unwrap();
        std::fs::write(
            temp.path().join("skills").join("default").join("SKILL.md"),
            "---\nname: default\ndescription: d\n---\n\nb",
        )
        .unwrap();
        std::fs::create_dir_all(temp.path().join("custom").join("extra")).unwrap();
        std::fs::write(
            temp.path().join("custom").join("extra").join("SKILL.md"),
            "---\nname: extra\ndescription: d\n---\n\nb",
        )
        .unwrap();

        let skills = parse_all_with_dirs(temp.path(), &[PathBuf::from("custom")]).unwrap();
        let names: Vec<&str> = skills.iter().map(|s| s.dir_name.as_str()).collect();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"extra"));
    }
}
