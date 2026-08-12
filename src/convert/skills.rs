use std::path::Path;

use crate::agent_plugins::skill::AgentSkill;
use crate::claude::skill::Skill;
use crate::error::Result;
use crate::validate::skill::validate_skill;

pub fn convert(
    _input_dir: &Path,
    output_dir: &Path,
    claude_skills: &[Skill],
) -> Result<Vec<crate::convert::ConvertedSkill>> {
    let mut converted = Vec::new();

    for skill in claude_skills {
        let warnings = validate_skill(skill);

        let mut metadata = skill.frontmatter.metadata.clone().unwrap_or_default();
        for (key, value) in &skill.frontmatter.extra {
            if let Ok(json_val) = serde_json::to_value(value) {
                metadata.insert(key.clone(), json_val);
            }
        }
        let metadata = if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        };

        let agent_skill = AgentSkill {
            name: skill.dir_name.clone(),
            description: skill.frontmatter.description.clone().unwrap_or_default(),
            license: skill.frontmatter.license.clone(),
            compatibility: skill.frontmatter.compatibility.clone(),
            metadata,
            allowed_tools: skill.frontmatter.allowed_tools.clone(),
        };

        crate::agent_plugins::skill::write_skill(
            &agent_skill,
            &skill.body,
            output_dir,
            &skill.dir_name,
        )?;

        converted.push(crate::convert::ConvertedSkill {
            path: output_dir.join("skills").join(&skill.dir_name),
            warnings,
        });
    }

    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::skill::{Skill, SkillFrontmatter};
    use std::path::PathBuf;

    #[test]
    fn convert_single_skill() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        let skill = Skill {
            dir_name: "my-skill".to_string(),
            path: PathBuf::from("skills/my-skill"),
            frontmatter: SkillFrontmatter {
                name: "my-skill".to_string(),
                description: Some("A test skill".to_string()),
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                extra: std::collections::HashMap::new(),
            },
            body: "Body content".to_string(),
            raw: "---\nname: my-skill\ndescription: A test skill\n---\n\nBody content".to_string(),
        };

        let result = convert(temp.path(), &output, &[skill]).unwrap();
        assert_eq!(result.len(), 1);
        let skill_md = output.join("skills/my-skill/SKILL.md");
        assert!(skill_md.exists());
        let content = std::fs::read_to_string(skill_md).unwrap();
        assert!(content.contains("name: my-skill"));
        assert!(content.contains("Body content"));
    }

    #[test]
    fn handles_missing_description() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out2");

        let skill = Skill {
            dir_name: "no-desc".to_string(),
            path: PathBuf::from("skills/no-desc"),
            frontmatter: SkillFrontmatter {
                name: "no-desc".to_string(),
                description: None,
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                extra: std::collections::HashMap::new(),
            },
            body: String::new(),
            raw: String::new(),
        };

        let result = convert(temp.path(), &output, &[skill]).unwrap();
        assert!(!result[0].warnings.is_empty());
    }
}
