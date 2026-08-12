pub struct EnvVarResult {
    pub value: String,
    pub warnings: Vec<String>,
}

pub fn transform_env_value(value: &str) -> EnvVarResult {
    let result = value
        .replace("${CLAUDE_PLUGIN_ROOT}", "${PLUGIN_ROOT}")
        .replace("${CLAUDE_PLUGIN_DATA}", "${PLUGIN_DATA}");

    let mut warnings = Vec::new();
    if result.contains("${CLAUDE_PROJECT_DIR}") {
        warnings.push("${CLAUDE_PROJECT_DIR} has no Agent Plugins equivalent".to_string());
    }

    EnvVarResult {
        value: result,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_plugin_root() {
        let result = transform_env_value("${CLAUDE_PLUGIN_ROOT}/data");
        assert_eq!(result.value, "${PLUGIN_ROOT}/data");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn replaces_plugin_data() {
        let result = transform_env_value("${CLAUDE_PLUGIN_DATA}/cache");
        assert_eq!(result.value, "${PLUGIN_DATA}/cache");
    }

    #[test]
    fn replaces_multiple() {
        let result = transform_env_value("${CLAUDE_PLUGIN_ROOT}/a:${CLAUDE_PLUGIN_DATA}/b");
        assert_eq!(result.value, "${PLUGIN_ROOT}/a:${PLUGIN_DATA}/b");
    }

    #[test]
    fn preserves_other_vars() {
        let result = transform_env_value("${HOME}/plugin");
        assert_eq!(result.value, "${HOME}/plugin");
    }

    #[test]
    fn no_change_for_non_claude_vars() {
        let result = transform_env_value("plain string");
        assert_eq!(result.value, "plain string");
    }

    #[test]
    fn warns_on_project_dir() {
        let result = transform_env_value("${CLAUDE_PROJECT_DIR}/test");
        assert_eq!(result.value, "${CLAUDE_PROJECT_DIR}/test");
        assert!(!result.warnings.is_empty());
    }
}
