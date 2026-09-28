use std::sync::OnceLock;

use regex::Regex;

fn user_config_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{user_config\.([A-Za-z_][A-Za-z0-9_]*)\}").unwrap())
}

pub struct Rewrite {
    pub value: String,
    pub warnings: Vec<String>,
}

/// Rewrite Claude plugin placeholders into the forms Cursor documents.
///
/// `${CLAUDE_PLUGIN_ROOT}` becomes `${CURSOR_PLUGIN_ROOT}` (Cursor expands both
/// in `mcp.json`, and the Cursor name is the one its plugin docs tell authors
/// to use). `${user_config.KEY}` becomes `${KEY}` so it can match a manifest
/// `variables` property. Plugin-data and project-dir placeholders are left in
/// place and warned about.
pub fn rewrite_placeholders(value: &str, context: &str) -> Rewrite {
    let mut warnings = Vec::new();
    let with_root = value.replace("${CLAUDE_PLUGIN_ROOT}", "${CURSOR_PLUGIN_ROOT}");

    if with_root.contains("${CLAUDE_PLUGIN_DATA}") {
        warnings.push(format!(
            "{context}: ${{CLAUDE_PLUGIN_DATA}} has no Cursor equivalent; left unchanged (original preserved in the sidecar)"
        ));
    }
    if with_root.contains("${CLAUDE_PROJECT_DIR}") {
        warnings.push(format!(
            "{context}: ${{CLAUDE_PROJECT_DIR}} is not expanded in Cursor plugin config (Cursor only aliases it for hooks); left unchanged"
        ));
    }

    let mut saw_user_config = false;
    let rewritten = user_config_re().replace_all(&with_root, |caps: &regex::Captures| {
        saw_user_config = true;
        format!("${{{}}}", &caps[1])
    });
    if saw_user_config {
        warnings.push(format!(
            "{context}: ${{user_config.KEY}} rewritten to ${{KEY}} for Cursor plugin variables; set the value in the dashboard (Plugins → Configure), not in the plugin repo"
        ));
    }

    Rewrite {
        value: rewritten.into_owned(),
        warnings,
    }
}

/// Plugin hook commands in Cursor's plugin example are `./`-relative to the
/// plugin root. Collapse a leading plugin-root placeholder to that form.
pub fn rewrite_hook_command(command: &str, context: &str) -> Rewrite {
    let trimmed = command.trim();
    let collapsed = trimmed
        .replace("\"${CLAUDE_PLUGIN_ROOT}\"/", "./")
        .replace("\"${CLAUDE_PLUGIN_ROOT}\"", ".")
        .replace("${CLAUDE_PLUGIN_ROOT}/", "./")
        .replace("${CLAUDE_PLUGIN_ROOT}", ".");
    rewrite_placeholders(&collapsed, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_plugin_root_and_user_config() {
        let result = rewrite_placeholders(
            "${CLAUDE_PLUGIN_ROOT}/bin/server ${user_config.api_token}",
            "mcp",
        );
        assert_eq!(
            result.value,
            "${CURSOR_PLUGIN_ROOT}/bin/server ${api_token}"
        );
        assert!(result.warnings.iter().any(|w| w.contains("user_config")));
    }

    #[test]
    fn warns_on_plugin_data_and_project_dir() {
        let result = rewrite_placeholders("${CLAUDE_PLUGIN_DATA}:${CLAUDE_PROJECT_DIR}", "mcp");
        assert!(result.value.contains("${CLAUDE_PLUGIN_DATA}"));
        assert!(result.value.contains("${CLAUDE_PROJECT_DIR}"));
        assert_eq!(result.warnings.len(), 2);
    }

    #[test]
    fn collapses_hook_command_root() {
        let result = rewrite_hook_command("\"${CLAUDE_PLUGIN_ROOT}\"/scripts/check.sh", "hook");
        assert_eq!(result.value, "./scripts/check.sh");
    }
}
