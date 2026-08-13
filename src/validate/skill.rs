use crate::claude::skill::Skill;
use crate::validate::name;

pub fn validate_skill(skill: &Skill) -> Vec<String> {
    let mut warnings = Vec::new();
    let expected_name = name::normalize_name(&skill.dir_name);

    if let Some(name) = &skill.frontmatter.name
        && *name != expected_name
    {
        warnings.push(format!(
            "Skill '{}': frontmatter name '{}' does not match expected name '{}'",
            skill.dir_name, name, expected_name
        ));
    }

    if skill.frontmatter.description.is_none() {
        warnings.push(format!(
            "Skill '{}': missing description in frontmatter",
            skill.dir_name
        ));
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::skill::{Skill, SkillFrontmatter};
    use std::path::PathBuf;

    fn make_skill(dir_name: &str, fm_name: &str, desc: Option<&str>) -> Skill {
        Skill {
            dir_name: dir_name.to_string(),
            path: PathBuf::from(dir_name),
            frontmatter: SkillFrontmatter {
                name: Some(fm_name.to_string()),
                description: desc.map(|s| s.to_string()),
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
                extra: std::collections::HashMap::new(),
            },
            body: String::new(),
            raw: String::new(),
        }
    }

    #[test]
    fn valid_skill_no_warnings() {
        let skill = make_skill("my-skill", "my-skill", Some("description"));
        let warnings = validate_skill(&skill);
        assert!(warnings.is_empty());
    }

    #[test]
    fn warns_on_name_mismatch() {
        let skill = make_skill("my-skill", "wrong-name", Some("desc"));
        let warnings = validate_skill(&skill);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("does not match"));
    }

    #[test]
    fn warns_on_missing_description() {
        let skill = make_skill("my-skill", "my-skill", None);
        let warnings = validate_skill(&skill);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("missing description"));
    }
}
