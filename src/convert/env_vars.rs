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
    if result.contains("${user_config.") {
        warnings.push(
            "${user_config.*} has no Agent Plugins equivalent; values must be supplied another way"
                .to_string(),
        );
    }

    EnvVarResult {
        value: result,
        warnings,
    }
}

pub fn transform_command(command: &str) -> EnvVarResult {
    if command == "${CLAUDE_PLUGIN_ROOT}" {
        return EnvVarResult {
            value: "./".to_string(),
            warnings: Vec::new(),
        };
    }
    if let Some(rest) = command.strip_prefix("${CLAUDE_PLUGIN_ROOT}/") {
        return EnvVarResult {
            value: format!("./{}", rest),
            warnings: Vec::new(),
        };
    }

    let mut warnings = Vec::new();
    if command.contains("${CLAUDE_PLUGIN_ROOT}")
        || command.contains("${CLAUDE_PLUGIN_DATA}")
        || command.contains("${CLAUDE_PROJECT_DIR}")
        || command.contains("${user_config.")
    {
        warnings.push(format!(
            "command '{}' is not a bare executable name or './'-relative path; Agent Plugins does not expand placeholders in command",
            command
        ));
    }

    EnvVarResult {
        value: command.to_string(),
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

    #[test]
    fn warns_on_user_config() {
        let result = transform_env_value("${user_config.api_token}");
        assert!(result.value.contains("${user_config.api_token}"));
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn rewrites_command_root_prefix() {
        let result = transform_command("${CLAUDE_PLUGIN_ROOT}/servers/db-server");
        assert_eq!(result.value, "./servers/db-server");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn rewrites_command_root_exact() {
        let result = transform_command("${CLAUDE_PLUGIN_ROOT}");
        assert_eq!(result.value, "./");
    }

    #[test]
    fn keeps_bare_command() {
        let result = transform_command("npx");
        assert_eq!(result.value, "npx");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn warns_on_placeholder_in_command() {
        let result = transform_command("node ${CLAUDE_PLUGIN_ROOT}/server.js");
        assert_eq!(result.value, "node ${CLAUDE_PLUGIN_ROOT}/server.js");
        assert!(!result.warnings.is_empty());
    }
}
