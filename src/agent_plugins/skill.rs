use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{Error, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentSkill {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(rename = "allowed-tools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<String>,
}

pub fn generate_skill_md(skill: &AgentSkill, body: &str) -> Result<String> {
    let frontmatter = serde_yaml::to_string(skill).map_err(Error::Yaml)?;
    Ok(format!("---\n{}---\n\n{}", frontmatter, body))
}

pub fn write_skill(
    skill: &AgentSkill,
    body: &str,
    output_dir: &Path,
    skill_name: &str,
) -> Result<()> {
    let skill_dir = output_dir.join("skills").join(skill_name);
    std::fs::create_dir_all(&skill_dir)?;
    let content = generate_skill_md(skill, body)?;
    std::fs::write(skill_dir.join("SKILL.md"), content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_minimal_skill_md() {
        let skill = AgentSkill {
            name: "my-skill".to_string(),
            description: "A test skill".to_string(),
            license: None,
            compatibility: None,
            metadata: None,
            allowed_tools: None,
        };

        let output = generate_skill_md(&skill, "Body text.").unwrap();
        assert!(output.contains("name: my-skill"));
        assert!(output.contains("description: A test skill"));
        assert!(output.contains("Body text."));
    }

    #[test]
    fn generate_full_skill_md() {
        let skill = AgentSkill {
            name: "my-skill".to_string(),
            description: "Test".to_string(),
            license: Some("MIT".to_string()),
            compatibility: Some(">=1.0.0".to_string()),
            metadata: None,
            allowed_tools: Some("Bash(git:*) Read".to_string()),
        };

        let output = generate_skill_md(&skill, "Body.").unwrap();
        assert!(output.contains("allowed-tools: Bash(git:*) Read"));
    }
}
