use regex::Regex;

pub fn is_valid_plugin_name(name: &str) -> bool {
    let re = Regex::new(r"^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$").unwrap();
    !name.is_empty()
        && name.len() <= 64
        && !name.contains("--")
        && !name.contains("..")
        && re.is_match(name)
}

pub fn is_valid_extension_namespace(ns: &str) -> bool {
    if ns.is_empty() || ns.len() > 128 {
        return false;
    }
    ns.split('.').all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }) && !ns.contains("..")
        && !ns.starts_with('.')
        && !ns.ends_with('.')
}

pub fn is_valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.contains("--")
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn normalize_skill_name(name: &str) -> String {
    let mut result = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    while result.contains("--") {
        result = result.replace("--", "-");
    }
    let result = result.trim_matches('-').to_string();
    if result.is_empty() {
        "unnamed-skill".to_string()
    } else {
        result
    }
}

pub fn normalize_name(name: &str) -> String {
    let result = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '.' => c,
            '_' | ' ' => '-',
            _ => '-',
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if result.is_empty() {
        "unnamed-plugin".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_simple_name() {
        assert!(is_valid_plugin_name("my-plugin"));
    }

    #[test]
    fn valid_with_dots() {
        assert!(is_valid_plugin_name("com.example.plugin"));
    }

    #[test]
    fn invalid_double_hyphen() {
        assert!(!is_valid_plugin_name("my--plugin"));
    }

    #[test]
    fn invalid_double_dot() {
        assert!(!is_valid_plugin_name("my..plugin"));
    }

    #[test]
    fn invalid_starts_with_hyphen() {
        assert!(!is_valid_plugin_name("-my-plugin"));
    }

    #[test]
    fn invalid_too_long() {
        let long_name = "a".repeat(65);
        assert!(!is_valid_plugin_name(&long_name));
    }

    #[test]
    fn invalid_empty() {
        assert!(!is_valid_plugin_name(""));
    }

    #[test]
    fn normalize_uppercase() {
        assert_eq!(normalize_name("My-Plugin"), "my-plugin");
    }

    #[test]
    fn normalize_spaces() {
        assert_eq!(normalize_name("my plugin"), "my-plugin");
    }

    #[test]
    fn normalize_underscores() {
        assert_eq!(normalize_name("my_plugin"), "my-plugin");
    }

    #[test]
    fn normalize_leading_trailing() {
        assert_eq!(normalize_name("-my-plugin-"), "my-plugin");
    }

    #[test]
    fn normalize_all_special_chars() {
        assert_eq!(normalize_name("!!!"), "unnamed-plugin");
    }

    #[test]
    fn valid_64_chars() {
        let name = "a".repeat(64);
        assert!(is_valid_plugin_name(&name));
    }

    #[test]
    fn valid_extension_namespace() {
        assert!(is_valid_extension_namespace("com.example.plugin"));
    }

    #[test]
    fn invalid_namespace_with_dots() {
        assert!(!is_valid_extension_namespace("com..example"));
    }

    #[test]
    fn invalid_namespace_empty() {
        assert!(!is_valid_extension_namespace(""));
    }

    #[test]
    fn invalid_namespace_starts_with_dot() {
        assert!(!is_valid_extension_namespace(".com.example"));
    }

    #[test]
    fn invalid_namespace_empty_part() {
        assert!(!is_valid_extension_namespace("com..plugin"));
    }

    #[test]
    fn valid_namespace_with_hyphen() {
        assert!(is_valid_extension_namespace("com.claude-code"));
    }

    #[test]
    fn invalid_namespace_too_long() {
        let ns = "x".repeat(129);
        assert!(!is_valid_extension_namespace(&ns));
    }

    #[test]
    fn invalid_namespace_slash() {
        assert!(!is_valid_extension_namespace("../../etc"));
    }

    #[test]
    fn valid_skill_names() {
        assert!(is_valid_skill_name("pdf-processing"));
        assert!(is_valid_skill_name("data-analysis"));
        assert!(is_valid_skill_name("a"));
    }

    #[test]
    fn invalid_skill_names() {
        assert!(!is_valid_skill_name("My-Skill"));
        assert!(!is_valid_skill_name("-leading"));
        assert!(!is_valid_skill_name("double--hyphen"));
        assert!(!is_valid_skill_name("has.period"));
        assert!(!is_valid_skill_name(""));
        assert!(!is_valid_skill_name(&"a".repeat(65)));
    }

    #[test]
    fn normalize_skill_names() {
        assert_eq!(normalize_skill_name("My-Skill"), "my-skill");
        assert_eq!(normalize_skill_name("has.period"), "has-period");
        assert_eq!(normalize_skill_name("under_score"), "under-score");
        assert_eq!(normalize_skill_name("---"), "unnamed-skill");
        assert_eq!(normalize_skill_name("a--b"), "a-b");
    }
}
